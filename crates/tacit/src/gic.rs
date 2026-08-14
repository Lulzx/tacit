//! GICv2 interrupt controller (QEMU virt default).
//! GICD at 0x0800_0000, GICC at 0x0801_0000.

const GICD_BASE: usize = 0x0800_0000;
const GICC_BASE: usize = 0x0801_0000;

const GICD_CTLR: usize = 0x000;
const GICD_ISENABLER: usize = 0x100;
const GICD_ICENABLER: usize = 0x180;
const GICD_IPRIORITYR: usize = 0x400;
const GICD_ITARGETSR: usize = 0x800;

const GICC_CTLR: usize = 0x000;
const GICC_PMR: usize = 0x004;
const GICC_IAR: usize = 0x00C;
const GICC_EOIR: usize = 0x010;

pub const UART0_IRQ: u32 = 33; // SPI 33 = 32 + 1

unsafe fn gicd_rd(off: usize) -> u32 {
    core::ptr::read_volatile((GICD_BASE + off) as *const u32)
}
unsafe fn gicd_wr(off: usize, v: u32) {
    core::ptr::write_volatile((GICD_BASE + off) as *mut u32, v)
}
unsafe fn gicc_rd(off: usize) -> u32 {
    core::ptr::read_volatile((GICC_BASE + off) as *const u32)
}
unsafe fn gicc_wr(off: usize, v: u32) {
    core::ptr::write_volatile((GICC_BASE + off) as *mut u32, v)
}

pub fn init() {
    unsafe {
        // Disable distributor and CPU interface
        gicd_wr(GICD_CTLR, 0);
        gicc_wr(GICC_CTLR, 0);

        // Set priority mask to allow all
        gicc_wr(GICC_PMR, 0xFF);

        // Enable UART0 IRQ in the distributor
        let reg = (UART0_IRQ / 32) as usize;
        let bit = 1u32 << (UART0_IRQ % 32);
        gicd_wr(GICD_ISENABLER + reg * 4, bit);

        // Set priority (default) and target CPU 0 (default 0 already).

        // Enable CPU interface and distributor
        gicc_wr(GICC_CTLR, 1);
        gicd_wr(GICD_CTLR, 1);
    }
}

pub fn enable() {
    unsafe {
        gicc_wr(GICC_CTLR, 1);
        gicd_wr(GICD_CTLR, 1);
    }
}

/// Read and acknowledge the current interrupt id (0x3FF if none).
pub fn ack() -> u32 {
    unsafe { gicc_rd(GICC_IAR) & 0x3FF }
}

pub fn eoi(irq: u32) {
    unsafe { gicc_wr(GICC_EOIR, irq) }
}
