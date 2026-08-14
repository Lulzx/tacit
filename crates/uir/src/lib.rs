#![no_std]

//! UIR: the shared semantic graph between the host compiler and the guest
//! stepper.  Values, transformations, capabilities and placement live here
//! as named nodes with shapes, purity, effects, homes and engines.
//!
//! The binary codec is hand-written and versioned so that a host compiler and
//! a guest stepper built at different times fail loudly rather than silently
//! misreading a payload.

extern crate alloc;

pub const MAGIC: [u8; 4] = *b"UIR\x00";
pub const VERSION: u32 = 1;

pub const ENGINE_PCORE: u8 = 0;
pub const ENGINE_ECORE: u8 = 1;
pub const ENGINE_NEON: u8 = 2;
pub const ENGINE_SME: u8 = 3;
pub const ENGINE_GPU: u8 = 4;
pub const ENGINE_ANE: u8 = 5;
pub const ENGINE_MEDIA: u8 = 6;
pub const ENGINE_DISPLAY: u8 = 7;

pub const HOME_UMA: u8 = 0;

pub const DTYPE_I64: u8 = 0;
pub const DTYPE_F32: u8 = 1;
pub const DTYPE_U8: u8 = 2;

pub const CAP_NONE: u8 = 0xff;
pub const CAP_DISPLAY: u8 = 0;
pub const CAP_KEYBOARD: u8 = 1;

// Op codes.  Values are stable across host and guest.
pub const OP_CONST: u8 = 0;
pub const OP_ADD: u8 = 1;
pub const OP_SUB: u8 = 2;
pub const OP_MUL: u8 = 3;
pub const OP_DIV: u8 = 4;
pub const OP_NEG: u8 = 5;
pub const OP_REDUCE_SUM: u8 = 6;
pub const OP_RESHAPE: u8 = 7;
pub const OP_DISPLAY: u8 = 8; // effect, needs display cap
pub const OP_KEYBOARD: u8 = 9; // effect, needs keyboard cap -> char array
pub const OP_GRAPH_NODES: u8 = 10; // source: live-graph node table
pub const OP_GRAPH_EDGES: u8 = 11; // source: live-graph edge table
pub const OP_MACHINE_DESC: u8 = 12; // source: machine description table
pub const OP_READY_SET: u8 = 13; // source: ready set (policy input)
pub const OP_FILTER: u8 = 14; // table filter by column/value
pub const OP_SORT_BY: u8 = 15; // table sort by column
pub const OP_REVERSE: u8 = 16; // reverse rows (or reverse 1d array)
pub const OP_COUNT: u8 = 17; // row count -> i64 scalar
pub const OP_FORMAT: u8 = 18; // template + scalar/array -> char array
pub const OP_FILL: u8 = 19; // shape + fill value -> array
pub const OP_COPY: u8 = 20; // explicit copy (bench control)
pub const OP_SEND: u8 = 21; // immutable region-cap share
pub const OP_COUNTER_BYTES: u8 = 22; // source: payload bytes moved
pub const OP_COUNTER_COPIED: u8 = 23; // source: payload bytes copied
pub const OP_COUNTER_ENTRIES: u8 = 24; // source: kernel entries
pub const OP_ORDER: u8 = 25; // sink: consume ordering (policy output)
pub const OP_ADD_MUL: u8 = 26; // fused Add-then-Multiply
pub const OP_ROWS: u8 = 27; // rank-wise map marker (parallel leading axis)
pub const OP_CAPS: u8 = 28; // source: capabilities table

pub const NONE: u32 = 0xffff_ffff;

/// Descriptor for one UIR node.
#[derive(Clone, Copy, Debug)]
pub struct NodeDesc {
    pub id: u32,
    pub op: u8,
    pub dtype: u8,
    pub rank: u8,
    pub shape: [u32; 4],
    pub pure: bool,
    pub parallel_axis: u8, // 0 = none, else 1-based axis index
    pub engine: u8,
    pub home: u8,
    pub cap_need: u8,
    pub in0: u32,
    pub in1: u32,
    pub in2: u32,
    pub name_len: u32,
    pub const_len: u32,
}

