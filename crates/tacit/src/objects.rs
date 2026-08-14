//! The content-addressed object store: `id = H(data)`.
//!
//! Storing a value registers a copy keyed by a deterministic hash of its
//! descriptor and payload; loading by that id returns the same value, so two
//! stores of equal data deduplicate to one object.  The store holds *values*,
//! not bulk payloads: a payload over [`OBJECT_LIMIT`] is refused and stays on
//! the datapath (the zero-copy send path is for that).

use crate::stepper::Value;

const OBJECT_LIMIT: usize = 64 * 1024;
const MAX_OBJECTS: usize = 64;

pub struct StoredObject {
    pub id: u64,
    pub dtype: u8,
    pub rank: u8,
    pub shape: [usize; 4],
    pub payload: alloc::vec::Vec<u8>,
}

static mut OBJECTS: alloc::vec::Vec<StoredObject> = alloc::vec::Vec::new();

/// FNV-1a 64 over the value descriptor and payload: a deterministic content
/// id.  Equal data always yields the same id; unequal data collides only by
/// the usual 64-bit hash odds.
pub fn content_id(v: &Value) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |b: u8| {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    mix(v.dtype);
    mix(v.rank);
    for s in v.shape.iter() {
        for i in 0..8 {
            mix(((s >> (i * 8)) & 0xff) as u8);
        }
    }
    let bytes = v.byte_len();
    if bytes > 0 {
        unsafe {
            let p = v.data as *const u8;
            for i in 0..bytes {
                mix(*p.add(i));
            }
        }
    }
    h
}

/// Register `v` under its content id (deduplicating), copying the payload.
/// Returns the id, or `None` when the store is full or the payload is too
/// large for a *value* (bulk payloads belong to the datapath, not the store).
pub fn store(v: &Value) -> Option<u64> {
    let id = content_id(v);
    unsafe {
        if OBJECTS.iter().any(|o| o.id == id) {
            return Some(id);
        }
        if OBJECTS.len() >= MAX_OBJECTS || v.byte_len() > OBJECT_LIMIT {
            return None;
        }
        let mut payload = alloc::vec![0u8; v.byte_len()];
        if payload.len() > 0 {
            core::ptr::copy_nonoverlapping(v.data as *const u8, payload.as_mut_ptr(), payload.len());
        }
        OBJECTS.push(StoredObject { id, dtype: v.dtype, rank: v.rank, shape: v.shape, payload });
        Some(id)
    }
}

/// Fetch the value registered under `id`.
pub fn load(id: u64) -> Option<(u8, u8, [usize; 4], alloc::vec::Vec<u8>)> {
    unsafe {
        OBJECTS
            .iter()
            .find(|o| o.id == id)
            .map(|o| (o.dtype, o.rank, o.shape, o.payload.clone()))
    }
}

/// Number of distinct objects currently registered.
pub fn count() -> usize {
    unsafe { OBJECTS.len() }
}
