#![no_std]
#![no_main]

extern crate alloc;

mod console;
mod devices;
mod fdt;
mod fmt;
mod fwcfg;
mod gic;
mod kernel;
mod machine;
mod mem;
mod mmu;
mod objects;
mod shell;
mod stepper;
mod trace;
mod uart;

use core::arch::global_asm;
use core::panic::PanicInfo;
use uir::Program;
use stepper::{Graph, RunOpts};

global_asm!(include_str!("boot.s"));

// UIR payloads produced by the host compiler (see build.sh).
static TINY_UIR: &[u8] = include_bytes!("../embedded/tiny.uir");
static AGENT_UIR: &[u8] = include_bytes!("../embedded/agent.uir");
static SUBSET_UIR: &[u8] = include_bytes!("../embedded/subset.uir");
static MACHINE_UIR: &[u8] = include_bytes!("../embedded/machine.uir");
static GRAPH_UIR: &[u8] = include_bytes!("../embedded/graph.uir");
static PROVENANCE_UIR: &[u8] = include_bytes!("../embedded/provenance.uir");
static OBJECTS_UIR: &[u8] = include_bytes!("../embedded/objects.uir");
static REPLAY_UIR: &[u8] = include_bytes!("../embedded/replay.uir");
static BENCH_SEND_UIR: &[u8] = include_bytes!("../embedded/bench-send.uir");
static POLICY_UIR: &[u8] = include_bytes!("../embedded/policy.uir");
static BENCH_FUSED_UIR: &[u8] = include_bytes!("../embedded/bench-fused.uir");
static BENCH_UNFUSED_UIR: &[u8] = include_bytes!("../embedded/bench-unfused.uir");

static mut FDT_BUF: [u8; 1024 * 1024] = [0; 1024 * 1024];
static mut CONSOLE: Option<console::Console> = None;

static mut TINY: Option<Program> = None;
static mut AGENT: Option<Program> = None;
static mut SUBSET: Option<Program> = None;
static mut MACHINE: Option<Program> = None;
static mut GRAPH: Option<Program> = None;
static mut PROVENANCE: Option<Program> = None;
static mut OBJPROG: Option<Program> = None;
static mut REPLAY: Option<Program> = None;
static mut BENCH_SEND: Option<Program> = None;
static mut POLICY: Option<Program> = None;
static mut BENCH_FUSED: Option<Program> = None;
static mut BENCH_UNFUSED: Option<Program> = None;

macro_rules! prog {
    ($slot:ident) => {
        unsafe { $slot.as_ref().expect("program not decoded") }
    };
}

// ---------------------------------------------------------------------------
// Global device access used by the kernel/stepper layers
// ---------------------------------------------------------------------------

pub fn console_write_bytes(b: &[u8]) {
    unsafe {
        if let Some(c) = &mut CONSOLE {
            c.write_bytes(b);
            c.flush();
        }
    }
    uart::write_bytes(b);
}

pub fn console_write_str(s: &str) {
    console_write_bytes(s.as_bytes());
}

pub fn console_char_count() -> usize {
    unsafe { CONSOLE.as_ref().map(|c| c.char_count()).unwrap_or(0) }
}

pub fn clock_now() -> u64 {
    devices::clock_us()
}

pub fn keyboard_read_line() -> Option<alloc::vec::Vec<u8>> {
    Some(devices::read_line())
}

pub fn enable_irqs() {
    unsafe {
        core::arch::asm!("msr daifclr, #2");
    }
}

