//! The effect-input trace: every world read (keyboard line, clock) is
//! recorded with a sequence number.  Replay consumes the recorded input
//! instead of the live device, so a Realm run twice from the same trace is
//! deterministic.  The trace is the data a later time-travel replay would
//! feed back; `&trace` projects it as an array.

use alloc::vec::Vec;

pub const KIND_KEYS: u8 = 0;
pub const KIND_CLOCK: u8 = 1;

pub struct TraceEntry {
    pub kind: u8,
    pub seq: u32,
    pub data: Vec<u8>,
}

static mut TRACE: Vec<TraceEntry> = Vec::new();
static mut SEQ: u32 = 0;
/// Per-kind cursor: the index of the next unconsumed entry of that kind.
static mut CURSOR: [usize; 2] = [0; 2];

/// Record one effect input.
pub fn record(kind: u8, data: Vec<u8>) {
    unsafe {
        SEQ += 1;
        TRACE.push(TraceEntry { kind, seq: SEQ, data });
    }
}

/// Consume the next recorded input of `kind` (deterministic replay).
pub fn replay_next(kind: u8) -> Option<Vec<u8>> {
    unsafe {
        let k = kind as usize;
        if k >= CURSOR.len() {
            return None;
        }
        while CURSOR[k] < TRACE.len() {
            let i = CURSOR[k];
            CURSOR[k] += 1;
            if TRACE[i].kind == kind {
                return Some(TRACE[i].data.clone());
            }
        }
        None
    }
}

/// The trace as a rank-2 table `[seq, kind, data-len]`.
pub fn table() -> Vec<[i64; 3]> {
    unsafe { TRACE.iter().map(|e| [e.seq as i64, e.kind as i64, e.data.len() as i64]).collect() }
}

pub fn count() -> usize {
    unsafe { TRACE.len() }
}
