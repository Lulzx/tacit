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
mod stepper;
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
static POLICY_UIR: &[u8] = include_bytes!("../embedded/policy.uir");
static ECHO_UIR: &[u8] = include_bytes!("../embedded/echo.uir");
static BENCH_FUSED_UIR: &[u8] = include_bytes!("../embedded/bench-fused.uir");
static BENCH_UNFUSED_UIR: &[u8] = include_bytes!("../embedded/bench-unfused.uir");

static mut FDT_BUF: [u8; 1024 * 1024] = [0; 1024 * 1024];
static mut CONSOLE: Option<console::Console> = None;

static mut TINY: Option<Program> = None;
static mut AGENT: Option<Program> = None;
static mut SUBSET: Option<Program> = None;
static mut POLICY: Option<Program> = None;
static mut ECHO: Option<Program> = None;
static mut BENCH_FUSED: Option<Program> = None;
static mut BENCH_UNFUSED: Option<Program> = None;

fn slot(p: &'static mut Option<Program>) -> &'static Program {
    unsafe { p.as_ref().expect("program not decoded") }
}

fn tiny_prog() -> &'static Program {
    unsafe { slot(&mut TINY) }
}
fn agent_prog() -> &'static Program {
    unsafe { slot(&mut AGENT) }
}
fn subset_prog() -> &'static Program {
    unsafe { slot(&mut SUBSET) }
}
fn policy_prog() -> &'static Program {
    unsafe { slot(&mut POLICY) }
}
fn echo_prog() -> &'static Program {
    unsafe { slot(&mut ECHO) }
}
fn bench_fused_prog() -> &'static Program {
    unsafe { slot(&mut BENCH_FUSED) }
}
fn bench_unfused_prog() -> &'static Program {
    unsafe { slot(&mut BENCH_UNFUSED) }
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
        POLICY = Some(uir::decode(POLICY_UIR).unwrap());
        ECHO = Some(uir::decode(ECHO_UIR).unwrap());
        BENCH_FUSED = Some(uir::decode(BENCH_FUSED_UIR).unwrap());
        BENCH_UNFUSED = Some(uir::decode(BENCH_UNFUSED_UIR).unwrap());
    }

    // =======================================================================
    // Ready banner + machine description
    // =======================================================================
    console_write_str("\n=== Tacit ready ===\n");
    console_write_str("Tacit: OS = values + transformations + capabilities + placement\n");
    machine::print();
    console_write_str("\n");

    // =======================================================================
    // Bundled tiny program: C = (A + B) x D, graph still inspectable
    // =======================================================================
    console_write_str("--- bundled program: C = (A + B) x D ---\n");
    let tiny = tiny_prog();
    graph_projection(tiny);
    console_write_str("result:\n");
    let mut tg = Graph::new(tiny);
    let opts = RunOpts { realm: 0, live: None, policy: None, interactive: false };
    match stepper::run(&mut tg, &opts) {
        Ok(()) => {}
        Err(e) => {
            console_write_str("runtime error: ");
            console_write_str(e);
            console_write_str(" (realm idle, no reset)\n");
        }
    }
    provenance(tiny, 4);
    console_write_str("node resource counters (payload bytes moved):\n");
    for (i, nd) in tiny.nodes.iter().enumerate() {
        if tg.node_bytes[i] > 0 {
            let mut s = alloc::vec::Vec::new();
            fmt::append_str(&mut s, "  #");
            fmt::append_u64(&mut s, i as u64);
            fmt::append_str(&mut s, " ");
            fmt::append_str(&mut s, &tiny.name(i));
            fmt::append_str(&mut s, ": ");
            fmt::append_u64(&mut s, tg.node_bytes[i]);
            fmt::append_str(&mut s, "\n");
            console_write_bytes(&s);
        }
    }
    console_write_str("\n");

    // =======================================================================
    // One granted agent-shaped transform: summarize the live graph
    // =======================================================================
    console_write_str("--- agent: one granted transform over the live graph ---\n");
    console_write_str("(two independent summaries fan out from one graph query; the\n");
    console_write_str(" scheduler policy — Uiua compiled to UIR — orders them)\n");
    let agent = agent_prog();
    let mut ag = Graph::new(agent);
    let aopts = RunOpts { realm: 0, live: Some(&tg), policy: Some(policy_prog()), interactive: false };
    if let Err(e) = stepper::run(&mut ag, &aopts) {
        console_write_str("agent runtime error: ");
        console_write_str(e);
        console_write_str("\n");
    }
    console_write_str("\n");

    // =======================================================================
    // Subset coverage: reduce, reshape, rank-wise map, capabilities table
    // =======================================================================
    console_write_str("--- subset: reduce, reshape, rank-wise map, capabilities ---\n");
    let mut sg = Graph::new(subset_prog());
    let sopts = RunOpts { realm: 0, live: Some(&tg), policy: None, interactive: false };
    if let Err(e) = stepper::run(&mut sg, &sopts) {
        console_write_str("subset runtime error: ");
        console_write_str(e);
        console_write_str("\n");
    }
    sg.release_all();
    console_write_str("\n");

    // =======================================================================
    // Effects: propose -> simulate -> validate -> commit
    // =======================================================================
    effects_demo();

    // =======================================================================
    // Fusion and zero-copy benches
    // =======================================================================
    bench_fusion();
    bench_send();

    // =======================================================================
    // Keyboard -> event array -> character line (echo)
    // =======================================================================
    console_write_str("--- keyboard ---\n");
    console_write_str("type a line and press Enter:\n");
    enable_irqs();
    loop {
        let mut eg = Graph::new(echo_prog());
        let eopts = RunOpts { realm: 0, live: None, policy: None, interactive: true };
        let _ = stepper::run(&mut eg, &eopts);
        eg.release_all();
    }
}