#[no_mangle]
pub extern "C" fn kernel_main(fdt_ptr: usize) -> ! {
    uart::init();
    uart::write_str("\n[boot] Tacit AArch64 stub\n");

    // --- physical memory map (the guest's only authority for RAM) ---
    let fdt = unsafe {
        let mut chosen = 0usize;
        if fdt_ptr != 0 && core::ptr::read_volatile(fdt_ptr as *const u32).to_be() == 0xd00dfeed {
            chosen = fdt_ptr;
        } else {
            let mut a = 0x4000_0000usize;
            while a < 0x4080_0000 {
                if core::ptr::read_volatile(a as *const u32).to_be() == 0xd00dfeed {
                    chosen = a;
                    break;
                }
                a += 8;
            }
        }
        if chosen != 0 {
            let size = core::ptr::read_volatile((chosen + 4) as *const u32).to_be() as usize;
            let n = size.min(FDT_BUF.len());
            core::ptr::copy_nonoverlapping(chosen as *const u8, FDT_BUF.as_mut_ptr(), n);
            &FDT_BUF[..n]
        } else {
            &[]
        }
    };

    let map = match fdt::parse(fdt) {
        Ok(Some(m)) => m,
        Ok(None) => boot_fault("no memory map in device tree"),
        Err(()) => boot_fault("malformed device tree"),
    };

    // --- enable the MMU: identity-mapped 16 KiB-granule page tables + caches ---
    mmu::init(map.base, map.size);

    // --- display (framebuffer) at the top of RAM ---
    let fb_addr = ((map.base + map.size - console::FB_BYTES as u64) as usize) & !(mem::PAGE_SIZE - 1);
    match fwcfg::configure_ramfb(fb_addr as u64, console::FB_WIDTH, console::FB_HEIGHT) {
        Some(_) => {}
        None => boot_fault("display init failed (ramfb)"),
    }

    let mut con = console::Console::new(fb_addr);
    con.clear();
    unsafe { CONSOLE = Some(con) };

    // --- allocator: image .. display (structurally excludes image/stack/display) ---
    let image_end = unsafe { &__end as *const _ as usize };
    mem::init(image_end, fb_addr);

    // --- machine description + microkernel (one Realm, starter caps, quota) ---
    let quota = fb_addr - image_end;
    kernel::init(quota);
    gic::init();
    uart::enable_rx_irq();

    // --- decode UIR payloads ---
    unsafe {
        TINY = Some(uir::decode(TINY_UIR).unwrap());
        AGENT = Some(uir::decode(AGENT_UIR).unwrap());
        SUBSET = Some(uir::decode(SUBSET_UIR).unwrap());
        MACHINE = Some(uir::decode(MACHINE_UIR).unwrap());
        GRAPH = Some(uir::decode(GRAPH_UIR).unwrap());
        PROVENANCE = Some(uir::decode(PROVENANCE_UIR).unwrap());
        OBJPROG = Some(uir::decode(OBJECTS_UIR).unwrap());
        REPLAY = Some(uir::decode(REPLAY_UIR).unwrap());
        BENCH_SEND = Some(uir::decode(BENCH_SEND_UIR).unwrap());
        POLICY = Some(uir::decode(POLICY_UIR).unwrap());
        BENCH_FUSED = Some(uir::decode(BENCH_FUSED_UIR).unwrap());
        BENCH_UNFUSED = Some(uir::decode(BENCH_UNFUSED_UIR).unwrap());
    }

    // =======================================================================
    // Ready.  From here on, everything that runs is a Uiua program compiled
    // to UIR; the Rust layer only loads and steps them.
    // =======================================================================
    console_write_str("\n=== Tacit ready ===\n");
    console_write_str("Tacit: OS = values + transformations + capabilities + placement\n\n");

    run_uir("machine", prog!(MACHINE), None, None);
    console_write_str("\n");

    // The bundled program is the live graph the rest of the demo inspects.
    let mut tg = Graph::new(prog!(TINY));
    let top = RunOpts { realm: 0, live: None, policy: None, interactive: false };
    if let Err(e) = stepper::run(&mut tg, &top) {
        console_write_str("[tiny] runtime error: ");
        console_write_str(e);
        console_write_str(" (realm idle, no reset)\n");
    }
    tg.release_all();
    console_write_str("\n");

    run_uir("graph", prog!(GRAPH), Some(&tg), None);
    console_write_str("\n");

    run_uir("provenance", prog!(PROVENANCE), Some(&tg), None);
    console_write_str("\n");

    run_uir("agent", prog!(AGENT), Some(&tg), Some(prog!(POLICY)));
    console_write_str("\n");

    run_uir("subset", prog!(SUBSET), Some(&tg), None);
    console_write_str("\n");

    run_uir("objects", prog!(OBJPROG), None, None);
    console_write_str("\n");

    run_uir("replay", prog!(REPLAY), None, None);
    console_write_str("\n");

    // Mechanism self-test: capability enforcement (revoke / forge / unmet
    // dependency / in-place mutation) exercised at the operation-array ABI.
    kernel_selftest();

    // Benches: the programs report their own counters.
    console_write_str("(fused run)\n");
    run_uir("bench-fused", prog!(BENCH_FUSED), None, None);
    let fused_bytes = kernel::counters().payload_moved;
    console_write_str("(unfused run)\n");
    run_uir("bench-unfused", prog!(BENCH_UNFUSED), None, None);
    let unfused_bytes = kernel::counters().payload_moved;
    console_write_str(if fused_bytes < unfused_bytes {
        "fused moves fewer bytes (ok)\n"
    } else {
        "fused did NOT move fewer bytes (FAIL)\n"
    });
    console_write_str("\n");

    run_uir("bench-send", prog!(BENCH_SEND), None, None);
    console_write_str("\n");

    // =======================================================================
    // Keyboard -> Uiua shell.  The guest compiles and steps typed Uiua lines
    // itself (same compiler source as the host), so bindings persist as
    // values and there is no fixed echo program.
    // =======================================================================
    console_write_str("\n--- Uiua shell ---\n");
    console_write_str("type Uiua lines; bindings persist as values.\n");
    enable_irqs();
    shell::run();
}

