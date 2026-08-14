//! The UIR stepper and live graph.  Loads a UIR program, keeps it as the
//! source of truth for running work, computes readiness from dataflow
//! dependencies, and executes named nodes on the boot CPU (`engine = p-core`,
//! `home = uma`).  Effects go through propose → simulate → validate → commit.

use uir::*;

#[derive(Clone)]
pub struct Value {
    pub data: usize,
    pub dtype: u8,
    pub rank: u8,
    pub shape: [usize; 4],
    pub region: Option<usize>,
}

impl Value {
    pub fn elems(&self) -> usize {
        elems(self.rank, &self.shape)
    }
    pub fn elem_size(&self) -> usize {
        dtype_size(self.dtype)
    }
    pub fn byte_len(&self) -> usize {
        self.elems() * self.elem_size()
    }
}

fn elems(rank: u8, shape: &[usize; 4]) -> usize {
    let mut n = 1usize;
    for i in 0..rank as usize {
        n *= shape[i];
    }
    n
}

fn to_usize_shape(rank: u8, s: &[u32; 4]) -> [usize; 4] {
    let mut o = [1usize; 4];
    for i in 0..rank as usize {
        o[i] = s[i] as usize;
    }
    o
}

pub fn scalar_value() -> Value {
    Value { data: 0, dtype: DTYPE_I64, rank: 0, shape: [1, 1, 1, 1], region: None }
}

/// The live graph: a borrowed UIR program plus per-node execution state.
pub struct Graph<'a> {
    pub prog: &'a Program,
    pub vals: alloc::vec::Vec<Option<Value>>,
    pub done: alloc::vec::Vec<bool>,
    pub consumers: alloc::vec::Vec<u32>,
    pub ready_input: Option<alloc::vec::Vec<[i64; 2]>>, // for ReadySet (policy)
    pub order_result: alloc::vec::Vec<u32>,             // for Order (policy)
    pub node_bytes: alloc::vec::Vec<u64>,               // per-node payload bytes moved
}

impl<'a> Graph<'a> {
    pub fn new(prog: &'a Program) -> Self {
        let n = prog.nodes.len();
        let mut consumers = alloc::vec![0u32; n];
        for nd in prog.nodes.iter() {
            for inp in [nd.in0, nd.in1, nd.in2] {
                if inp != NONE {
                    consumers[inp as usize] += 1;
                }
            }
        }
        Graph {
            prog,
            vals: (0..n).map(|_| None).collect(),
            done: alloc::vec![false; n],
            consumers,
            ready_input: None,
            order_result: alloc::vec::Vec::new(),
            node_bytes: alloc::vec![0u64; n],
        }
    }

    pub fn n(&self) -> usize {
        self.prog.nodes.len()
    }

    pub fn ready_set(&self) -> alloc::vec::Vec<u32> {
        let mut r = alloc::vec::Vec::new();
        for i in 0..self.n() {
            if self.done[i] {
                continue;
            }
            if self.inputs_ready(i) {
                r.push(i as u32);
            }
        }
        r
    }

    fn inputs_ready(&self, i: usize) -> bool {
        let nd = &self.prog.nodes[i];
        for inp in [nd.in0, nd.in1, nd.in2] {
            if inp != NONE && self.vals[inp as usize].is_none() {
                return false;
            }
        }
        true
    }

    pub fn node_table(&self) -> alloc::vec::Vec<[i64; 9]> {
        let mut t = alloc::vec::Vec::with_capacity(self.n());
        for (i, nd) in self.prog.nodes.iter().enumerate() {
            let ready = !self.done[i] && self.inputs_ready(i);
            t.push([
                i as i64,
                nd.op as i64,
                if nd.pure { 1 } else { 0 },
                nd.engine as i64,
                nd.home as i64,
                shape_elems(nd.rank, &nd.shape) as i64,
                if ready { 1 } else { 0 },
                nd.cap_need as i64,
                self.node_bytes[i] as i64,
            ]);
        }
        t
    }

    pub fn edge_table(&self) -> alloc::vec::Vec<[i64; 2]> {
        let mut t = alloc::vec::Vec::new();
        for nd in self.prog.nodes.iter() {
            for inp in [nd.in0, nd.in1, nd.in2] {
                if inp != NONE {
                    t.push([inp as i64, nd.id as i64]);
                }
            }
        }
        t
    }

