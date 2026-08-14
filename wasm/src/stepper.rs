//! A self-contained, portable UIR interpreter for the WASM build.
//!
//! This is the *language machine*: it decodes UIR (via `uir::decode`) and
//! steps the dataflow graph, exactly like the AArch64 guest's `stepper.rs`,
//! but with owned `Vec<u8>` values instead of raw kernel-region pointers.
//! The machine layer (MMU, NEON/SME engines, PACGA) is not here — WASM is a
//! second machine layer for the same language machine.
//!
//! Supported ops are the pure compute subset the compiler emits for the
//! fusion/matmul benches: fill, const, elementwise, fused add-mul, reduce,
//! reshape, reverse, count, matmul, eq, grade, select, keep, pick, couple.

extern crate alloc;

use alloc::vec::Vec;
use uir::*;

#[derive(Clone)]
pub struct Value {
    pub data: Vec<u8>,
    pub dtype: u8,
    pub rank: u8,
    pub shape: [usize; 4],
}

impl Value {
    pub fn elems(&self) -> usize {
        let mut n = 1usize;
        for i in 0..self.rank as usize {
            n *= self.shape[i];
        }
        n
    }
    pub fn elem_size(&self) -> usize {
        dtype_size(self.dtype)
    }
    pub fn byte_len(&self) -> usize {
        self.elems() * self.elem_size()
    }
}

fn to_u32_shape(s: &[usize; 4]) -> [u32; 4] {
    [s[0] as u32, s[1] as u32, s[2] as u32, s[3] as u32]
}

/// Run a decoded program to completion; return the value of the last node.
pub fn run(prog: &Program) -> Result<Value, String> {
    let n = prog.nodes.len();
    let mut vals: Vec<Option<Value>> = (0..n).map(|_| None).collect();
    for i in 0..n {
        let v = step(prog, &prog.nodes[i], &vals)?;
        vals[i] = Some(v);
    }
    for i in (0..n).rev() {
        if let Some(v) = &vals[i] {
            return Ok(Value::clone(v));
        }
    }
    Err("no value produced".into())
}

/// One fired node: enough for the browser to draw the tape without
/// shipping the whole array back.
pub struct StepInfo {
    pub id: u32,
    pub op: u8,
    pub engine: u8,
    pub dtype: u8,
    pub rank: u8,
    pub shape: [u32; 4],
    pub bytes: u32,
    pub elems: u32,
    pub in0: u32,
    pub in1: u32,
    pub in2: u32,
    pub pure: bool,
    pub home: u8,
    pub cap: u8,
    pub parallel: u8,
    pub checksum: u64,
    pub vmin: f64,
    pub vmax: f64,
    pub vsum: f64,
    pub name: Vec<u8>,
    pub preview: Vec<u8>,
}

/// A window into a materialized node value.
pub struct Peek {
    pub id: u32,
    pub dtype: u8,
    pub rank: u8,
    pub shape: [u32; 4],
    pub elems: u32,
    pub bytes: u32,
    pub offset: u32,
    pub count: u32,
    pub window: Vec<u8>,
}

/// A loaded program that can be stepped one node at a time so the
/// browser can time each fire with `performance.now()`.
pub struct Session {
    prog: Program,
    vals: Vec<Option<Value>>,
    next: usize,
}

impl Session {
    pub fn load(prog: Program) -> Self {
        let n = prog.nodes.len();
        Session {
            vals: (0..n).map(|_| None).collect(),
            prog,
            next: 0,
        }
    }

    pub fn node_count(&self) -> usize {
        self.prog.nodes.len()
    }

    pub fn step(&mut self) -> Result<Option<StepInfo>, String> {
        if self.next >= self.prog.nodes.len() {
            return Ok(None);
        }
        let i = self.next;
        let nd = self.prog.nodes[i];
        let v = step(&self.prog, &nd, &self.vals)?;
        let (vmin, vmax, vsum) = value_stats(&v);
        let preview_n = v.data.len().min(256);
        let info = StepInfo {
            id: nd.id,
            op: nd.op,
            engine: nd.engine,
            dtype: v.dtype,
            rank: v.rank,
            shape: to_u32_shape(&v.shape),
            bytes: v.byte_len() as u32,
            elems: v.elems() as u32,
            in0: nd.in0,
            in1: nd.in1,
            in2: nd.in2,
            pure: nd.pure,
            home: nd.home,
            cap: nd.cap_need,
            parallel: nd.parallel_axis,
            checksum: fnv1a(&v.data),
            vmin,
            vmax,
            vsum,
            name: self.prog.names[i].clone(),
            preview: v.data[..preview_n].to_vec(),
        };
        self.vals[i] = Some(v);
        self.next += 1;
        Ok(Some(info))
    }