// ---------------------------------------------------------------------------
// Demo helpers
// ---------------------------------------------------------------------------

fn run_uir(name: &str, prog: &'static Program, live: Option<&Graph<'static>>, policy: Option<&'static Program>) {
    let mut g = Graph::new(prog);
    let opts = RunOpts { realm: 0, live, policy, interactive: false };
    if let Err(e) = stepper::run(&mut g, &opts) {
        console_write_str("[");
        console_write_str(name);
        console_write_str("] runtime error: ");
        console_write_str(e);
        console_write_str(" (realm idle, no reset)\n");
    }
    g.release_all();
}

fn display_cap_token() -> u64 {
    kernel::cap_of_kind(0, uir::CAP_DISPLAY)
}

/// Kernel mechanism self-test: capability enforcement at the operation-array
/// ABI.  This is a test of the microkernel, not policy, so it lives in Rust.
fn kernel_selftest() {
    console_write_str("--- kernel self-test (capability enforcement) ---\n");

    // revoke -> write rejected, console unchanged
    let tok = display_cap_token();
    let _ = kernel::submit(0, &alloc::vec![kernel::Op::new(kernel::OpKind::Revoke { token: tok })]);
    let before = console_char_count();
    let failed = alloc::vec![kernel::Op::new(kernel::OpKind::DisplaySend {
        cap: tok,
        text: alloc::vec::Vec::from(&b"should not appear"[..]),
    })];
    let res = kernel::submit(0, &failed);
    let after = console_char_count();
    if matches!(res[0], kernel::OpResult::CapError) && before == after {
        console_write_str("  revoked display cap: write rejected, console unchanged (ok)\n");
    } else {
        console_write_str("  revoked display cap: FAILED\n");
    }
    let _ = kernel::submit(0, &alloc::vec![kernel::Op::new(kernel::OpKind::Grant { token: tok })]);

    // forged integer/bytes as a display cap
    let forged = 0x1234_5678_9abc_def0u64;
    let forge = alloc::vec![kernel::Op::new(kernel::OpKind::DisplaySend {
        cap: forged,
        text: alloc::vec::Vec::from(&b"forged"[..]),
    })];
    if matches!(kernel::submit(0, &forge)[0], kernel::OpResult::CapError) {
        console_write_str("  forged integer as display cap: rejected (ok)\n");
    } else {
        console_write_str("  forged integer as display cap: FAILED\n");
    }

    // operation-array batching with a dependency, and an unmet dependency
    let batch = alloc::vec![
        kernel::Op::new(kernel::OpKind::Clock { cap: kernel::cap_of_kind(0, kernel::CAP_CLOCK) }),
        kernel::Op::new(kernel::OpKind::DisplaySend {
            cap: tok,
            text: alloc::vec::Vec::from(&b"batch: dependent display send"[..]),
        })
        .dep(0),
    ];
    let bres = kernel::submit(0, &batch);
    if matches!(bres[0], kernel::OpResult::Clock(_)) && matches!(bres[1], kernel::OpResult::Ok) {
        console_write_str("  op-array batch: dependent send after clock (ok)\n");
    } else {
        console_write_str("  op-array batch: FAILED\n");
    }
    let unmet = alloc::vec![kernel::Op::new(kernel::OpKind::DisplaySend {
        cap: tok,
        text: alloc::vec::Vec::from(&b"unmet dependency"[..]),
    })
    .dep(5)];
    if matches!(kernel::submit(0, &unmet)[0], kernel::OpResult::DepError) {
        console_write_str("  op with unmet dependency: refused (ok)\n");
    } else {
        console_write_str("  op with unmet dependency: FAILED\n");
    }

    // unique-region in-place mutation vs immutable refusal
    let ur = kernel::alloc_region(0, 16 * 1024).unwrap();
    let patch = [42u8; 8];
    let ip = alloc::vec![kernel::Op::new(kernel::OpKind::InPlace {
        region: ur,
        offset: 0,
        data: alloc::vec::Vec::from(&patch[..]),
    })];
    console_write_str("  unique region in-place update: ");
    console_write_str(if matches!(kernel::submit(0, &ip)[0], kernel::OpResult::Ok) { "allowed (ok)\n" } else { "refused (FAIL)\n" });
    kernel::mark_immutable(ur);
    let ip2 = alloc::vec![kernel::Op::new(kernel::OpKind::InPlace {
        region: ur,
        offset: 0,
        data: alloc::vec::Vec::from(&patch[..]),
    })];
    console_write_str("  immutable (shared) region in-place update: ");
    console_write_str(if matches!(kernel::submit(0, &ip2)[0], kernel::OpResult::CapError) { "refused, copy required (ok)\n" } else { "incorrectly allowed (FAIL)\n" });
    kernel::free_region(0, ur);
    console_write_str("\n");
}