    /// Node names as a rank-2 u8 table [n, maxlen], NUL-padded.
    pub fn names_table(&self) -> alloc::vec::Vec<u8> {
        let n = self.n();
        let mut maxlen = 1usize;
        for name in self.prog.names.iter() {
            if name.len() > maxlen {
                maxlen = name.len();
            }
        }
        let mut t = alloc::vec![0u8; n * maxlen];
        for (i, name) in self.prog.names.iter().enumerate() {
            for (j, b) in name.iter().enumerate() {
                t[i * maxlen + j] = *b;
            }
        }
        t
    }

    pub fn names_stride(&self) -> usize {
        let mut maxlen = 1usize;
        for name in self.prog.names.iter() {
            if name.len() > maxlen {
                maxlen = name.len();
            }
        }
        maxlen
    }

    /// Free every owned region still held by this graph (explicit lifetime;
    /// there is no garbage collector).
    pub fn release_all(&mut self) {
        for v in self.vals.iter_mut() {
            if let Some(val) = v.take() {
                if let Some(region) = val.region {
                    crate::kernel::free_region(0, region);
                }
            }
        }
    }
}

pub struct RunOpts<'a> {
    pub realm: u32,
    pub live: Option<&'a Graph<'a>>, // for GraphNodes/Edges sources
    pub policy: Option<&'a Program>, // ready-set ordering policy
    pub interactive: bool,
}

