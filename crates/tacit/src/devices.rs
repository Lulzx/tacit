//! Input events (keyboard via UART RX) and the clock (arch timer).
//!
//! The machine layer turns hardware IRQs into timestamped event records; no
//! program supplies an ISR.  The UART RX interrupt pushes a character event
//! into a ring buffer; a granted reader drains it into a character line.

const KEYBUF_LEN: usize = 512;

static mut KEYBUF: [u8; KEYBUF_LEN] = [0; KEYBUF_LEN];
static mut KEY_HEAD: usize = 0; // producer (IRQ context)
static mut KEY_TAIL: usize = 0; // consumer (main context)

/// Number of events currently buffered.
fn key_pending() -> usize {
    unsafe { KEY_HEAD.wrapping_sub(KEY_TAIL) & (KEYBUF_LEN - 1) }
}

/// Push a character event from interrupt context.
pub fn push_key_event(ch: u8) {
    unsafe {
        let next = (KEY_HEAD + 1) & (KEYBUF_LEN - 1);
        if next != KEY_TAIL {
            KEYBUF[KEY_HEAD] = ch;
            KEY_HEAD = next;
        }
    }
}

/// Pop one character event (non-blocking).
pub fn pop_key_event() -> Option<u8> {
    unsafe {
        if KEY_HEAD == KEY_TAIL {
            return None;
        }
        let ch = KEYBUF[KEY_TAIL];
        KEY_TAIL = (KEY_TAIL + 1) & (KEYBUF_LEN - 1);
        Some(ch)
    }
}

pub fn have_key_event() -> bool {
    unsafe { KEY_HEAD != KEY_TAIL }
}

/// Read a full line (characters until Enter).  Blocks (wfi) until complete.
/// Returns the line bytes without the newline, or None if the reader is not
/// authorized (handled by the caller via capabilities).
pub fn read_line() -> alloc::vec::Vec<u8> {
    let mut line = alloc::vec::Vec::new();
    loop {
        if let Some(ch) = pop_key_event() {
            if ch == b'\r' || ch == b'\n' {
                return line;
            }
            if ch == 0x7f || ch == 0x08 {
                // backspace: remove last char
                line.pop();
                continue;
            }
            if ch >= 0x20 && ch < 0x7f {
                line.push(ch);
            }
        } else {
            unsafe {
                core::arch::asm!("wfi");
            }
        }
    }
}

pub fn pending_count() -> usize {
    key_pending()
}

// ---------------------------------------------------------------------------
// Clock (ARM architectural timer)
// ---------------------------------------------------------------------------

pub fn clock_ticks() -> u64 {
    let mut v: u64;
    unsafe {
        core::arch::asm!("mrs {0}, cntpct_el0", out(reg) v);
    }
    v
}

pub fn clock_freq() -> u64 {
    let mut v: u64;
    unsafe {
        core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) v);
    }
    v
}

/// Microseconds since boot (a monotonic clock).
pub fn clock_us() -> u64 {
    let freq = clock_freq();
    if freq == 0 {
        return 0;
    }
    clock_ticks() / (freq / 1_000_000)
}
