//! PL011 UART (QEMU virt UART0 at 0x0900_0000).
//! Used for boot diagnostics and as the keyboard input source.

pub const UART0_BASE: usize = 0x0900_0000;

const UARTDR: usize = 0x000;
const UARTFR: usize = 0x018;
const UARTIBRD: usize = 0x024;
const UARTFBRD: usize = 0x028;
const UARTLCR_H: usize = 0x02C;
const UARTCR: usize = 0x030;
const UARTIMSC: usize = 0x038;
const UARTICR: usize = 0x044;

fn reg(off: usize) -> *mut u32 {
    (UART0_BASE + off) as *mut u32
}

unsafe fn rd(off: usize) -> u32 {
    core::ptr::read_volatile(reg(off))
}

unsafe fn wr(off: usize, v: u32) {
    core::ptr::write_volatile(reg(off), v)
}

pub fn init() {
    unsafe {
        // Disable UART
        wr(UARTCR, 0);
        // Baud rate divisors for 115200 @ 24 MHz (QEMU virt clock)
        // IBRD = 24000000 / (16 * 115200) = 13
        // FBRD = round((64 * 24000000) / (16 * 115200) - 64 * 13) = 1
        wr(UARTIBRD, 13);
        wr(UARTFBRD, 1);
        // 8N1, FIFO enabled
        wr(UARTLCR_H, 0x70);
        // Enable UART, TX, RX
        wr(UARTCR, 0x301);
    }
}

pub fn can_read() -> bool {
    unsafe { rd(UARTFR) & (1 << 4) == 0 } // RXFE == 0
}

pub fn read_byte() -> u8 {
    unsafe { rd(UARTDR) as u8 }
}

pub fn can_write() -> bool {
    unsafe { rd(UARTFR) & (1 << 5) == 0 } // TXFF == 0
}

pub fn write_byte(b: u8) {
    unsafe {
        while !can_write() {}
        wr(UARTDR, b as u32)
    }
}

pub fn write_str(s: &str) {
    write_bytes(s.as_bytes());
}

pub fn write_bytes(s: &[u8]) {
    for b in s {
        write_byte(*b);
    }
}

/// Enable the RX interrupt (used to turn key presses into IRQ event records).
pub fn enable_rx_irq() {
    unsafe {
        wr(UARTIMSC, 1 << 4); // RXIM
    }
}

/// Acknowledge the RX interrupt.
pub fn ack_rx_irq() {
    unsafe {
        wr(UARTICR, 1 << 4); // RXIC
    }
}