/// Run a program to completion.  Returns Err on the first runtime error.
pub fn run(g: &mut Graph, opts: &RunOpts) -> Result<(), &'static str> {
    loop {
        let ready = g.ready_set();
        if ready.is_empty() {
            break;
        }
        let order = order_ready(g, opts, &ready);
        for id in order {
            let id = id as usize;
            if g.done[id] {
                continue;
            }
            let r = step_node(g, opts, id);
            match r {
                Ok(v) => {
                    g.vals[id] = v;
                    g.done[id] = true;
                    release_inputs(g, id);
                }
                Err(e) => {
                    // A defined runtime error: surface it; the Realm stays
                    // idle (no reset loop).  The policy runner ignores errors
                    // and falls back to the default order.
                    g.done[id] = true;
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

fn order_ready(_g: &Graph, opts: &RunOpts, ready: &[u32]) -> alloc::vec::Vec<u32> {
    match opts.policy {
        None => ready.to_vec(),
        Some(prog) => {
            let table: alloc::vec::Vec<[i64; 2]> =
                ready.iter().map(|id| [*id as i64, *id as i64]).collect();
            let mut pg = Graph::new(prog);
            pg.ready_input = Some(table);
            let popts = RunOpts { realm: opts.realm, live: None, policy: None, interactive: false };
            let _ = run(&mut pg, &popts);
            if pg.order_result.is_empty() {
                ready.to_vec()
            } else {
                pg.order_result.clone()
            }
        }
    }
}

fn release_inputs(g: &mut Graph, id: usize) {
    let nd = &g.prog.nodes[id];
    let ins = [nd.in0, nd.in1, nd.in2];
    for inp in ins {
        if inp == NONE {
            continue;
        }
        let inp = inp as usize;
        if g.consumers[inp] > 0 {
            g.consumers[inp] -= 1;
        }
        if g.consumers[inp] == 0 {
            if let Some(v) = g.vals[inp].take() {
                if let Some(region) = v.region {
                    crate::kernel::free_region(0, region);
                }
            }
        }
    }
}

fn input(g: &Graph, nd: &NodeDesc, slot: usize) -> Result<Value, &'static str> {
    let idx = match slot {
        0 => nd.in0,
        1 => nd.in1,
        _ => nd.in2,
    };
    if idx == NONE {
        Ok(scalar_value())
    } else {
        g.vals[idx as usize].clone().ok_or("missing input")
    }
}

fn alloc_array(dtype: u8, rank: u8, shape: &[usize; 4]) -> Result<Value, &'static str> {
    let n = elems(rank, shape);
    let bytes = n * dtype_size(dtype);
    let region = crate::kernel::alloc_region(0, bytes).ok_or("alloc failed")?;
    let data = crate::kernel::region_base(region).ok_or("bad region")?;
    let mut sh = [1usize; 4];
    sh[..rank as usize].copy_from_slice(&shape[..rank as usize]);
    Ok(Value { data, dtype, rank, shape: sh, region: Some(region) })
}

fn step_node(g: &mut Graph, opts: &RunOpts, id: usize) -> Result<Option<Value>, &'static str> {
    let nd = &g.prog.nodes[id];
    let c = &g.prog.consts[id];
    match nd.op {
        OP_CONST => {
            if nd.const_len > 0 && c[0] == 0xFF {
                // fill descriptor
                if let Some((dtype, rank, shape, fill)) = decode_fill(c) {
                    let us = to_usize_shape(rank, &shape);
                    return fill_array(dtype, rank, &us, fill);
                }
            }
            let us = to_usize_shape(nd.rank, &nd.shape);
            let mut v = alloc_array(nd.dtype, nd.rank, &us)?;
            let bytes = v.byte_len();
            if bytes > 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(c.as_ptr(), v.data as *mut u8, bytes.min(c.len()));
                }
            }
            Ok(Some(v))
        }
        OP_FILL => {
            let (dtype, rank, shape, fill) = decode_fill(c).ok_or("bad fill")?;
            let us = to_usize_shape(rank, &shape);
            fill_array(dtype, rank, &us, fill)
        }
        OP_ADD | OP_SUB | OP_MUL | OP_DIV => {
            let a = input(g, nd, 0)?;
            let b = input(g, nd, 1)?;
            let r = elementwise(nd.op, &a, &b)?;
            if let Some(v) = &r {
                unsafe {
                    let moved = (v.byte_len() + a.byte_len() + b.byte_len()) as u64;
                    crate::kernel::COUNTERS.payload_moved += moved;
                    crate::kernel::COUNTERS.kernel_entries += 1;
                    g.node_bytes[id] += moved;
                }
            }
            Ok(r)
        }
        OP_ADD_MUL => {
            let a = input(g, nd, 0)?;
            let b = input(g, nd, 1)?;
            let d = input(g, nd, 2)?;
            let r = elementwise_fused(&a, &b, &d)?;
            if let Some(v) = &r {
                unsafe {
                    let moved = (v.byte_len() + a.byte_len() + b.byte_len() + d.byte_len()) as u64;
                    crate::kernel::COUNTERS.payload_moved += moved;
                    crate::kernel::COUNTERS.kernel_entries += 1;
                    g.node_bytes[id] += moved;
                }
            }
            Ok(r)
        }
        OP_NEG => {
            let a = input(g, nd, 0)?;
            let mut v = alloc_array(a.dtype, a.rank, &a.shape)?;
            let n = v.elems();
            unsafe {
                let p = v.data as *mut u8;
                match a.dtype {
                    DTYPE_F32 => {
                        for i in 0..n {
                            *(p.add(i * 4) as *mut f32) = -(*(a.data as *const f32).add(i));
                        }
                    }
                    _ => {
                        for i in 0..n {
                            *(p.add(i * 8) as *mut i64) = -(*(a.data as *const i64).add(i));
                        }
                    }
                }
            }
            Ok(Some(v))
        }
        OP_REDUCE_SUM => {
            let a = input(g, nd, 0)?;
            reduce_sum(&a)
        }
        OP_RESHAPE => {
            let a = input(g, nd, 0)?;
            // metadata-only: shares the region, swaps the shape
            if let Some(region) = a.region {
                crate::kernel::region_addref(region);
            }
            let mut v = a.clone();
            v.rank = nd.rank;
            v.shape = [1, 1, 1, 1];
            for i in 0..nd.rank as usize {
                v.shape[i] = nd.shape[i] as usize;
            }
            Ok(Some(v))
        }
        OP_ROWS => {
            // rank-wise map marker: the leading axis is independent (16 rows
            // are 16 units); the stepper still runs them in order, but the
            // parallel axis is recorded and the region is shared, not copied.
            let a = input(g, nd, 0)?;
            if let Some(region) = a.region {
                crate::kernel::region_addref(region);
            }
            Ok(Some(a))
        }
        OP_REVERSE => {
            let a = input(g, nd, 0)?;
            let mut v = alloc_array(a.dtype, a.rank, &a.shape)?;
            let n = v.elems();
            let es = v.elem_size();
            unsafe {
                for i in 0..n {
                    core::ptr::copy_nonoverlapping(
                        (a.data + (n - 1 - i) * es) as *const u8,
                        (v.data + i * es) as *mut u8,
                        es,
                    );
                }
            }
            Ok(Some(v))
        }
        OP_DISPLAY => {
            let a = input(g, nd, 0)?;
            display_effect(opts, &a)
        }
        OP_KEYBOARD => {
            let line = crate::devices::read_line();
            let mut v = alloc_array(DTYPE_U8, 1, &[line.len(), 1, 1, 1])?;
            unsafe {
                core::ptr::copy_nonoverlapping(line.as_ptr(), v.data as *mut u8, line.len());
            }
            Ok(Some(v))
        }
        OP_GRAPH_NODES => match opts.live {
            Some(live) => {
                let table = live.node_table();
                let rows = table.len();
                let mut v = alloc_array(DTYPE_I64, 2, &[rows, 9, 1, 1])?;
                unsafe {
                    let p = v.data as *mut i64;
                    for (r, row) in table.iter().enumerate() {
                        for (cc, val) in row.iter().enumerate() {
                            *p.add(r * 9 + cc) = *val;
                        }
                    }
                }
                Ok(Some(v))
            }
            None => Ok(Some(scalar_value())),
        },
        OP_GRAPH_EDGES => match opts.live {
            Some(live) => {
                let table = live.edge_table();
                let rows = table.len();
                let mut v = alloc_array(DTYPE_I64, 2, &[rows, 2, 1, 1])?;
                unsafe {
                    let p = v.data as *mut i64;
                    for (r, row) in table.iter().enumerate() {
                        *p.add(r * 2) = row[0];
                        *p.add(r * 2 + 1) = row[1];
                    }
                }
                Ok(Some(v))
            }
            None => Ok(Some(scalar_value())),
        },
        OP_MACHINE_DESC => {
            let mut v = alloc_array(DTYPE_I64, 2, &[8, 2, 1, 1])?;
            unsafe {
                let p = v.data as *mut i64;
                for (i, e) in crate::machine::ENGINES.iter().enumerate() {
                    *p.add(i * 2) = e.kind as i64;
                    *p.add(i * 2 + 1) = if e.online { 1 } else { 0 };
                }
            }
            Ok(Some(v))
        }
        OP_READY_SET => match &g.ready_input {
            Some(table) => {
                let rows = table.len();
                let mut v = alloc_array(DTYPE_I64, 2, &[rows, 2, 1, 1])?;
                unsafe {
                    let p = v.data as *mut i64;
                    for (r, row) in table.iter().enumerate() {
                        *p.add(r * 2) = row[0];
                        *p.add(r * 2 + 1) = row[1];
                    }
                }
                Ok(Some(v))
            }
            None => Ok(Some(scalar_value())),
        },
        OP_FILTER => {
            let a = input(g, nd, 0)?;
            if c.len() < 9 {
                return Err("bad filter");
            }
            let col = c[0] as usize;
            let val = i64::from_le_bytes([c[1], c[2], c[3], c[4], c[5], c[6], c[7], c[8]]);
            filter_table(&a, col, val)
        }
        OP_SORT_BY => {
            let a = input(g, nd, 0)?;
            if c.len() < 2 {
                return Err("bad sort");
            }
            let col = c[0] as usize;
            let desc = c[1] != 0;
            sort_table(&a, col, desc)
        }
        OP_COUNT => {
            let a = input(g, nd, 0)?;
            let rows = if a.rank >= 2 {
                a.shape[0]
            } else if a.rank == 1 {
                a.shape[0]
            } else {
                1
            };
            let mut v = alloc_array(DTYPE_I64, 0, &[1, 1, 1, 1])?;
            unsafe { *(v.data as *mut i64) = rows as i64 };
            Ok(Some(v))
        }
        OP_FORMAT => {
            let a = input(g, nd, 0)?;
            format_value(&a, c)
        }
        OP_COPY => {
            // dyadic: (trigger, array).  The trigger orders the copy after a
            // prior measurement; the array is copied (explicit payload copy).
            let _trigger = input(g, nd, 0)?;
            let a = input(g, nd, 1)?;
            let mut v = alloc_array(a.dtype, a.rank, &a.shape)?;
            unsafe {
                core::ptr::copy_nonoverlapping(a.data as *const u8, v.data as *mut u8, v.byte_len());
            }
            unsafe {
                crate::kernel::COUNTERS.payload_copied += v.byte_len() as u64;
            }
            Ok(Some(v))
        }
        OP_SEND => {
            // immutable same-home send is a region-cap share, not a memcpy.
            let a = input(g, nd, 0)?;
            if let Some(region) = a.region {
                crate::kernel::region_addref(region);
            }
            Ok(Some(a))
        }
        OP_CAPS => {
            let table = crate::kernel::caps_table();
            let rows = table.len();
            let mut v = alloc_array(DTYPE_I64, 2, &[rows, 4, 1, 1])?;
            unsafe {
                let p = v.data as *mut i64;
                for (r, row) in table.iter().enumerate() {
                    for (cc, val) in row.iter().enumerate() {
                        *p.add(r * 4 + cc) = *val;
                    }
                }
            }
            Ok(Some(v))
        }
        OP_NAMES => match opts.live {
            Some(live) => {
                let stride = live.names_stride();
                let table = live.names_table();
                let rows = table.len() / stride;
                let mut v = alloc_array(DTYPE_U8, 2, &[rows, stride, 1, 1])?;
                unsafe {
                    core::ptr::copy_nonoverlapping(table.as_ptr(), v.data as *mut u8, table.len());
                }
                Ok(Some(v))
            }
            None => Ok(Some(scalar_value())),
        },
        OP_ZERO => {
            crate::kernel::reset_counters();
            let mut v = alloc_array(DTYPE_I64, 0, &[1, 1, 1, 1])?;
            unsafe { *(v.data as *mut i64) = 0 };
            Ok(Some(v))
        }
        OP_FMT_MACHINE => {
            let text = crate::machine::description_text();
            let mut v = alloc_array(DTYPE_U8, 1, &[text.len(), 1, 1, 1])?;
            unsafe {
                core::ptr::copy_nonoverlapping(text.as_ptr(), v.data as *mut u8, text.len());
            }
            Ok(Some(v))
        }
        OP_PROVENANCE => {
            // const payload = node id (u32 LE)
            let node = u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize;
            let text = provenance_text(opts.live, node);
            let mut v = alloc_array(DTYPE_U8, 1, &[text.len(), 1, 1, 1])?;
            unsafe {
                core::ptr::copy_nonoverlapping(text.as_ptr(), v.data as *mut u8, text.len());
            }
            Ok(Some(v))
        }
        OP_COUNTER_BYTES => {
            let _a = input(g, nd, 0)?; // dependency: wait for the measured value
            let mut v = alloc_array(DTYPE_I64, 0, &[1, 1, 1, 1])?;
            unsafe {
                *(v.data as *mut i64) = crate::kernel::counters().payload_moved as i64;
            }
            Ok(Some(v))
        }
        OP_COUNTER_COPIED => {
            let _a = input(g, nd, 0)?;
            let mut v = alloc_array(DTYPE_I64, 0, &[1, 1, 1, 1])?;
            unsafe {
                *(v.data as *mut i64) = crate::kernel::counters().payload_copied as i64;
            }
            Ok(Some(v))
        }
        OP_COUNTER_ENTRIES => {
            let _a = input(g, nd, 0)?;
            let mut v = alloc_array(DTYPE_I64, 0, &[1, 1, 1, 1])?;
            unsafe {
                *(v.data as *mut i64) = crate::kernel::counters().kernel_entries as i64;
            }
            Ok(Some(v))
        }
        OP_STATS => {
            let _a = input(g, nd, 0)?; // dependency: wait for the measured value
            let c = crate::kernel::counters();
            let mut out = alloc::vec::Vec::new();
            crate::fmt::append_str(&mut out, "payload bytes moved: ");
            crate::fmt::append_u64(&mut out, c.payload_moved);
            crate::fmt::append_str(&mut out, ", kernel entries: ");
            crate::fmt::append_u64(&mut out, c.kernel_entries);
            let mut v = alloc_array(DTYPE_U8, 1, &[out.len(), 1, 1, 1])?;
            unsafe {
                core::ptr::copy_nonoverlapping(out.as_ptr(), v.data as *mut u8, out.len());
            }
            Ok(Some(v))
        }
        OP_ORDER => {
            let a = input(g, nd, 0)?;
            let cols = if a.rank >= 2 { a.shape[1] } else { 2 };
            let n = if cols >= 1 { a.elems() / cols } else { 0 };
            unsafe {
                let p = a.data as *const i64;
                for r in 0..n {
                    g.order_result.push(*p.add(r * cols) as u32);
                }
            }
            Ok(None)
        }
        _ => Err("unknown op"),
    }
}

