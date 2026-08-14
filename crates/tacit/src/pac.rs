//! Pointer authentication (FEAT_PACGA): capability tokens are the `pacga` MAC
//! of a kernel-generated nonce under a kernel-only GA key, so a Realm cannot
//! mint one by arithmetic.  The kernel verifies a presented token by
//! recomputing the MAC over the stored nonce (equivalent to `autga`; this
//! LLVM does not assemble the reverse instruction).
//!
//! Presence is probed from `ID_AA64ISAR1_EL1.GPI` (QEMU `-cpu max` and the M4
//! both implement FEAT_PACGA).  When absent, the kernel falls back to software
//! unforgeability (random table tokens), which the capability spec already
//! allows on virt.

static mut ENABLED: bool = false;

/// Turn on PAC capability signing: set a fresh GA key and enable `sign`/
/// `verify`.  Called once during boot, before any capability is minted.
pub fn init() {
    let isar1: u64;
    unsafe {
        core::arch::asm!("mrs {0}, id_aa64isar1_el1", out(reg) isar1);
    }
    // ID_AA64ISAR1_EL1.GPI (bits [11:8]) != 0 means FEAT_PACGA.
    if ((isar1 >> 8) & 0xf) == 0 {
        return;
    }
    let lo = crate::kernel::rand64();
    let hi = crate::kernel::rand64();
    unsafe {
        core::arch::asm!("msr apgakeylo_el1, {0}", in(reg) lo, options(nostack));
        core::arch::asm!("msr apgakeyhi_el1, {0}", in(reg) hi, options(nostack));
        core::arch::asm!("isb");
    }
    unsafe { ENABLED = true };
}

pub fn enabled() -> bool {
    unsafe { ENABLED }
}

/// Sign `data` with the GA key and `modifier`: the capability token.
pub fn sign(data: u64, modifier: u64) -> u64 {
    let mut out: u64;
    unsafe {
        core::arch::asm!(
            "pacga {0}, {1}, {2}",
            out(reg) out,
            in(reg) data,
            in(reg) modifier,
            options(nostack)
        );
    }
    out
}

/// Verify `token` against the stored `nonce`/`modifier`: recompute the MAC.
/// Equivalent to `autga` (the kernel holds the nonce, so it does not need the
/// reverse instruction, which this LLVM does not assemble).
pub fn check(token: u64, nonce: u64, modifier: u64) -> bool {
    sign(nonce, modifier) == token
}