// ---------------------------------------------------------------------------
// Demo helpers
// ---------------------------------------------------------------------------

fn graph_projection(prog: &Program) {
    console_write_str("live graph (nodes, edges, shapes, engine, home):\n");
    let g = Graph::new(prog);
    for (i, nd) in prog.nodes.iter().enumerate() {
        let mut s = alloc::vec::Vec::new();
        fmt::append_str(&mut s, "  #");
        fmt::append_u64(&mut s, i as u64);
        fmt::append_str(&mut s, " ");
        fmt::append_str(&mut s, &prog.name(i));
        fmt::append_str(&mut s, "  ");
        fmt::append_str(&mut s, uir::dtype_name(nd.dtype));
        fmt::append_str(&mut s, "[");
        fmt::append_u64(&mut s, nd.shape[0] as u64);
        for d in 1..nd.rank as usize {
            fmt::append_str(&mut s, "x");
            fmt::append_u64(&mut s, nd.shape[d] as u64);
        }
        fmt::append_str(&mut s, "]  ");
        fmt::append_str(&mut s, if nd.pure { "pure" } else { "effect" });
        fmt::append_str(&mut s, "  engine=");
        fmt::append_str(&mut s, uir::engine_name(nd.engine));
        fmt::append_str(&mut s, " home=");
        fmt::append_str(&mut s, if nd.home == uir::HOME_UMA { "uma" } else { "?" });
        if nd.cap_need != uir::CAP_NONE {
            fmt::append_str(&mut s, " cap=");
            fmt::append_str(&mut s, if nd.cap_need == uir::CAP_DISPLAY { "display" } else { "keyboard" });
        }
        if nd.parallel_axis != 0 {
            fmt::append_str(&mut s, " parallel-axis=");
            fmt::append_u64(&mut s, nd.parallel_axis as u64);
        }
        if nd.in0 != uir::NONE || nd.in1 != uir::NONE || nd.in2 != uir::NONE {
            fmt::append_str(&mut s, "  <- (");
            if nd.in0 != uir::NONE {
                fmt::append_u64(&mut s, nd.in0 as u64);
            }
            if nd.in1 != uir::NONE {
                fmt::append_str(&mut s, ", ");
                fmt::append_u64(&mut s, nd.in1 as u64);
            }
            if nd.in2 != uir::NONE {
                fmt::append_str(&mut s, ", ");
                fmt::append_u64(&mut s, nd.in2 as u64);
            }
            fmt::append_str(&mut s, ")");
        }
        fmt::append_str(&mut s, "\n");
        console_write_bytes(&s);
    }
    let _ = g;
}