    pub fn last_value(&self) -> Option<&Value> {
        self.vals.iter().rev().find_map(|v| v.as_ref())
    }

    pub fn peek(&self, id: u32, offset: u32, count: u32) -> Result<Peek, String> {
        let v = self
            .vals
            .get(id as usize)
            .and_then(|x| x.as_ref())
            .ok_or_else(|| "no value".to_string())?;
        let es = v.elem_size();
        let elems = v.elems() as u32;
        let off = offset.min(elems);
        let n = count.min(elems.saturating_sub(off));
        let start = off as usize * es;
        let end = start + n as usize * es;
        Ok(Peek {
            id,
            dtype: v.dtype,
            rank: v.rank,
            shape: to_u32_shape(&v.shape),
            elems,
            bytes: v.byte_len() as u32,
            offset: off,
            count: n,
            window: v.data[start..end].to_vec(),
        })
    }
}

fn value_stats(v: &Value) -> (f64, f64, f64) {
    let n = v.elems();
    if n == 0 {
        return (0.0, 0.0, 0.0);
    }
    match v.dtype {
        DTYPE_F32 => {
            let mut min = f32::INFINITY;
            let mut max = f32::NEG_INFINITY;
            let mut sum = 0.0f64;
            for c in v.data.chunks_exact(4) {
                let x = f32::from_bits(u32::from_le_bytes([c[0], c[1], c[2], c[3]]));
                if x < min { min = x; }
                if x > max { max = x; }
                sum += x as f64;
            }
            (min as f64, max as f64, sum)
        }
        DTYPE_I64 => {
            let mut min = i64::MAX;
            let mut max = i64::MIN;
            let mut sum = 0.0f64;
            for c in v.data.chunks_exact(8) {
                let x = i64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]);
                if x < min { min = x; }
                if x > max { max = x; }
                sum += x as f64;
            }
            (min as f64, max as f64, sum)
        }
        _ => {
            let mut min = u8::MAX;
            let mut max = u8::MIN;
            let mut sum = 0.0f64;
            for &x in &v.data {
                if x < min { min = x; }
                if x > max { max = x; }
                sum += x as f64;
            }
            (min as f64, max as f64, sum)
        }
    }
}

fn input<'a>(prog: &'a Program, vals: &'a [Option<Value>], id: u32) -> Result<&'a Value, String> {
    if id == NONE {
        return Err("missing input".into());
    }
    vals[id as usize].as_ref().ok_or_else(|| "input not ready".into())
}

