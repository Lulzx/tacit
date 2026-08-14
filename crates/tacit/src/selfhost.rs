//! Self-hosted compiler verification.
//!
//! The guest runs the *same* `crates/compile` source the host uses.  For
//! every bundled Uiua program (the `--no-fuse` ones, whose UIR is the
//! compiler's direct output), the guest re-compiles the embedded source and
//! must produce byte-identical UIR to the host-compiled payload it already
//! runs.  This is the self-hosted compiler story: same sources, new host.

struct Pair {
    name: &'static str,
    src: &'static str,
    uir: &'static [u8],
}

const PAIRS: &[Pair] = &[
    Pair { name: "tiny.ua", src: include_str!("../../../uiua/tiny.ua"), uir: include_bytes!("../embedded/tiny.uir") },
    Pair { name: "agent.ua", src: include_str!("../../../uiua/agent.ua"), uir: include_bytes!("../embedded/agent.uir") },
    Pair { name: "agent-sort.ua", src: include_str!("../../../uiua/agent-sort.ua"), uir: include_bytes!("../embedded/agent-sort.uir") },
    Pair { name: "agent-pick.ua", src: include_str!("../../../uiua/agent-pick.ua"), uir: include_bytes!("../embedded/agent-pick.uir") },
    Pair { name: "plan.ua", src: include_str!("../../../uiua/plan.ua"), uir: include_bytes!("../embedded/plan.uir") },
    Pair { name: "subset.ua", src: include_str!("../../../uiua/subset.ua"), uir: include_bytes!("../embedded/subset.uir") },
    Pair { name: "machine.ua", src: include_str!("../../../uiua/machine.ua"), uir: include_bytes!("../embedded/machine.uir") },
    Pair { name: "graph.ua", src: include_str!("../../../uiua/graph.ua"), uir: include_bytes!("../embedded/graph.uir") },
    Pair { name: "provenance.ua", src: include_str!("../../../uiua/provenance.ua"), uir: include_bytes!("../embedded/provenance.uir") },
    Pair { name: "objects.ua", src: include_str!("../../../uiua/objects.ua"), uir: include_bytes!("../embedded/objects.uir") },
    Pair { name: "replay.ua", src: include_str!("../../../uiua/replay.ua"), uir: include_bytes!("../embedded/replay.uir") },
    Pair { name: "bench-send.ua", src: include_str!("../../../uiua/bench-send.ua"), uir: include_bytes!("../embedded/bench-send.uir") },
    Pair { name: "bench-matmul.ua", src: include_str!("../../../uiua/bench-matmul.ua"), uir: include_bytes!("../embedded/bench-matmul.uir") },
    Pair { name: "scheduler.ua", src: include_str!("../../../uiua/scheduler.ua"), uir: include_bytes!("../embedded/scheduler.uir") },
    Pair { name: "authorize.ua", src: include_str!("../../../uiua/authorize.ua"), uir: include_bytes!("../embedded/authorize.uir") },
];

pub fn verify() {
    crate::console_write_str("self-hosted compiler (same UIR sources, in-guest):\n");
    let mut all_ok = true;
    for p in PAIRS {
        match compile::compile_file(p.src) {
            Ok(bytes) => {
                if bytes.as_slice() == p.uir {
                    let mut out = alloc::vec::Vec::new();
                    crate::fmt::append_str(&mut out, "  ");
                    crate::fmt::append_str(&mut out, p.name);
                    crate::fmt::append_str(&mut out, ": byte-identical (ok)\n");
                    crate::console_write_bytes(&out);
                } else {
                    crate::console_write_str("  ");
                    crate::console_write_str(p.name);
                    crate::console_write_str(": MISMATCH (FAILED)\n");
                    all_ok = false;
                }
            }
            Err(e) => {
                crate::console_write_str("  ");
                crate::console_write_str(p.name);
                crate::console_write_str(": compile error (FAILED): ");
                crate::console_write_str(&e);
                crate::console_write_str("\n");
                all_ok = false;
            }
        }
    }
    crate::console_write_str(if all_ok {
        "  all sources byte-identical to the host-compiled payloads (ok)\n"
    } else {
        "  self-hosted verification FAILED\n"
    });
}