fn fill_array(
    dtype: u8,
    rank: u8,
    shape: &[usize; 4],
    fill: f64,
) -> Result<Option<Value>, &'static str> {
    let mut v = alloc_array(dtype, rank, shape)?;
    let n = v.elems();
    unsafe {
        let p = v.data as *mut u8;
        match dtype {
            DTYPE_I64 => {
                let f = fill as i64;
                for i in 0..n {
                    *(p.add(i * 8) as *mut i64) = f;
                }
            }
            DTYPE_F32 => {
                let f = fill as f32;
                for i in 0..n {
                    *(p.add(i * 4) as *mut f32) = f;
                }
            }
            _ => {
                for i in 0..n {
                    *p.add(i) = fill as u8;
                }
            }
        }
    }
    Ok(Some(v))
}

fn elementwise(
    op: u8,
    a: &Value,
    b: &Value,
) -> Result<Option<Value>, &'static str> {
    let (dtype, rank, shape) = broadcast(a, b);
    let mut v = alloc_array(dtype, rank, &shape)?;
    let n = v.elems();
    let a_scalar = a.rank == 0;
    let b_scalar = b.rank == 0;
    unsafe {
        let p = v.data as *mut u8;
        match dtype {
            DTYPE_F32 => {
                let af = a.data as *const f32;
                let bf = b.data as *const f32;
                for i in 0..n {
                    let x = *af.add(if a_scalar { 0 } else { i });
                    let y = *bf.add(if b_scalar { 0 } else { i });
                    let r = match op {
                        OP_ADD => x + y,
                        OP_SUB => x - y,
                        OP_MUL => x * y,
                        OP_DIV => x / y,
                        _ => 0.0,
                    };
                    *(p.add(i * 4) as *mut f32) = r;
                }
            }
            _ => {
                let ai = a.data as *const i64;
                let bi = b.data as *const i64;
                for i in 0..n {
                    let x = *ai.add(if a_scalar { 0 } else { i });
                    let y = *bi.add(if b_scalar { 0 } else { i });
                    let r = match op {
                        OP_ADD => x + y,
                        OP_SUB => x - y,
                        OP_MUL => x * y,
                        OP_DIV => x / y,
                        _ => 0,
                    };
                    *(p.add(i * 8) as *mut i64) = r;
                }
            }
        }
    }
    Ok(Some(v))
}