fn step(prog: &Program, nd: &NodeDesc, vals: &[Option<Value>]) -> Result<Value, String> {
    match nd.op {
        OP_CONST => {
            let c = &prog.consts[nd.id as usize];
            Ok(Value {
                data: c.clone(),
                dtype: nd.dtype,
                rank: nd.rank,
                shape: [nd.shape[0] as usize, nd.shape[1] as usize, nd.shape[2] as usize, nd.shape[3] as usize],
            })
        }
        OP_FILL => {
            let (dtype, rank, shape, fill) = decode_fill(&prog.consts[nd.id as usize])
                .ok_or("bad fill payload")?;
            let mut data = Vec::new();
            let elems = shape_elems(rank, &shape);
            match dtype {
                DTYPE_I64 => {
                    let f = fill as i64;
                    for _ in 0..elems {
                        data.extend_from_slice(&f.to_le_bytes());
                    }
                }
                DTYPE_F32 => {
                    let f = fill as f32;
                    for _ in 0..elems {
                        data.extend_from_slice(&f.to_bits().to_le_bytes());
                    }
                }
                DTYPE_U8 => {
                    let f = fill as u8;
                    for _ in 0..elems {
                        data.push(f);
                    }
                }
                _ => return Err("bad fill dtype".into()),
            }
            Ok(Value {
                data,
                dtype,
                rank,
                shape: [shape[0] as usize, shape[1] as usize, shape[2] as usize, shape[3] as usize],
            })
        }
        OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_EQ => {
            let a = input(prog, vals, nd.in0)?;
            let b = input(prog, vals, nd.in1)?;
            binop(a, b, nd.op)
        }
        OP_ADD_MUL => {
            let a = input(prog, vals, nd.in0)?;
            let b = input(prog, vals, nd.in1)?;
            let d = input(prog, vals, nd.in2)?;
            add_mul(a, b, d)
        }
        OP_NEG => {
            let a = input(prog, vals, nd.in0)?;
            unop(a, nd.op)
        }
        OP_REDUCE_SUM => {
            let a = input(prog, vals, nd.in0)?;
            reduce_sum(a)
        }
        OP_RESHAPE => {
            let a = input(prog, vals, nd.in0)?;
            Ok(Value {
                data: a.data.clone(),
                dtype: a.dtype,
                rank: nd.rank,
                shape: [nd.shape[0] as usize, nd.shape[1] as usize, nd.shape[2] as usize, nd.shape[3] as usize],
            })
        }
        OP_REVERSE => {
            let a = input(prog, vals, nd.in0)?;
            reverse(a)
        }
        OP_COUNT => {
            let a = input(prog, vals, nd.in0)?;
            let rows = if a.rank == 0 { 1 } else { a.shape[0] };
            Ok(Value {
                data: (rows as i64).to_le_bytes().to_vec(),
                dtype: DTYPE_I64,
                rank: 0,
                shape: [1, 1, 1, 1],
            })
        }
        OP_MATMUL => {
            let a = input(prog, vals, nd.in0)?;
            let b = input(prog, vals, nd.in1)?;
            matmul(a, b)
        }
        OP_GRADE_UP | OP_GRADE_DOWN => {
            let a = input(prog, vals, nd.in0)?;
            grade(a, nd.op)
        }
        OP_SELECT | OP_KEEP | OP_PICK => {
            let a = input(prog, vals, nd.in0)?;
            let b = input(prog, vals, nd.in1)?;
            select_keep_pick(a, b, nd.op)
        }
        OP_COUPLE => {
            let a = input(prog, vals, nd.in0)?;
            let b = input(prog, vals, nd.in1)?;
            couple(a, b)
        }
        OP_HASH => {
            let a = input(prog, vals, nd.in0)?;
            let h = fnv1a(&a.data);
            Ok(Value {
                data: (h as i64).to_le_bytes().to_vec(),
                dtype: DTYPE_I64,
                rank: 0,
                shape: [1, 1, 1, 1],
            })
        }
        // Effects / sources / machine-layer ops: not part of the pure
        // compute subset. In the browser these are the "machine layer".
        OP_DISPLAY | OP_KEYBOARD | OP_GRAPH_NODES | OP_GRAPH_EDGES | OP_MACHINE_DESC
        | OP_READY_SET | OP_FORMAT | OP_COPY | OP_SEND | OP_COUNTER_BYTES
        | OP_COUNTER_COPIED | OP_COUNTER_ENTRIES | OP_ROWS | OP_CAPS | OP_NAMES
        | OP_ZERO | OP_FMT_MACHINE | OP_PROVENANCE | OP_STATS | OP_CLOCK
        | OP_REPLAY_KEYS | OP_REPLAY_CLOCK | OP_TRACE | OP_REQUEST => {
            Err(format!("op {} not supported in WASM stepper", nd.op))
        }
        _ => Err(format!("unknown op {}", nd.op)),
    }
}

