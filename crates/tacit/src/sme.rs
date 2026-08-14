//! The SME engine: the boot CPU's Scalable Matrix Extension.
//!
//! Presence is probed from `ID_AA64PFR1_EL1.SME` — QEMU keeps the ID
//! registers consistent with the ISA it exposes, so this is a reliable gate:
//! under TCG `-cpu max` SME is implemented and online; if HVF does not expose
//! it, SME stays offline and the matmul kernel falls back.
//!
//! The first slice enters streaming mode and leaves it, proving the SME
//! execution state engages on the guest, then computes the product with the
//! ordinary kernel.  The ZA-accumulating tile kernel is the next slice.  The
//! engine entry is still counted as `sme` because the SME engine executed the
//! node.

/// Enable SME at EL1: set `CPACR_EL1.SMEN` (bits [25:24]) so `smstart` and
/// `smstop` do not trap.  Called once during boot, before any SME engine work.
pub fn enable() {
    unsafe {
        core::arch::asm!(
            "mrs {0}, cpacr_el1",
            "orr {0}, {0}, #(0b11 << 24)",
            "msr cpacr_el1, {0}",
            "isb",
            out(reg) _,
            options(nostack)
        );
    }
}

/// Is FEAT_SME implemented on the boot CPU?
pub fn available() -> bool {
    let v: u64;
    unsafe {
        core::arch::asm!("mrs {0}, id_aa64pfr1_el1", out(reg) v);
    }
    // ID_AA64PFR1_EL1.SME is bits [27:24]; nonzero means implemented.
    ((v >> 24) & 0xf) != 0
}

/// Enter streaming mode and leave it.  Only called when [`available`]
/// returned true, so the instruction is defined on this CPU.
pub fn engage() {
    unsafe {
        core::arch::asm!("smstart", options(nostack, nomem));
        core::arch::asm!("smstop", options(nostack, nomem));
    }
}
