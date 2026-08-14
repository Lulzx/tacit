## Why

Uiua thinks in arrays and tacit transformations. A Unix kernel whose source happens to be Uiua would waste that. Official Uiua already moved from bytecode to a tree-based compiler, and its hosted system APIs assume an OS underneath. The interesting design is an OS whose objects are **values, transformations, capabilities, and placement**.

**Thesis:** Linux virtualizes a computer for processes. Tacit virtualizes the computer as an array-transformation machine. Everything is a composable transformation; every effect carries authority and provenance; speed comes from keeping the graph.

`OS = values + transformations + capabilities + placement`, not `processes + threads + files + syscalls`.

**Research test (not Unix compat):** a nontrivial Uiua program expresses computation once; the OS discovers dependencies, shares immutable data zero-copy, and places work across cores / SIMD / GPU without threads, locks, explicit GPU dispatch, or byte-stream IPC.

Name: **Tacit**. Language: Uiua. IR: UIR.

## What Changes

- Define **Tacit** as one freestanding system: Uiua → **UIR** → native/stepped code → QEMU, with **no Linux or macOS in the guest**.
- Keep a **tiny microkernel** (about 10–15 operations). No kernel filesystem, TCP, POSIX, `fork`, or `ioctl`.
- Cross the kernel as an **operation array**. The kernel may batch, reorder, or fuse ops the dependency graph allows.
- The executable object is a **Transform** `T : A → B` with shape, effects, dependencies, capabilities, and placement — not a thread.
- Isolation is a **Realm** (heap, transforms, capability table, quota, failure boundary) with **zero ambient authority**.
- Split **values** from **authorities**. Shape-aware **regions** replace `void*`.
- Name five native primitives and force everything else to emerge from them: **values, transformations, composition, capabilities, evaluation**.
- The machine's primary interface is the **live computation graph**. `ps`/`top`/`strace`/`lsof`/`/proc` are projections of that graph.
- Agent execution is observe → propose → simulate → validate → commit → observe. An agent is a transform, not a process running Python.
- Name the **speed stack**: specialized unikernel image, fuse named UIR, send capabilities, keep the kernel off the data path, place work where the array already lives.

**First QEMU milestone (this change, first tasks):** boot, text/framebuffer, allocator, keyboard, one Realm, a tiny Uiua program whose graph is still inspectable. Show Add, Multiply, and the edge between them. Print a console projection of that graph. One granted agent-shaped transform may summarize it.

**Later research (specified now, not the first boot image):** object store, Uiua shell, compositor, SIMD/GPU placement, self-hosted compiler, time-travel replay, multi-agent planner. The first *speed* number is fusing `C=(A+B)×D` and measuring DRAM traffic; that is a later task in this change, after named UIR loads.

## Non-goals

- Success defined as “can it run Unix programs?” That would measure the wrong OS.
- Unix clone, POSIX table, `fork`/`exec`/`pthread`, files-as-the-worldview.
- Dragging the official hosted Uiua runtime into ring 0.
- Kernel GC, kernel TCP, kernel filesystem, giant driver frameworks.
- Self-hosting, GPU dispatch, networking, or a graphical shell in the first milestone.
- Running the OS inside the hosted interpreter.
- Agents that drive a conventional desktop by clicking.
- A separate “agent framework” bolted onto a Unix kernel.
- Claiming a win on pointer-chasing C or Chrome. UnixBench is not the scoreboard.
- Shipping a full multi-agent planner or GPU dispatch on the first QEMU image.

## Seven laws

1. No abstraction enters the core merely because Unix has it.
2. Pure computation is separated from authority.
3. Concurrency follows dependencies, not manually created threads.
4. Arrays retain shape as far down the system as possible.
5. Capabilities are the only route to effects.
6. Determinism is the default; nondeterminism is explicit data.
7. The hot path stays simple even when global policy is sophisticated.

## Capabilities

### New Capabilities

- `system`: What the OS believes computation is; the equation; the seven laws; the research test; the ban on a Unix-shaped core.
- `boot`: Freestanding load under QEMU from reset to ready, with no host OS.
- `microkernel`: Trusted mechanism only: maps, caps, events, execution, interrupts; operation-array ABI.
- `machine`: First-milestone hardware: console, keyboard, physical memory, no kernel GC.
- `regions`: Shape-aware memory with placement; arrays as region + offset + shape + strides.
- `capabilities`: Unforgeable authorities; zero ambient authority; values vs authorities.
- `realms`: Isolation, quotas, failure boundaries; not POSIX processes.
- `array-runtime`: Host compiler to UIR; first-milestone Uiua subset; compiler and scheduler share one model.
- `array-kernel`: Transforms, dependency scheduling, two-level placer, events-as-arrays, determinism.
- `combinators`: The five primitives. Unix objects must be derived or refused.
- `live-graph`: The running machine is a queryable graph. Inspection tools are projections.
- `effects`: Every effect names authority and predicted world. Propose, simulate, validate, commit.
- `agents`: Agents are transformations with a small compositional algebra. Authority flows as data.
- `fusion`: Adjacent pure UIR nodes become one kernel; first milestone is Add-then-Multiply; traffic is measured.
- `zero-copy`: Default send is a capability to an immutable region; copy is opt-in and counted.
- `datapath`: Kernel stays off the data path; events and ops are arrays; no per-byte syscall.
- `placement`: Transforms name a place; host is required; GPU/device is a later place of the same node.

### Modified Capabilities

- None. Greenfield repo. `openspec/specs/` is empty.

## Impact

- Empty codebase. Implementation adds a boot image, a `no_std` microkernel, a host UIR compiler, Uiua sources for policy and the first Realm, graph query, effect preview, one agent-shaped transform, and later fusion / cap-send benches.
- Required run target: QEMU x86_64. Loader: Limine or equivalent.
- Acceptance of this change is a reviewed OpenSpec plan, not running code.