fn binop(a: &Value, b: &Value, op: u8) -> Result<Value, String> {
    if a.dtype != b.dtype {
        return Err("dtype mismatch".into());
    }
    let n = a.elems().min(b.elems());
    let mut data = Vec::with_capacity(n * a.elem_size());
    match a.dtype {
        DTYPE_I64 => {
            let av = i64s(a);
            let bv = i64s(b);
            for i in 0..n {
                let x = av[i];
                let y = bv[i];
                let r = match op {
                    OP_ADD => x.wrapping_add(y),
                    OP_SUB => x.wrapping_sub(y),
                    OP_MUL => x.wrapping_mul(y),
                    OP_DIV => if y != 0 { x / y } else { 0 },
                    OP_EQ => (x == y) as i64,
                    _ => 0,
                };
                data.extend_from_slice(&r.to_le_bytes());
            }
        }
        DTYPE_F32 => {
            let av = f32s(a);
            let bv = f32s(b);
            for i in 0..n {
                let x = av[i];
                let y = bv[i];
                let r = match op {
                    OP_ADD => x + y,
                    OP_SUB => x - y,
                    OP_MUL => x * y,
                    OP_DIV => x / y,
                    OP_EQ => (x == y) as i64 as f32,
                    _ => 0.0,
                };
                data.extend_from_slice(&r.to_bits().to_le_bytes());
            }
        }
        DTYPE_U8 => {
            let av = u8s(a);
            let bv = u8s(b);
            for i in 0..n {
                let x = av[i];
                let y = bv[i];
                let r = match op {
                    OP_ADD => x.wrapping_add(y),
                    OP_SUB => x.wrapping_sub(y),
                    OP_MUL => x.wrapping_mul(y),
                    OP_DIV => if y != 0 { x / y } else { 0 },
                    OP_EQ => (x == y) as u8,
                    _ => 0,
                };
                data.push(r);
            }
        }
        _ => return Err("bad dtype".into()),
    }
    let out_dtype = if op == OP_EQ { DTYPE_I64 } else { a.dtype };
    Ok(Value {
        data,
        dtype: out_dtype,
        rank: a.rank,
        shape: a.shape,
    })
}

fn add_mul(a: &Value, b: &Value, d: &Value) -> Result<Value, String> {
    // fused (a + b) * d
    let n = a.elems().min(b.elems()).min(d.elems());
    let mut data = Vec::with_capacity(n * a.elem_size());
    match a.dtype {
        DTYPE_F32 => {
            let av = f32s(a);
            let bv = f32s(b);
            let dv = f32s(d);
            for i in 0..n {
                let r = (av[i] + bv[i]) * dv[i];
                data.extend_from_slice(&r.to_bits().to_le_bytes());
            }
        }
        DTYPE_I64 => {
            let av = i64s(a);
            let bv = i64s(b);
            let dv = i64s(d);
            for i in 0..n {
                let r = (av[i] + bv[i]) * dv[i];
                data.extend_from_slice(&r.to_le_bytes());
            }
        }
        _ => return Err("add_mul: unsupported dtype".into()),
    }
    Ok(Value {
        data,
        dtype: a.dtype,
        rank: a.rank,
        shape: a.shape,
    })
}

fn unop(a: &Value, op: u8) -> Result<Value, String> {
    let n = a.elems();
    let mut data = Vec::with_capacity(n * a.elem_size());
    match a.dtype {
        DTYPE_I64 => {
            for &x in i64s(a).iter() {
                let r = match op {
                    OP_NEG => -x,
                    _ => x,
                };
                data.extend_from_slice(&r.to_le_bytes());
            }
        }
        DTYPE_F32 => {
            for &x in f32s(a).iter() {
                let r = match op {
                    OP_NEG => -x,
                    _ => x,
                };
                data.extend_from_slice(&r.to_bits().to_le_bytes());
            }
        }
        _ => return Err("unop: unsupported dtype".into()),
    }
    Ok(Value { data, dtype: a.dtype, rank: a.rank, shape: a.shape })
}

fn reduce_sum(a: &Value) -> Result<Value, String> {
    let mut data = Vec::new();
    match a.dtype {
        DTYPE_I64 => {
            let s: i64 = i64s(a).iter().sum();
            data.extend_from_slice(&s.to_le_bytes());
        }
        DTYPE_F32 => {
            let s: f32 = f32s(a).iter().sum();
            data.extend_from_slice(&s.to_bits().to_le_bytes());
        }
        _ => return Err("reduce: unsupported dtype".into()),
    }
    Ok(Value { data, dtype: a.dtype, rank: 0, shape: [1, 1, 1, 1] })
}

fn reverse(a: &Value) -> Result<Value, String> {
    let row = a.elem_size() * (if a.rank > 1 { a.shape[1] * a.shape[2] * a.shape[3] } else { 1 });
    let rows = if a.rank == 0 { 1 } else { a.shape[0] };
    let mut data = Vec::with_capacity(a.byte_len());
    for r in (0..rows).rev() {
        data.extend_from_slice(&a.data[r * row..(r + 1) * row]);
    }
    Ok(Value { data, dtype: a.dtype, rank: a.rank, shape: a.shape })
}