pub fn op_name(op: u8) -> &'static str {
    match op {
        OP_CONST => "Const",
        OP_ADD => "Add",
        OP_SUB => "Sub",
        OP_MUL => "Multiply",
        OP_DIV => "Div",
        OP_NEG => "Neg",
        OP_REDUCE_SUM => "ReduceSum",
        OP_RESHAPE => "Reshape",
        OP_DISPLAY => "Display",
        OP_KEYBOARD => "Keyboard",
        OP_GRAPH_NODES => "GraphNodes",
        OP_GRAPH_EDGES => "GraphEdges",
        OP_MACHINE_DESC => "MachineDesc",
        OP_READY_SET => "ReadySet",
        OP_FILTER => "Filter",
        OP_SORT_BY => "SortBy",
        OP_REVERSE => "Reverse",
        OP_COUNT => "Count",
        OP_FORMAT => "Format",
        OP_FILL => "Fill",
        OP_COPY => "Copy",
        OP_SEND => "Send",
        OP_COUNTER_BYTES => "BytesMoved",
        OP_COUNTER_COPIED => "BytesCopied",
        OP_COUNTER_ENTRIES => "KernelEntries",
        OP_ORDER => "Order",
        OP_ADD_MUL => "AddMul",
        OP_ROWS => "Rows",
        OP_CAPS => "Caps",
        _ => "?",
    }
}

pub fn engine_name(e: u8) -> &'static str {
    match e {
        ENGINE_PCORE => "p-core",
        ENGINE_ECORE => "e-core",
        ENGINE_NEON => "neon",
        ENGINE_SME => "sme",
        ENGINE_GPU => "gpu",
        ENGINE_ANE => "ane",
        ENGINE_MEDIA => "media",
        ENGINE_DISPLAY => "display",
        _ => "?",
    }
}

pub fn dtype_name(d: u8) -> &'static str {
    match d {
        DTYPE_I64 => "i64",
        DTYPE_F32 => "f32",
        DTYPE_U8 => "u8",
        _ => "?",
    }
}

pub fn dtype_size(d: u8) -> usize {
    match d {
        DTYPE_I64 => 8,
        DTYPE_F32 => 4,
        DTYPE_U8 => 1,
        _ => 1,
    }
}

/// Number of elements described by a shape (product of dims, min 1).
pub fn shape_elems(rank: u8, shape: &[u32; 4]) -> usize {
    let mut n = 1usize;
    for i in 0..rank as usize {
        n *= shape[i] as usize;
    }
    n
}

// ---------------------------------------------------------------------------
// Binary codec
// ---------------------------------------------------------------------------

/// Growable byte buffer.
pub struct VecWriter {
    pub buf: alloc::vec::Vec<u8>,
}

impl VecWriter {
    pub fn new() -> Self {
        VecWriter { buf: alloc::vec::Vec::new() }
    }
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
}

/// Encoder for a whole program.
pub struct Encoder {
    pub w: VecWriter,
    pub count: u32,
}

impl Encoder {
    pub fn new() -> Self {
        let mut w = VecWriter::new();
        w.bytes(&MAGIC);
        w.u32(VERSION);
        w.u32(0); // patched in finish()
        Encoder { w, count: 0 }
    }

    pub fn node(&mut self, d: &NodeDesc, name: &[u8], const_payload: &[u8]) -> u32 {
        let id = self.count;
        self.w.u32(d.id);
        self.w.u8(d.op);
        self.w.u8(d.dtype);
        self.w.u8(d.rank);
        for i in 0..4 {
            self.w.u32(d.shape[i]);
        }
        self.w.u8(if d.pure { 1 } else { 0 });
        self.w.u8(d.parallel_axis);
        self.w.u8(d.engine);
        self.w.u8(d.home);
        self.w.u8(d.cap_need);
        self.w.u32(d.in0);
        self.w.u32(d.in1);
        self.w.u32(d.in2);
        self.w.u32(name.len() as u32);
        self.w.u32(const_payload.len() as u32);
        self.w.bytes(name);
        self.w.bytes(const_payload);
        self.count += 1;
        id
    }

    pub fn finish(mut self) -> alloc::vec::Vec<u8> {
        self.w.buf[8..12].copy_from_slice(&self.count.to_le_bytes());
        self.w.buf
    }
}

/// A fully-decoded program that owns its data.
pub struct Program {
    pub nodes: alloc::vec::Vec<NodeDesc>,
    pub names: alloc::vec::Vec<alloc::vec::Vec<u8>>,
    pub consts: alloc::vec::Vec<alloc::vec::Vec<u8>>,
}

impl Program {
    pub fn name(&self, i: usize) -> &str {
        core::str::from_utf8(&self.names[i]).unwrap_or("?")
    }
}

#[derive(Debug)]
pub struct DecodeError;

