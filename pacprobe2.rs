#![no_std]
#[no_mangle]
pub unsafe fn probe(data: u64, m: u64) -> u64 {
    let mut o: u64;
    core::arch::asm!("pacga {0}, {1}, {2}", out(reg) o, in(reg) data, in(reg) m, options(nostack));
    o
}
