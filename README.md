# Tacit

Tacit is an operating system for [Uiua](https://www.uiua.org/).

Linux virtualizes a computer for processes. Tacit virtualizes Apple Silicon as an array-transformation machine.

```
OS = values + transformations + capabilities + placement
```

A program like `C = (A + B) × D` stays a graph after compile. The scheduler places **Add** and **Multiply** on an engine — P-core, E-core, SME, later GPU — not a thread. Shape survives into unified memory. Effects need a capability. There is no `fork`, no file descriptor, no POSIX table, no Metal command buffer.

The name is the programming style. You do not write threads. The graph is the program.

If Unix had never existed, the primitives would be values, transformations, composition, capabilities, and evaluation. `ps` and a debugger are projections of the same live graph. An agent is a transform over that graph, not a process that clicks or parses shell text. Every effect carries authority and provenance.

## Status

Boots freestanding under QEMU `aarch64` virt (HVF on an Apple Silicon Mac), no Linux or macOS in the guest. See `docs/run.md` for the full first-milestone demo.

## Build and run

```sh
./build.sh   # host compiler (Uiua -> UIR), then the freestanding AArch64 image
./qemu.sh    # boot build/tacit.elf under QEMU aarch64 virt (HVF)
```

The guest prints a ready banner that names Tacit, publishes the M4 Pro machine
description, runs `C = (A + B) × D` while still showing Add, Multiply, and the
edge between them, and reports its provenance. One granted agent-shaped
transform summarizes the live graph. Pure elementwise work is placed on the
**NEON engine** (a real 128-bit SIMD kernel; `&stats` reports per-engine
entries). Fusion (`fused bytes < unfused`) and zero-copy send benches print
in-image counters.

## Laws

1. No abstraction enters the core because Unix has it. Host-versus-device memory does not enter the core because discrete GPUs have it.
2. Pure computation is separated from authority.
3. Concurrency follows dependencies, not threads.
4. Arrays keep shape as far down as possible.
5. Capabilities are the only route to effects.
6. Determinism is the default. Nondeterminism is explicit data.
7. The hot path stays simple.

## Reading

- [proposal](openspec/changes/introduce-tacit/proposal.md) — why, what, the seven laws
- [design](openspec/changes/introduce-tacit/design.md) — decisions
- [research](openspec/changes/introduce-tacit/research.md) — why this can beat Linux on array work
- [five primitives and the Unix/Metal noun map](docs/primitives.md)
- [first-milestone Uiua subset](docs/subset.md)
- [build and run](docs/run.md)

Language is Uiua. IR is UIR. Machine layer is a tiny freestanding AArch64 runtime. Policy is Uiua. Reference hardware is Apple M4 Pro.
