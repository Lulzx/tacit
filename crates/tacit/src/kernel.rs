//! The microkernel mechanism layer: regions, capabilities, realms, and the
//! operation-array ABI.  Trusted mechanism only — no filesystem, TCP, POSIX,
//! threads, or vendor compute APIs.  Policy, fusion, placement and the graph
//! live above this in Uiua/UIR.

use uir::{CAP_DISPLAY, CAP_KEYBOARD};

pub const CAP_CLOCK: u8 = 2;
pub const CAP_REGION: u8 = 3;

pub const RIGHTS_READ: u8 = 1;
pub const RIGHTS_WRITE: u8 = 2;

#[derive(Clone)]
pub struct Region {
    pub base: usize,
    pub size: usize, // bytes, page-aligned
    pub home: u8,
    pub cache: u8,
    pub immutable: bool,
    pub refs: u32,
}

pub struct Cap {
    pub token: u64,
    pub kind: u8,
    pub region: i32, // region index, or -1
    pub rights: u8,
    pub nonce: u64,    // the signed value (PAC) or the software token
    pub modifier: u64, // PAC modifier; unused when PAC is off
}

pub struct Realm {
    pub id: u32,
    pub held: alloc::vec::Vec<u64>, // cap tokens
    pub quota: usize,
    pub used: usize,
}

#[derive(Clone, Copy)]
pub struct Counters {
    pub payload_moved: u64,
    pub payload_copied: u64,
    pub kernel_entries: u64,
    /// Kernel entries per engine (indexed by UIR engine code).
    pub engine_entries: [u64; 8],
}

pub struct Kernel {
    pub regions: alloc::vec::Vec<Region>,
    pub caps: alloc::vec::Vec<Cap>,
    pub realms: alloc::vec::Vec<Realm>,
    pub prng: u64,
}

static mut K: Kernel = Kernel {
    regions: alloc::vec::Vec::new(),
    caps: alloc::vec::Vec::new(),
    realms: alloc::vec::Vec::new(),
    prng: 0x9e3779b97f4a7c15,
};

pub static mut COUNTERS: Counters = Counters {
    payload_moved: 0,
    payload_copied: 0,
    kernel_entries: 0,
    engine_entries: [0; 8],
};

pub fn reset_counters() {
    unsafe {
        COUNTERS = Counters {
            payload_moved: 0,
            payload_copied: 0,
            kernel_entries: 0,
            engine_entries: [0; 8],
        };
    }
}

pub fn counters() -> Counters {
    unsafe { COUNTERS }
}

fn next_token() -> u64 {
    unsafe {
        // xorshift64* — a non-cryptographic PRNG.  With FEAT_PACGA enabled
        // the token is `pacga`-signed under a kernel-only key, so arithmetic
        // cannot mint one even in this single-EL guest.
        let mut x = K.prng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        K.prng = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
}

/// A fresh random 64-bit value (also used for the PAC GA key).
pub fn rand64() -> u64 {
    next_token()
}

/// The PAC modifier for a capability record: kind and rights in the high
/// bits, the region index below, so distinct capabilities authenticate
/// against distinct modifiers.
fn cap_modifier(kind: u8, region: i32, rights: u8) -> u64 {
    ((kind as u64) << 48)
        | (((rights as u64) & 0xff) << 32)
        | ((region as u32) as u64)
}

pub fn mint(kind: u8, region: i32, rights: u8) -> u64 {
    unsafe {
        let modifier = cap_modifier(kind, region, rights);
        let nonce = next_token();
        let token = if crate::pac::enabled() {
            crate::pac::sign(nonce, modifier)
        } else {
            nonce
        };
        K.caps.push(Cap { token, kind, region, rights, nonce, modifier });
        token
    }
}

pub fn lookup(token: u64) -> Option<&'static Cap> {
    unsafe {
        for c in K.caps.iter() {
            if crate::pac::enabled() {
                // Recompute the PACGA MAC over the stored nonce: a presented
                // token is valid only if it is exactly the signed nonce.
                if crate::pac::check(token, c.nonce, c.modifier) {
                    return Some(c);
                }
            } else if c.token == token {
                return Some(c);
            }
        }
        None
    }
}

pub fn cap_is(kind: u8, token: u64) -> bool {
    matches!(lookup(token), Some(c) if c.kind == kind)
}

