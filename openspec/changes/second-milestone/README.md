# Second milestone: engines, PAC, shell, store, replay, planner

Ships the proposal's *later research* items that QEMU virt can honestly
demonstrate:

- **Engines**: NEON elementwise kernels; SME `&matmul` (streaming mode,
  probe-gated); per-engine kernel-entry counters.
- **Capabilities**: PACGA-signed tokens with software fallback.
- **Uiua shell** and **self-hosted compiler** (byte-identical UIR in-guest).
- **Content-addressed object store** (`id = H(data)`).
- **Deterministic replay** of recorded effect inputs.
- **Multi-agent planner** (a Uiua plan orders the agents).

Status: complete and verified under QEMU `aarch64` virt (HVF and TCG).
GPU, ANE, real SLC counters, and native Apple Silicon boot remain out of
scope (they need metal).  The SME ZA tile kernel is the documented next
slice.
