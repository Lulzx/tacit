## 1. Engines

- [x] 1.1 Record `engine = neon` on pure elementwise nodes; run them as 128-bit Advanced-SIMD kernels with a per-engine entry counter
- [x] 1.2 Add `&matmul` placed on `engine = sme`; probe `ID_AA64PFR1_EL1.SME`, enable `CPACR_EL1.SMEN` (bits 25:24), and enter/leave streaming mode; fall back to p-core when SME is absent
- [x] 1.3 Machine description reports SME online/offline from the probe; `&stats` reports per-engine kernel entries

## 2. Capabilities

- [x] 2.1 Sign capability tokens with `pacga` under a kernel-only GA key when FEAT_PACGA is present; verify by recomputing the MAC over the stored nonce
- [x] 2.2 Keep software tokens as the fallback; the capability spec is unchanged
- [x] 2.3 Selftest rejects a forged integer and a single bit-flipped genuine token

## 3. Uiua shell and self-hosted compiler

- [x] 3.1 Extract the compiler into `crates/compile` (shared `no_std`), used by both host and guest
- [x] 3.2 Interactive shell: compile each typed line in-guest, step it, echo the value; bindings persist as session constants
- [x] 3.3 Self-hosted check: the guest re-compiles every bundled Uiua source and must produce byte-identical UIR to the host-compiled payload

## 4. Object store

- [x] 4.1 `&hash`/`&store`/`&load`: content id `H(data)` (deterministic FNV-1a), deduplicating stores, values only (64 KiB limit)

## 5. Deterministic replay

- [x] 5.1 Record every world read (`&keys`, `&clock`) with a sequence number; `&trace` projects it
- [x] 5.2 `&replay-keys`/`&replay-clock` consume recorded input in order, so a sequence re-run is deterministic

## 6. Multi-agent planner

- [x] 6.1 Three granted transforms over the live graph, ordered by a Uiua plan (`⊏ ⍖ ⊡ 1 &ready`); each agent still runs with the node-level policy