fn matmul(a: &Value, b: &Value) -> Result<Value, String> {
    if a.dtype != DTYPE_F32 || b.dtype != DTYPE_F32 {
        return Err("matmul: f32 only".into());
    }
    let m = a.shape[0];
    let k = a.shape[1];
    let n = b.shape[1];
    let av = f32s(a);
    let bv = f32s(b);
    let mut data = Vec::with_capacity(m * n * 4);
    for i in 0..m {
        for j in 0..n {
            let mut acc = 0.0f32;
            for kk in 0..k {
                acc += av[i * k + kk] * bv[kk * n + j];
            }
            data.extend_from_slice(&acc.to_bits().to_le_bytes());
        }
    }
    Ok(Value {
        data,
        dtype: DTYPE_F32,
        rank: 2,
        shape: [m, n, 1, 1],
    })
}

fn grade(a: &Value, op: u8) -> Result<Value, String> {
    // grade rows by their first element (or scalar value)
    let rows = if a.rank == 0 { 1 } else { a.shape[0] };
    let mut idx: Vec<usize> = (0..rows).collect();
    let key = |i: usize| -> i64 {
        match a.dtype {
            DTYPE_I64 => i64s(a)[i],
            DTYPE_F32 => f32s(a)[i] as i64,
            _ => 0,
        }
    };
    if op == OP_GRADE_UP {
        idx.sort_by_key(|&i| key(i));
    } else {
        idx.sort_by_key(|&i| std::cmp::Reverse(key(i)));
    }
    let mut data = Vec::with_capacity(rows * 8);
    for i in idx {
        data.extend_from_slice(&(i as i64).to_le_bytes());
    }
    Ok(Value { data, dtype: DTYPE_I64, rank: 1, shape: [rows, 1, 1, 1] })
}

fn select_keep_pick(a: &Value, b: &Value, op: u8) -> Result<Value, String> {
    // a = data (rows), b = index/mask vector
    let row = a.elem_size() * (if a.rank > 1 { a.shape[1] * a.shape[2] * a.shape[3] } else { 1 });
    let rows = if a.rank == 0 { 1 } else { a.shape[0] };
    let idx = i64s(b);
    let mut data = Vec::new();
    match op {
        OP_SELECT => {
            for &i in idx.iter() {
                let i = i as usize;
                if i < rows {
                    data.extend_from_slice(&a.data[i * row..(i + 1) * row]);
                }
            }
        }
        OP_KEEP => {
            for i in 0..rows {
                if i < idx.len() && idx[i] != 0 {
                    data.extend_from_slice(&a.data[i * row..(i + 1) * row]);
                }
            }
        }
        OP_PICK => {
            let i = idx[0] as usize;
            if i < rows {
                data.extend_from_slice(&a.data[i * row..(i + 1) * row]);
            }
        }
        _ => return Err("bad select op".into()),
    }
    let out_rows = data.len() / row;
    Ok(Value {
        data,
        dtype: a.dtype,
        rank: a.rank,
        shape: [out_rows, a.shape[1], a.shape[2], a.shape[3]],
    })
}

fn couple(a: &Value, b: &Value) -> Result<Value, String> {
    let n = a.elems().min(b.elems());
    let mut data = Vec::new();
    match a.dtype {
        DTYPE_I64 => {
            let av = i64s(a);
            let bv = i64s(b);
            for i in 0..n {
                data.extend_from_slice(&av[i].to_le_bytes());
                data.extend_from_slice(&bv[i].to_le_bytes());
            }
        }
        DTYPE_F32 => {
            let av = f32s(a);
            let bv = f32s(b);
            for i in 0..n {
                data.extend_from_slice(&av[i].to_bits().to_le_bytes());
                data.extend_from_slice(&bv[i].to_bits().to_le_bytes());
            }
        }
        _ => return Err("couple: unsupported dtype".into()),
    }
    Ok(Value { data, dtype: a.dtype, rank: 2, shape: [2, n, 1, 1] })
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn i64s(v: &Value) -> Vec<i64> {
    v.data.chunks_exact(8).map(|c| i64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])).collect()
}
fn f32s(v: &Value) -> Vec<f32> {
    v.data.chunks_exact(4).map(|c| f32::from_bits(u32::from_le_bytes([c[0], c[1], c[2], c[3]]))).collect()
}
fn u8s(v: &Value) -> Vec<u8> {
    v.data.clone()
}