fn elementwise_fused(a: &Value, b: &Value, d: &Value) -> Result<Option<Value>, &'static str> {
    let (dtype, rank, shape) = broadcast(a, b);
    let mut v = alloc_array(dtype, rank, &shape)?;
    let n = v.elems();
    let a_scalar = a.rank == 0;
    let b_scalar = b.rank == 0;
    let d_scalar = d.rank == 0;
    unsafe {
        let p = v.data as *mut u8;
        match dtype {
            DTYPE_F32 => {
                let af = a.data as *const f32;
                let bf = b.data as *const f32;
                let df = d.data as *const f32;
                for i in 0..n {
                    let x = *af.add(if a_scalar { 0 } else { i });
                    let y = *bf.add(if b_scalar { 0 } else { i });
                    let z = *df.add(if d_scalar { 0 } else { i });
                    *(p.add(i * 4) as *mut f32) = (x + y) * z;
                }
            }
            _ => {
                let ai = a.data as *const i64;
                let bi = b.data as *const i64;
                let di = d.data as *const i64;
                for i in 0..n {
                    let x = *ai.add(if a_scalar { 0 } else { i });
                    let y = *bi.add(if b_scalar { 0 } else { i });
                    let z = *di.add(if d_scalar { 0 } else { i });
                    *(p.add(i * 8) as *mut i64) = (x + y) * z;
                }
            }
        }
    }
    Ok(Some(v))
}