pub fn decode(buf: &[u8]) -> Result<Program, DecodeError> {
    if buf.len() < 12 || buf[0..4] != MAGIC {
        return Err(DecodeError);
    }
    let version = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    if version != VERSION {
        return Err(DecodeError);
    }
    let count = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]) as usize;
    let mut nodes = alloc::vec::Vec::with_capacity(count);
    let mut names = alloc::vec::Vec::with_capacity(count);
    let mut consts = alloc::vec::Vec::with_capacity(count);
    let mut pos = 12usize;
    for _ in 0..count {
        let take = |pos: &mut usize, n: usize| -> Result<&[u8], DecodeError> {
            if *pos + n > buf.len() {
                return Err(DecodeError);
            }
            let s = &buf[*pos..*pos + n];
            *pos += n;
            Ok(s)
        };
        let id = u32::from_le_bytes(read4(take(&mut pos, 4)?));
        let op = take(&mut pos, 1)?[0];
        let dtype = take(&mut pos, 1)?[0];
        let rank = take(&mut pos, 1)?[0];
        let mut shape = [0u32; 4];
        for i in 0..4 {
            shape[i] = u32::from_le_bytes(read4(take(&mut pos, 4)?));
        }
        let pure = take(&mut pos, 1)?[0] != 0;
        let parallel_axis = take(&mut pos, 1)?[0];
        let engine = take(&mut pos, 1)?[0];
        let home = take(&mut pos, 1)?[0];
        let cap_need = take(&mut pos, 1)?[0];
        let in0 = u32::from_le_bytes(read4(take(&mut pos, 4)?));
        let in1 = u32::from_le_bytes(read4(take(&mut pos, 4)?));
        let in2 = u32::from_le_bytes(read4(take(&mut pos, 4)?));
        let name_len = u32::from_le_bytes(read4(take(&mut pos, 4)?)) as usize;
        let const_len = u32::from_le_bytes(read4(take(&mut pos, 4)?)) as usize;
        let name = take(&mut pos, name_len)?;
        let c = take(&mut pos, const_len)?;
        nodes.push(NodeDesc {
            id,
            op,
            dtype,
            rank,
            shape,
            pure,
            parallel_axis,
            engine,
            home,
            cap_need,
            in0,
            in1,
            in2,
            name_len: name_len as u32,
            const_len: const_len as u32,
        });
        names.push(alloc::vec::Vec::from(name));
        consts.push(alloc::vec::Vec::from(c));
    }
    Ok(Program { nodes, names, consts })
}

fn read4(b: &[u8]) -> [u8; 4] {
    [b[0], b[1], b[2], b[3]]
}

/// Serialize a numeric array into const-payload bytes (little-endian).
pub fn const_i64(vals: &[i64]) -> alloc::vec::Vec<u8> {
    let mut w = VecWriter::new();
    for v in vals {
        w.u64(*v as u64);
    }
    w.buf
}

pub fn const_f32(vals: &[f32]) -> alloc::vec::Vec<u8> {
    let mut w = VecWriter::new();
    for v in vals {
        w.u32(v.to_bits());
    }
    w.buf
}

pub fn const_u8(vals: &[u8]) -> alloc::vec::Vec<u8> {
    alloc::vec::Vec::from(vals)
}

/// A fill descriptor: (dtype, shape rank, shape, fill-value as f64/i64 bytes).
/// Encoded as: u8 dtype, u8 rank, 4x u32 shape, then 8 bytes of fill value.
pub fn const_fill(dtype: u8, rank: u8, shape: &[u32; 4], fill: f64) -> alloc::vec::Vec<u8> {
    let mut w = VecWriter::new();
    w.u8(dtype);
    w.u8(rank);
    for i in 0..4 {
        w.u32(shape[i]);
    }
    w.u64(fill.to_bits());
    w.buf
}

pub fn decode_fill(payload: &[u8]) -> Option<(u8, u8, [u32; 4], f64)> {
    if payload.len() != 1 + 1 + 16 + 8 {
        return None;
    }
    let dtype = payload[0];
    let rank = payload[1];
    let mut shape = [0u32; 4];
    for i in 0..4 {
        shape[i] = u32::from_le_bytes(read4(&payload[2 + i * 4..6 + i * 4]));
    }
    let fill = f64::from_bits(u64::from_le_bytes(read8(&payload[18..26])));
    Some((dtype, rank, shape, fill))
}

fn read8(b: &[u8]) -> [u8; 8] {
    [b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]
}