fn provenance(prog: &Program, node: u32) {
    let nd = &prog.nodes[node as usize];
    let mut s = alloc::vec::Vec::new();
    fmt::append_str(&mut s, "provenance of C: producer ");
    fmt::append_str(&mut s, &prog.name(node as usize));
    fmt::append_str(&mut s, " (node #");
    fmt::append_u64(&mut s, node as u64);
    fmt::append_str(&mut s, "), inputs ");
    let mut first = true;
    for inp in [nd.in0, nd.in1, nd.in2] {
        if inp != uir::NONE {
            if !first {
                fmt::append_str(&mut s, " and ");
            }
            first = false;
            fmt::append_str(&mut s, &prog.name(inp as usize));
            fmt::append_str(&mut s, " (node #");
            fmt::append_u64(&mut s, inp as u64);
            fmt::append_str(&mut s, ")");
        }
    }
    fmt::append_str(&mut s, "\n");
    console_write_bytes(&s);
}

fn effects_demo() {
    console_write_str("--- effects: propose -> simulate -> validate -> commit ---\n");

    // Build a small text value to "display".
    let mut v = alloc::vec::Vec::new();
    v.extend_from_slice(b"effects demo: committed after validate");
    let region = kernel::alloc_region(0, v.len()).unwrap();
    let data = kernel::region_base(region).unwrap();
    unsafe { core::ptr::copy_nonoverlapping(v.as_ptr(), data as *mut u8, v.len()) };
    let val = stepper::Value {
        data,
        dtype: uir::DTYPE_U8,
        rank: 1,
        shape: [v.len(), 1, 1, 1],
        region: Some(region),
    };

    // simulate: prediction without touching the console
    let before = console_char_count();
    let predicted = stepper::predict_text(&val);
    let after_sim = console_char_count();
    console_write_str("  simulate: predicted \"");
    console_write_bytes(&predicted);
    console_write_str("\"");
    if after_sim == before {
        console_write_str("  console unchanged during simulate (ok)\n");
    }
    console_write_str("  prediction marks reversibility: irreversible (console contents are not retained for undo in this milestone)\n");
    // validate + commit via the operation-array ABI (display send)
    let caps: alloc::vec::Vec<kernel::Op> = {
        let mut c = alloc::vec::Vec::new();
        c.push(kernel::Op::new(kernel::OpKind::DisplaySend {
            cap: display_cap_token(),
            text: predicted,
        }));
        c
    };
    let res = kernel::submit(0, &caps);
    match &res[0] {
        kernel::OpResult::Ok => console_write_str("  commit: console updated\n"),
        _ => console_write_str("  commit: FAILED (cap error)\n"),
    }

    // missing-cap path: revoke display, attempt write, verify unchanged
    console_write_str("  revoking display capability...\n");
    let tok = display_cap_token();
    let _ = kernel::submit(0, &alloc::vec![kernel::Op::new(kernel::OpKind::Revoke { token: tok })]);
    let before2 = console_char_count();
    let failed: alloc::vec::Vec<kernel::Op> =
        alloc::vec![kernel::Op::new(kernel::OpKind::DisplaySend { cap: tok, text: alloc::vec::Vec::from(&b"should not appear"[..]) })];
    let res2 = kernel::submit(0, &failed);
    let after2 = console_char_count();
    if matches!(res2[0], kernel::OpResult::CapError) && before2 == after2 {
        console_write_str("  missing display cap: write rejected, console unchanged (ok)\n");
    } else {
        console_write_str("  missing display cap: FAILED to reject\n");
    }
    // re-grant so the rest of the demo keeps working
    let _ = kernel::submit(0, &alloc::vec![kernel::Op::new(kernel::OpKind::Grant { token: tok })]);

    // forged capability: an integer/bytes presented as a display cap is not
    // a capability (software unforgeability; PAC is a later hardware change).
    let forged = 0x1234_5678_9abc_def0u64;
    let forge_ops: alloc::vec::Vec<kernel::Op> = alloc::vec![kernel::Op::new(kernel::OpKind::DisplaySend {
        cap: forged,
        text: alloc::vec::Vec::from(&b"forged"[..]),
    })];
    let res3 = kernel::submit(0, &forge_ops);
    if matches!(res3[0], kernel::OpResult::CapError) {
        console_write_str("  forged integer as display cap: rejected (ok)\n");
    } else {
        console_write_str("  forged integer as display cap: FAILED to reject\n");
    }

    // operation-array batching + dependency ordering
    let tok = display_cap_token();
    let batch: alloc::vec::Vec<kernel::Op> = alloc::vec![
        kernel::Op::new(kernel::OpKind::Clock { cap: kernel::cap_of_kind(0, kernel::CAP_CLOCK) }),
        kernel::Op::new(kernel::OpKind::DisplaySend {
            cap: tok,
            text: alloc::vec::Vec::from(&b"batch: dependent display send"[..]),
        })
        .dep(0),
    ];
    let bres = kernel::submit(0, &batch);
    if matches!(bres[0], kernel::OpResult::Clock(_)) && matches!(bres[1], kernel::OpResult::Ok) {
        console_write_str("  op-array batch: 2 ops, dependent send after clock (ok)\n");
    } else {
        console_write_str("  op-array batch: FAILED\n");
    }
    let unmet: alloc::vec::Vec<kernel::Op> = alloc::vec![
        kernel::Op::new(kernel::OpKind::DisplaySend {
            cap: tok,
            text: alloc::vec::Vec::from(&b"unmet dependency"[..]),
        })
        .dep(5), // depends on a nonexistent prior op
    ];
    let ures = kernel::submit(0, &unmet);
    if matches!(ures[0], kernel::OpResult::DepError) {
        console_write_str("  op with unmet dependency: refused (ok)\n");
    } else {
        console_write_str("  op with unmet dependency: FAILED to refuse\n");
    }

    kernel::free_region(0, region);
    console_write_str("\n");
}