fn broadcast(a: &Value, b: &Value) -> (u8, u8, [usize; 4]) {
    let dtype = if a.rank == 0 { b.dtype } else { a.dtype };
    if a.rank == 0 {
        (dtype, b.rank, b.shape)
    } else {
        (dtype, a.rank, a.shape)
    }
}

fn reduce_sum(a: &Value) -> Result<Option<Value>, &'static str> {
    if a.rank <= 1 {
        let mut v = alloc_array(a.dtype, 0, &[1, 1, 1, 1])?;
        let n = a.elems();
        unsafe {
            match a.dtype {
                DTYPE_F32 => {
                    let mut s = 0.0f32;
                    for i in 0..n {
                        s += *(a.data as *const f32).add(i);
                    }
                    *(v.data as *mut f32) = s;
                }
                _ => {
                    let mut s = 0i64;
                    for i in 0..n {
                        s += *(a.data as *const i64).add(i);
                    }
                    *(v.data as *mut i64) = s;
                }
            }
        }
        Ok(Some(v))
    } else {
        let rows = elems(a.rank - 1, &a.shape);
        let cols = a.shape[(a.rank - 1) as usize];
        let mut sh = [1usize; 4];
        sh[0] = rows;
        let mut v = alloc_array(a.dtype, 1, &sh)?;
        unsafe {
            match a.dtype {
                DTYPE_F32 => {
                    for r in 0..rows {
                        let mut s = 0.0f32;
                        for c_ in 0..cols {
                            s += *(a.data as *const f32).add(r * cols + c_);
                        }
                        *(v.data as *mut f32).add(r) = s;
                    }
                }
                _ => {
                    for r in 0..rows {
                        let mut s = 0i64;
                        for c_ in 0..cols {
                            s += *(a.data as *const i64).add(r * cols + c_);
                        }
                        *(v.data as *mut i64).add(r) = s;
                    }
                }
            }
        }
        Ok(Some(v))
    }
}

