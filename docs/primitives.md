# The five primitives

Tacit's core vocabulary is exactly five things. Nothing else is fundamental;
Unix objects and Metal/CUDA objects are derived from these five or refused.

1. **Values** — numbers, characters, arrays, boxes, functions. Computed
   freely. A value carries shape and type; an array is a region view
   (`region + offset + shape + strides`), never a bare pointer + length.
2. **Transformations** — `T : A → B` with shape, effects, dependencies,
   capabilities, home, engine, and cache domain. The runnable object is a
   transform, not a thread.
3. **Composition** — data dependence is the schedule. `Ready(T)` iff every
   input exists. Fan-out is visible; there is no thread-create.
4. **Capabilities** — unforgeable authorities (region, device, channel, clock,
   realm, execution, engine). Minted only by the kernel; arithmetic cannot
   mint one. The only route to effects.
5. **Evaluation** — stepping a ready transform on an engine over `home = uma`.

## One-page map: Unix and Metal/CUDA nouns → derived or refused

| Unix / macOS noun | Tacit | Status |
| --- | --- | --- |
| process, `fork`, `exec` | a Realm (heap + transforms + cap table + quota) | derived |
| thread, `pthread`, `clone` | independent transforms; readiness is the sync | derived |
| file, fd, `open/read/seek/write/close` | a value/region + a capability | derived (later) |
| pipe, socket, byte-stream IPC | an edge carrying a value or a region capability | derived |
| `ps`, `top`, `strace`, `lsof`, `/proc` | projections of the live graph | derived |
| `chmod`, UID/GID ambient authority | grant / revoke / narrow; born with `{}` | derived |
| syscall table | the operation-array ABI (ops in, results out) | derived |
| scheduler (CFS) | a Uiua transform over the ready array | derived (policy) |
| in-kernel TCP, `listen`/`accept` | a libOS transform over descriptor rings (later) | refused here |
| POSIX `ioctl` | device as an array, mapped via `map-device` | refused here |
| `void*`, addr + length | shape-aware region (`type, shape, strides, home, cache`) | refused |
| Metal command buffer, `MTLDevice` | `engine = gpu` of the same transform | refused as core |
| CUDA stream / `cudaMemcpy` | `engine = gpu`; same-home `uma` send is a region cap | refused as core |
| CoreML model handle | `engine = ane`; a learned transform | refused as core |
| Accelerate / AMX | `engine = sme` (SME, not SVE) | refused as core |
| host RAM vs VRAM | one `uma` pool; placement is engine + cache domain | refused as core |
