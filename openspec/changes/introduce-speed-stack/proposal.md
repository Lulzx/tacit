## Why

`introduce-uiua-os` defines the machine. It defers speed. Linux is not slow because it is C. It is slow on array work because it deleted the graph and then paid for generality: syscalls, KPTI, copies, a universal scheduler, bounce buffers to the GPU.

The speed stack is how we are faster **by construction**: keep the graph, delete those layers, measure the first number that Linux cannot match.

**Thesis:** specialize the image, fuse named UIR, send capabilities, keep the kernel off the data path, place work where the array already lives.

## What Changes

- Name and sequence the **speed stack** as the only performance program.
- **Layer 1 — Unikernel image:** one specialized payload. Already required by `introduce-uiua-os`. This change forbids growing a general kernel “for later.”
- **Layer 2 — Fusion (first number):** fuse `C = (A+B)×D` (and adjacent pure elementwise/reduce chains) into one kernel. Report DRAM traffic or equivalent memory-op counts vs the unfused graph.
- **Layer 3 — Zero-copy send:** immutable send is a region-capability handoff, O(1) metadata. Copy is the fallback, measured, not the default.
- **Layer 4 — Datapath:** events and I/O stay in batches. No per-element syscall. Kernel does not implement TCP or a file path.
- **Layer 5 — Placement:** host tiles first; device/GPU is a placement of the same transform, not a CUDA-shaped API. Bounce copies are explicit and counted.
- Scoreboard is the research test, not UnixBench.

## Non-goals

- Applying `introduce-uiua-os` (boot + tiny program) in this change.
- POSIX, Linux ABI, a general FS/net/CFS.
- Claiming a win on pointer-chasing C or Chrome.
- Shipping GPU dispatch in the first speed milestone (fusion on host/QEMU is enough for the first number).
- Embedding VoltDB, DPDK, or the official Uiua runtime.

## Capabilities

### New Capabilities

- `fusion`: Adjacent pure UIR nodes become one kernel; first milestone is Add-then-Multiply; traffic is measured.
- `zero-copy`: Default send is a capability to an immutable region; copy is opt-in and counted.
- `datapath`: Kernel stays off the data path; events and ops are arrays; no per-byte syscall.
- `placement`: Transforms name a place; host is required; GPU/device is a later place of the same node, not another universe.

### Modified Capabilities

- None in `openspec/specs/` yet. `introduce-uiua-os` is still an active change. This stack adds speed capabilities on top; it does not rewrite those deltas in place.

## Impact

- Planning only until apply. Implementation depends on a bootable UIR stepper from `introduce-uiua-os`.
- Adds a documented bench: fused vs unfused `C=(A+B)×D`, plus a copy-vs-cap-send bench.
- No Linux ABI. No new host OS in the guest.
