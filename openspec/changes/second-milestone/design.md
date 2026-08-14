## Context

First milestone is archived (`openspec/changes/archive/introduce-tacit`); its specs are synced to `openspec/specs/`.  This change is the "later research" list from the first proposal, reduced to what QEMU virt can honestly demonstrate.

## Decisions

### Decision: engines are probe-gated, never assumed

QEMU keeps ID registers consistent with what it emulates.  `ID_AA64PFR1_EL1.SME` gates the SME engine; `ID_AA64ISAR1_EL1.GPI` gates PACGA.  When the probe fails, the engine stays offline or the kernel falls back (matmul → p-core; tokens → software).  The machine description and the counters always report what actually ran.

### Decision: NEON first, SME as streaming mode

Elementwise f32 work is a 128-bit Advanced-SIMD kernel.  `&matmul` is placed on `engine = sme`; the shipped slice enters and leaves streaming mode (`smstart`/`smstop`) and computes the product with the ordinary kernel, counted as an `sme` entry.  The ZA-accumulating tile kernel is the documented next slice (LLVM's assembler here does not express ZA tile mnemonics, so raw encodings would be needed).

### Decision: PAC tokens are a MAC, verified by recomputation

`mint` hands out `pacga(nonce, modifier)` under a kernel-only GA key.  `lookup` verifies a presented token by recomputing the MAC over the stored nonce (equivalent to `autga`; this LLVM does not assemble the reverse instruction).  Forging requires the GA key, so arithmetic cannot mint a capability.  On hardware with real EL separation the GA key is kernel-only; on this single-EL guest the mechanism is still demonstrated and the software fallback remains.

### Decision: one compiler source, host and guest

The compiler is `crates/compile`, a shared `no_std` crate used by `hostc` (a thin std driver) and by the in-guest shell and self-hosted check.  Byte-identity of UIR is a property of the shared encoder, not a coincidence.

### Decision: bindings in the shell are values

A bound name is snapshotted as a session constant and re-emitted by later lines as a fresh constant node — line programs stay self-contained and the shell is deterministic.

### Decision: the object store holds values, not bulk payloads

Content addressing is for small values (`id = H(data)`, deduplicated).  Bulk payloads belong to the datapath (zero-copy regions), not to a store that would copy them.

### Decision: replay is effect-input recording, not a second OS

The runtime records each world read with a sequence number; the replay functions consume the recorded input.  This is the data a later time-travel replay would feed back.

## Microkernel / policy line

All of the above runs above the microkernel line: the kernel adds mechanism (PAC signing, trace recording, the object store, per-engine counters, `smstart` enable), and everything that decides behavior is Uiua (the planner, the shell lines, the benches).  No scheduler or policy code moved into assembly.
