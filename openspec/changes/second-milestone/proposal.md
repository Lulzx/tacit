## Why

The first milestone (boot, machine description, one Realm, tiny program, graph, fusion, zero-copy) is archived and demonstrable under QEMU aarch64 virt.  The proposal's *later research* list promised: SME/NEON kernels as engines of the same node, the Uiua shell and a self-hosted compiler, a content-addressed object store, deterministic replay, and a multi-agent planner.  This change ships the slices of that list that are implementable and verifiable on QEMU virt, with the same rule as the first milestone: anything that needs real metal stays named-but-offline.

**Thesis (unchanged):** Linux virtualizes a computer for processes; Tacit virtualizes Apple Silicon as an array-transformation machine.  This change makes placement and authority *observably real* in the guest: elementwise work runs on the NEON engine, matrix work engages the SME engine, and capability tokens are PAC-signed.

## What Changes

- **Engines become real placements.**  Pure elementwise nodes are `engine = neon` and execute as 128-bit Advanced-SIMD kernels; `&matmul` is `engine = sme` and enters/leaves streaming mode (probe-gated on `ID_AA64PFR1_EL1.SME`).  Per-engine kernel-entry counters make the engine observable.
- **Capability tokens are PAC-signed.**  When FEAT_PACGA is present, a token is the `pacga` MAC of a kernel nonce under a kernel-only GA key; the kernel verifies by recomputing the MAC.  A forged integer and a single flipped bit are rejected.  Software tokens remain the fallback, so the capability spec does not change.
- **Uiua shell.**  The guest drops into an interactive REPL after the benches: each typed line is compiled to UIR in-guest by the *same* `crates/compile` source the host uses, and stepped on the boot CPU.  Bindings are values that persist as session constants.
- **Self-hosted compiler check.**  At boot the guest re-compiles every bundled Uiua source (embedded as text) and must produce byte-identical UIR to the host-compiled payload it runs.  Same sources, new host.
- **Content-addressed object store.**  `&store` registers a value under `id = H(data)` (deterministic FNV-1a), deduplicating; `&load` returns it.  The store holds values, not bulk payloads (64 KiB limit).
- **Deterministic replay.**  Every world read (`&keys`, `&clock`) is recorded with a sequence number; `&replay-keys`/`&replay-clock` consume the recorded input instead of the live device, so a sequence re-run is deterministic.
- **Multi-agent planner.**  Three granted transforms over the live graph are ordered by a Uiua plan program (`⊏ ⍖ ⊡ 1 &ready` — select on grade-down of the picked priority column).

## Non-goals

- Wiring GPU, ANE, or real SLC counters (require metal).
- Native Apple Silicon boot (iBoot/AIC/DART/DCP).
- The full SME ZA-accumulating tile kernel (the streaming-mode slice is shipped; ZA tiles are a documented next slice).
- Any Unix abstraction or vendor compute API in the core (law 1).

## Acceptance

Each item above is demonstrable under QEMU `aarch64` virt (HVF on the Mac, TCG fallback): the boot log shows the engine counters, the PACGA-signed-token line, the self-hosted byte-identical report, and the benches; the shell compiles typed Uiua.