fn display_cap_token() -> u64 {
    kernel::cap_of_kind(0, uir::CAP_DISPLAY)
}

fn bench_fusion() {
    console_write_str("--- bench-fusion (2048x2048 f32, home=uma, engine=p-core) ---\n");
    let opts = RunOpts { realm: 0, live: None, policy: None, interactive: false };

    kernel::reset_counters();
    let mut gf = Graph::new(bench_fused_prog());
    let _ = stepper::run(&mut gf, &opts);
    let fused_bytes = kernel::counters().payload_moved;
    let fused_entries = kernel::counters().kernel_entries;
    gf.release_all();

    kernel::reset_counters();
    let mut gu = Graph::new(bench_unfused_prog());
    let _ = stepper::run(&mut gu, &opts);
    let unfused_bytes = kernel::counters().payload_moved;
    let unfused_entries = kernel::counters().kernel_entries;
    gu.release_all();

    let mut s = alloc::vec::Vec::new();
    fmt::append_str(&mut s, "  fused:   ");
    fmt::append_u64(&mut s, fused_bytes);
    fmt::append_str(&mut s, " bytes moved, ");
    fmt::append_u64(&mut s, fused_entries);
    fmt::append_str(&mut s, " kernel entries\n");
    fmt::append_str(&mut s, "  unfused: ");
    fmt::append_u64(&mut s, unfused_bytes);
    fmt::append_str(&mut s, " bytes moved, ");
    fmt::append_u64(&mut s, unfused_entries);
    fmt::append_str(&mut s, " kernel entries\n");
    fmt::append_str(&mut s, "  in-image byte-traffic proxy (no SLC hit rate is claimed on virt)\n");
    console_write_bytes(&s);
    if fused_bytes < unfused_bytes {
        console_write_str("  fused moves fewer bytes (ok)\n\n");
    } else {
        console_write_str("  fused did NOT move fewer bytes (FAIL)\n\n");
    }
}