fn filter_table(a: &Value, col: usize, val: i64) -> Result<Option<Value>, &'static str> {
    if a.rank != 2 || col >= a.shape[1] {
        return Err("bad table");
    }
    let rows = a.shape[0];
    let cols = a.shape[1];
    let mut keep = alloc::vec::Vec::new();
    unsafe {
        let p = a.data as *const i64;
        for r in 0..rows {
            if *p.add(r * cols + col) == val {
                keep.push(r);
            }
        }
    }
    let mut v = alloc_array(DTYPE_I64, 2, &[keep.len(), cols, 1, 1])?;
    unsafe {
        let p = a.data as *const i64;
        let o = v.data as *mut i64;
        for (i, r) in keep.iter().enumerate() {
            for cc in 0..cols {
                *o.add(i * cols + cc) = *p.add(r * cols + cc);
            }
        }
    }
    Ok(Some(v))
}

fn sort_table(a: &Value, col: usize, desc: bool) -> Result<Option<Value>, &'static str> {
    if a.rank != 2 || col >= a.shape[1] {
        return Err("bad table");
    }
    let rows = a.shape[0];
    let cols = a.shape[1];
    let mut idx: alloc::vec::Vec<usize> = (0..rows).collect();
    unsafe {
        let p = a.data as *const i64;
        idx.sort_by(|x, y| {
            let kx = *p.add(x * cols + col);
            let ky = *p.add(y * cols + col);
            if desc {
                ky.cmp(&kx)
            } else {
                kx.cmp(&ky)
            }
        });
    }
    let mut v = alloc_array(DTYPE_I64, 2, &[rows, cols, 1, 1])?;
    unsafe {
        let p = a.data as *const i64;
        let o = v.data as *mut i64;
        for (i, r) in idx.iter().enumerate() {
            for cc in 0..cols {
                *o.add(i * cols + cc) = *p.add(r * cols + cc);
            }
        }
    }
    Ok(Some(v))
}