// ---------------------------------------------------------------------------
// Boot / fault helpers
// ---------------------------------------------------------------------------

extern "C" {
    static __end: u8;
}

pub fn hex64(v: u64) -> [u8; 16] {
    const DIG: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[15 - i] = DIG[((v >> (i * 4)) & 0xf) as usize];
    }
    out
}

pub fn dec(mut v: usize) -> [u8; 20] {
    let mut out = [0u8; 20];
    let mut i = 20;
    if v == 0 {
        i -= 1;
        out[i] = b'0';
    }
    while v > 0 {
        i -= 1;
        out[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    let mut s = [0u8; 20];
    let n = 20 - i;
    s[..n].copy_from_slice(&out[i..]);
    s
}

fn boot_fault(msg: &str) -> ! {
    uart::write_str("[boot] FAULT: ");
    uart::write_str(msg);
    uart::write_str("\n");
    halt();
}

pub fn halt() -> ! {
    unsafe {
        core::arch::asm!("wfi");
    }
    loop {}
}

#[no_mangle]
pub extern "C" fn sync_exception(esr: u64, elr: u64, far: u64) -> ! {
    uart::write_str("\n[sync] FAULT esr=0x");
    uart::write_bytes(&hex64(esr));
    uart::write_str(" elr=0x");
    uart::write_bytes(&hex64(elr));
    uart::write_str(" far=0x");
    uart::write_bytes(&hex64(far));
    uart::write_str("\n");
    halt();
}

#[no_mangle]
pub extern "C" fn irq_dispatch() {
    let irq = gic::ack();
    if irq == gic::UART0_IRQ {
        uart::ack_rx_irq();
        while uart::can_read() {
            let ch = uart::read_byte();
            devices::push_key_event(ch);
        }
    }
    gic::eoi(irq);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    uart::write_str("\n[panic] ");
    if let Some(s) = info.message().as_str() {
        uart::write_str(s);
    }
    if let Some(loc) = info.location() {
        uart::write_str(" @ ");
        uart::write_str(loc.file());
    }
    uart::write_str("\n");
    halt();
}