fn bench_send() {
    console_write_str("--- bench-send (immutable same-home share vs explicit copy) ---\n");
    // allocate a large immutable region and share it by capability vs copy it
    let bytes = 32 * 1024 * 1024; // 32 MiB
    let region = match kernel::alloc_region(0, bytes) {
        Some(r) => r,
        None => {
            console_write_str("  alloc failed (quota)\n\n");
            return;
        }
    };
    let base = kernel::region_base(region).unwrap();
    unsafe {
        for i in 0..(bytes / 8) {
            *(base as *mut u64).add(i) = i as u64;
        }
    }
    // mark immutable (sharing requires immutability)
    kernel::mark_immutable(region);

    let token = kernel::mint_region_cap(region);

    // share (region-cap share, no payload copy)
    kernel::reset_counters();
    let share_ops = alloc::vec![kernel::Op::new(kernel::OpKind::Share { cap: token })];
    let share_res = kernel::submit(0, &share_ops);
    let share_copied = kernel::counters().payload_copied;

    // copy (explicit payload copy — the bench control)
    let copy_ops = alloc::vec![kernel::Op::new(kernel::OpKind::Copy { cap: token })];
    let copy_res = kernel::submit(0, &copy_ops);
    let copy_copied = kernel::counters().payload_copied;

    let mut s = alloc::vec::Vec::new();
    fmt::append_str(&mut s, "  share (region cap): ");
    fmt::append_u64(&mut s, share_copied);
    fmt::append_str(&mut s, " bytes copied, result=");
    fmt::append_str(&mut s, if matches!(share_res[0], kernel::OpResult::Ok) { "ok" } else { "cap-error" });
    fmt::append_str(&mut s, "\n");
    fmt::append_str(&mut s, "  copy  (memcpy)    : ");
    fmt::append_u64(&mut s, copy_copied);
    fmt::append_str(&mut s, " bytes copied, result=");
    fmt::append_str(&mut s, if matches!(copy_res[0], kernel::OpResult::Region(_)) { "ok" } else { "err" });
    fmt::append_str(&mut s, "\n");
    console_write_bytes(&s);
    if share_copied < copy_copied {
        console_write_str("  capability send copies fewer payload bytes (ok)\n");
    } else {
        console_write_str("  capability send did NOT copy fewer bytes (FAIL)\n");
    }

    // unique-region in-place mutation: a region with exactly one owner and not
    // marked immutable may be updated in place; a shared immutable region must
    // be copied instead.
    let ur = kernel::alloc_region(0, 16 * 1024).unwrap();
    let patch = [42u8; 8];
    let ip = alloc::vec![kernel::Op::new(kernel::OpKind::InPlace {
        region: ur,
        offset: 0,
        data: alloc::vec::Vec::from(&patch[..]),
    })];
    let ipres = kernel::submit(0, &ip);
    console_write_str("  unique region in-place update: ");
    console_write_str(if matches!(ipres[0], kernel::OpResult::Ok) { "allowed (ok)\n" } else { "refused (FAIL)\n" });

    kernel::mark_immutable(ur);
    let ip2 = alloc::vec![kernel::Op::new(kernel::OpKind::InPlace {
        region: ur,
        offset: 0,
        data: alloc::vec::Vec::from(&patch[..]),
    })];
    let ipres2 = kernel::submit(0, &ip2);
    console_write_str("  immutable (shared) region in-place update: ");
    console_write_str(if matches!(ipres2[0], kernel::OpResult::CapError) { "refused, copy required (ok)\n\n" } else { "incorrectly allowed (FAIL)\n\n" });
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
