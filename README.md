# Tacit

Tacit is an operating system for [Uiua](https://www.uiua.org/).

Linux virtualizes a computer for processes. Tacit virtualizes it as an array-transformation machine.

```
OS = values + transformations + capabilities + placement
```

A program like `C = (A + B) × D` stays a graph after compile. The scheduler places **Add** and **Multiply**, not threads. Shape survives into memory. Effects need a capability. There is no `fork`, no file descriptor, no POSIX table.

The name is the programming style. You do not write threads. The graph is the program.

If Unix had never existed, the primitives would be values, transformations, composition, capabilities, and evaluation. `ps` and a debugger are projections of the same live graph. An agent is a transform over that graph, not a process that clicks or parses shell text. Every effect carries authority and provenance.

## Status

Specs only. Nothing boots yet.

One change: [`introduce-tacit`](openspec/changes/introduce-tacit/). Kernel model, QEMU boot, live graph, effects before commit, agents as transforms, then fusion and capability send.

Read the [proposal](openspec/changes/introduce-tacit/proposal.md), then the [research note](openspec/changes/introduce-tacit/research.md) if you care why this can be faster than Linux on array work.

## Laws

1. No abstraction enters the core because Unix has it.
2. Pure computation is separated from authority.
3. Concurrency follows dependencies, not threads.
4. Arrays keep shape as far down as possible.
5. Capabilities are the only route to effects.
6. Determinism is the default. Nondeterminism is explicit data.
7. The hot path stays simple.

## First demo

Boot under QEMU with no Linux or macOS in the guest. Print a ready banner that says Tacit. Run `C = (A + B) × D` and still show Add, Multiply, and the edge between them.

Language is Uiua. IR is UIR. Machine layer is a tiny freestanding runtime. Policy is Uiua.