fn format_value(a: &Value, template: &[u8]) -> Result<Option<Value>, &'static str> {
    let mut out = alloc::vec::Vec::new();
    out.extend_from_slice(template);
    if a.rank >= 2 {
        // table: render row by row
        let rows = a.shape[0];
        let cols = a.shape[1];
        unsafe {
            match a.dtype {
                DTYPE_U8 => {
                    let p = a.data as *const u8;
                    for r in 0..rows {
                        for cc in 0..cols {
                            let b = *p.add(r * cols + cc);
                            if b == 0 {
                                break;
                            }
                            out.push(b);
                        }
                        out.push(b'\n');
                    }
                }
                _ => {
                    let p = a.data as *const i64;
                    for r in 0..rows {
                        out.extend_from_slice(b"  ");
                        for cc in 0..cols {
                            if cc > 0 {
                                out.push(b' ');
                            }
                            crate::fmt::append_i64(&mut out, *p.add(r * cols + cc));
                        }
                        out.push(b'\n');
                    }
                }
            }
        }
    } else if a.rank == 0 {
        unsafe {
            match a.dtype {
                DTYPE_I64 => crate::fmt::append_i64(&mut out, *(a.data as *const i64)),
                DTYPE_F32 => crate::fmt::append_f32(&mut out, *(a.data as *const f32)),
                _ => crate::fmt::append_u64(&mut out, *(a.data as *const u8) as u64),
            }
        }
    } else {
        crate::fmt::append_array(&mut out, a.dtype, a.data as *const u8, a.elems());
    }
    let mut v = alloc_array(DTYPE_U8, 1, &[out.len(), 1, 1, 1])?;
    unsafe {
        core::ptr::copy_nonoverlapping(out.as_ptr(), v.data as *mut u8, out.len());
    }
    Ok(Some(v))
}

/// Build the provenance text for a node in the live graph.
fn provenance_text(live: Option<&Graph>, node: usize) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::new();
    match live {
        Some(g) => {
            let nd = &g.prog.nodes[node];
            crate::fmt::append_str(&mut out, "producer ");
            crate::fmt::append_str(&mut out, g.prog.name(node));
            crate::fmt::append_str(&mut out, " (node #");
            crate::fmt::append_u64(&mut out, node as u64);
            crate::fmt::append_str(&mut out, "), inputs ");
            let mut first = true;
            for inp in [nd.in0, nd.in1, nd.in2] {
                if inp != NONE {
                    if !first {
                        crate::fmt::append_str(&mut out, " and ");
                    }
                    first = false;
                    crate::fmt::append_str(&mut out, g.prog.name(inp as usize));
                    crate::fmt::append_str(&mut out, " (node #");
                    crate::fmt::append_u64(&mut out, inp as u64);
                    crate::fmt::append_str(&mut out, ")");
                }
            }
        }
        None => crate::fmt::append_str(&mut out, "no live graph"),
    }
    out
}

/// Build the predicted text of a display write without touching the console.
pub fn predict_text(a: &Value) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::new();
    if a.dtype == DTYPE_U8 && a.rank == 1 {
        unsafe {
            let p = a.data as *const u8;
            for i in 0..a.elems() {
                out.push(*p.add(i));
            }
        }
    } else {
        crate::fmt::append_array(&mut out, a.dtype, a.data as *const u8, a.elems());
    }
    out
}

/// The display effect: validate the cap, simulate the predicted text, and only
/// then commit.  It is a tee (like Uiua's `&p`): the input value is returned,
/// so the stack thread continues.  A missing cap leaves the console unchanged.
fn display_effect(opts: &RunOpts, a: &Value) -> Result<Option<Value>, &'static str> {
    if !crate::kernel::holds(opts.realm, CAP_DISPLAY) {
        return Err("no display capability");
    }
    let mut predicted = predict_text(a);
    predicted.push(b'\n');
    crate::console_write_bytes(&predicted);
    // tee: return the input (sharing its region)
    if let Some(region) = a.region {
        crate::kernel::region_addref(region);
    }
    Ok(Some(a.clone()))
}