/// Allocate a region for `realm`, counting against its quota.  Returns the
/// region index.
pub fn alloc_region(realm: u32, bytes: usize) -> Option<usize> {
    unsafe {
        let r = K.realms.get_mut(realm as usize)?;
        let page = crate::mem::PAGE_SIZE;
        let rounded = (bytes + page - 1) & !(page - 1);
        if r.used + rounded > r.quota {
            return None; // quota exceeded — clean failure
        }
        let base = crate::mem::alloc_region(rounded)?;
        r.used += rounded;
        K.regions.push(Region {
            base,
            size: rounded,
            home: uir::HOME_UMA,
            cache: 3, // dram
            immutable: false,
            refs: 1,
        });
        Some(K.regions.len() - 1)
    }
}

pub fn free_region(realm: u32, region: usize) {
    unsafe {
        if let Some(r) = K.realms.get_mut(realm as usize) {
            if let Some(reg) = K.regions.get(region) {
                if reg.refs <= 1 {
                    let (base, size) = (reg.base, reg.size);
                    r.used = r.used.saturating_sub(size);
                    crate::mem::free_region(base, size);
                    K.regions[region].base = 0; // mark dead
                } else {
                    K.regions[region].refs -= 1;
                }
            }
        }
    }
}

pub fn region_base(region: usize) -> Option<usize> {
    unsafe { K.regions.get(region).map(|r| r.base) }
}

pub fn region_size(region: usize) -> Option<usize> {
    unsafe { K.regions.get(region).map(|r| r.size) }
}

/// Increment a region's reference count (cap share).  Returns region index.
pub fn share_region(token: u64) -> Option<usize> {
    unsafe {
        let idx = lookup(token)?.region;
        if idx < 0 {
            return None;
        }
        let r = K.regions.get_mut(idx as usize)?;
        if !r.immutable {
            return None; // only immutable regions may be shared without copy
        }
        r.refs += 1;
        Some(idx as usize)
    }
}

/// Increment a region's reference count directly (metadata-only sharing by a
/// value view such as reshape/rows/send).
pub fn region_addref(region: usize) {
    unsafe {
        if let Some(r) = K.regions.get_mut(region) {
            r.refs += 1;
        }
    }
}

/// In-place update of a uniquely-owned region.  A region is in-place mutable
/// only when it has exactly one owner and is not marked immutable; a shared
/// immutable region must be copied instead.
pub fn inplace_update(region: usize, offset: usize, src: *const u8, len: usize) -> bool {
    unsafe {
        let Some(r) = K.regions.get_mut(region) else { return false };
        if r.refs != 1 || r.immutable {
            return false;
        }
        if offset + len > r.size {
            return false;
        }
        core::ptr::copy_nonoverlapping(src, (r.base + offset) as *mut u8, len);
        true
    }
}

/// The capabilities table as arrays (kind, region, rights, held-by-realm0).
pub fn caps_table() -> alloc::vec::Vec<[i64; 4]> {
    unsafe {
        let mut t = alloc::vec::Vec::new();
        for c in K.caps.iter() {
            let held = K.realms.get(0).map(|r| r.held.contains(&c.token)).unwrap_or(false);
            t.push([c.kind as i64, c.region as i64, c.rights as i64, if held { 1 } else { 0 }]);
        }
        t
    }
}

pub fn grant(realm: u32, token: u64) -> bool {
    invalidate_auth_cache();
    unsafe {
        if lookup(token).is_none() {
            return false;
        }
        if let Some(r) = K.realms.get_mut(realm as usize) {
            if !r.held.contains(&token) {
                r.held.push(token);
            }
            return true;
        }
        false
    }
}

pub fn revoke(realm: u32, token: u64) -> bool {
    invalidate_auth_cache();
    unsafe {
        if let Some(r) = K.realms.get_mut(realm as usize) {
            if let Some(pos) = r.held.iter().position(|t| *t == token) {
                r.held.remove(pos);
                return true;
            }
        }
        false
    }
}

pub fn holds(realm: u32, kind: u8) -> bool {
    unsafe {
        if let Some(r) = K.realms.get(realm as usize) {
            for t in r.held.iter() {
                if let Some(c) = lookup(*t) {
                    if c.kind == kind {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Does `realm` hold this specific capability token?
pub fn holds_cap(realm: u32, token: u64) -> bool {
    unsafe {
        if let Some(r) = K.realms.get(realm as usize) {
            r.held.contains(&token) && lookup(token).is_some()
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Capability authorization policy (in Uiua)
// ---------------------------------------------------------------------------
// The *decision* of whether a realm holds a cap of a requested kind is a Uiua
// program (`uiua/authorize.ua`); the *verification* of the capability token
// itself (PAC) stays in Rust.  The decision is memoized per (realm, kind) and
// invalidated on grant/revoke, so the hot path is not a Uiua run per op.

static mut AUTH_PROG: Option<&'static uir::Program> = None;
static mut AUTH_CACHE: [(u32, u8, bool); 16] = [(0, 0, false); 16];
static mut AUTH_CACHE_LEN: usize = 0;

pub fn set_authorize_prog(p: &'static uir::Program) {
    unsafe { AUTH_PROG = Some(p); }
}

fn invalidate_auth_cache() {
    unsafe { AUTH_CACHE_LEN = 0; }
}

/// Does `realm` hold a capability of `kind`?  The answer comes from the Uiua
/// authorize program (memoized per (realm, kind)).
fn authorize(realm: u32, kind: u8) -> bool {
    unsafe {
        for i in 0..AUTH_CACHE_LEN {
            if AUTH_CACHE[i].0 == realm && AUTH_CACHE[i].1 == kind {
                return AUTH_CACHE[i].2;
            }
        }
    }
    let prog = unsafe { AUTH_PROG }.expect("authorize program not set");
    let mut pg = crate::stepper::Graph::new(prog);
    pg.request = Some(kind as i64);
    let popts = crate::stepper::RunOpts { realm, live: None, policy: None, scheduler: None, interactive: false };
    let _ = crate::stepper::run(&mut pg, &popts);
    let count = match pg.last.and_then(|i| pg.vals[i].clone()) {
        Some(v) => unsafe { *(v.data as *const i64) },
        None => 0,
    };
    let authorized = count > 0;
    unsafe {
        if AUTH_CACHE_LEN < AUTH_CACHE.len() {
            AUTH_CACHE[AUTH_CACHE_LEN] = (realm, kind, authorized);
            AUTH_CACHE_LEN += 1;
        }
    }
    authorized
}

/// Return the first capability token of `kind` held by `realm` (0 if none).
pub fn cap_of_kind(realm: u32, kind: u8) -> u64 {
    unsafe {
        if let Some(r) = K.realms.get(realm as usize) {
            for t in r.held.iter() {
                if let Some(c) = lookup(*t) {
                    if c.kind == kind {
                        return *t;
                    }
                }
            }
        }
        0
    }
}

/// Mark a region immutable (a precondition for zero-copy sharing).
pub fn mark_immutable(region: usize) {
    unsafe {
        if let Some(r) = K.regions.get_mut(region) {
            r.immutable = true;
        }
    }
}

/// Mint a region capability token for an already-allocated region.
pub fn mint_region_cap(region: usize) -> u64 {
    mint(CAP_REGION, region as i32, RIGHTS_READ | RIGHTS_WRITE)
}

pub fn realm_count() -> usize {
    unsafe { K.realms.len() }
}

pub fn init(memory_quota: usize) -> u32 {
    unsafe {
        let display = mint(CAP_DISPLAY, -1, RIGHTS_READ | RIGHTS_WRITE);
        let keyboard = mint(CAP_KEYBOARD, -1, RIGHTS_READ);
        let clock = mint(CAP_CLOCK, -1, RIGHTS_READ);
        let realm = Realm {
            id: 0,
            held: alloc::vec![display, keyboard, clock],
            quota: memory_quota,
            used: 0,
        };
        K.realms.push(realm);
        0
    }
}

// ---------------------------------------------------------------------------
// Operation-array ABI
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub enum OpKind {
    Alloc { bytes: usize },
    Map { region: usize },         // region -> shaped array view (metadata)
    Free { region: usize },
    Share { cap: u64 },             // immutable region-cap share
    Copy { cap: u64 },              // explicit copy (bench control)
    Grant { token: u64 },
    Revoke { token: u64 },
    DisplaySend { cap: u64, text: alloc::vec::Vec<u8> },
    KeyboardWait { cap: u64 },
    Clock { cap: u64 },
    InPlace { region: usize, offset: usize, data: alloc::vec::Vec<u8> },
}

/// One entry of an operation array: the op plus an optional dependency on a
/// prior op in the same batch.
#[derive(Clone)]
pub struct Op {
    pub kind: OpKind,
    pub dep: Option<u32>,
}

impl Op {
    pub fn new(kind: OpKind) -> Self {
        Op { kind, dep: None }
    }
    pub fn dep(mut self, idx: u32) -> Self {
        self.dep = Some(idx);
        self
    }
}

#[derive(Clone)]
pub enum OpResult {
    Region(usize),
    Text(alloc::vec::Vec<u8>),
    Clock(u64),
    Ok,
    CapError,
    QuotaError,
    DepError,
}

/// Submit an operation array to the kernel; returns a result array.  The
/// kernel is allowed to batch or reorder independent operations, but an
/// operation whose documented dependency (a prior op in the batch) has not
/// succeeded is refused with `DepError`.
pub fn submit(realm: u32, ops: &[Op]) -> alloc::vec::Vec<OpResult> {
    let mut out = alloc::vec::Vec::with_capacity(ops.len());
    for (i, op) in ops.iter().enumerate() {
        if let Some(dep) = op.dep {
            let dep = dep as usize;
            let ok = dep < i
                && matches!(
                    out.get(dep),
                    Some(OpResult::Ok | OpResult::Region(_) | OpResult::Text(_) | OpResult::Clock(_))
                );
            if !ok {
                out.push(OpResult::DepError);
                continue;
            }
        }
        out.push(exec(realm, &op.kind));
    }
    out
}

fn exec(realm: u32, op: &OpKind) -> OpResult {
    match op {
        OpKind::Alloc { bytes } => match alloc_region(realm, *bytes) {
            Some(r) => {
                let token = mint(CAP_REGION, r as i32, RIGHTS_READ | RIGHTS_WRITE);
                let _ = token;
                OpResult::Region(r)
            }
            None => OpResult::QuotaError,
        },
        OpKind::Free { region } => {
            free_region(realm, *region);
            OpResult::Ok
        }
        OpKind::Map { region } => {
            // map a region into a shaped array view (metadata-only in a single
            // address space; the stepper attaches shape/strides when it builds
            // the array from the region).
            if region_base(*region).is_some() {
                OpResult::Region(*region)
            } else {
                OpResult::CapError
            }
        }
        OpKind::Share { cap } => match share_region(*cap) {
            Some(_) => OpResult::Ok,
            None => OpResult::CapError,
        },
        OpKind::Copy { cap } => {
            // Explicit payload copy: the bench control for zero-copy send.
            let Some(c) = lookup(*cap) else { return OpResult::CapError };
            if c.kind != CAP_REGION || c.region < 0 {
                return OpResult::CapError;
            }
            let idx = c.region as usize;
            let base = region_base(idx);
            let size = region_size(idx);
            let (Some(base), Some(size)) = (base, size) else {
                return OpResult::CapError;
            };
            let Some(nidx) = alloc_region(realm, size) else {
                return OpResult::QuotaError;
            };
            let Some(dst) = region_base(nidx) else { return OpResult::CapError };
            unsafe {
                core::ptr::copy_nonoverlapping(base as *const u8, dst as *mut u8, size);
            }
            unsafe { COUNTERS.payload_copied += size as u64 };
            OpResult::Region(nidx)
        }
        OpKind::Grant { token } => {
            if grant(realm, *token) {
                OpResult::Ok
            } else {
                OpResult::CapError
            }
        }
        OpKind::Revoke { token } => {
            if revoke(realm, *token) {
                OpResult::Ok
            } else {
                OpResult::CapError
            }
        }
        OpKind::DisplaySend { cap, text } => {
            // PAC token verification (cap_is) is the Rust mechanism; whether
            // the realm is *authorized* for the kind is the Uiua policy.
            if !cap_is(CAP_DISPLAY, *cap) || !authorize(realm, CAP_DISPLAY) {
                OpResult::CapError
            } else {
                crate::console_write_bytes(text);
                OpResult::Ok
            }
        }
        OpKind::KeyboardWait { cap } => {
            if !cap_is(CAP_KEYBOARD, *cap) || !authorize(realm, CAP_KEYBOARD) {
                OpResult::CapError
            } else {
                match crate::keyboard_read_line() {
                    Some(line) => OpResult::Text(line),
                    None => OpResult::CapError,
                }
            }
        }
        OpKind::Clock { cap } => {
            if !cap_is(CAP_CLOCK, *cap) || !authorize(realm, CAP_CLOCK) {
                OpResult::CapError
            } else {
                OpResult::Clock(crate::clock_now())
            }
        }
        OpKind::InPlace { region, offset, data } => {
            if inplace_update(*region, *offset, data.as_ptr(), data.len()) {
                OpResult::Ok
            } else {
                OpResult::CapError
            }
        }
    }
}
