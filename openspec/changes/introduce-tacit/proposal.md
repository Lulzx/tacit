## Why

Uiua thinks in arrays and tacit transformations. A Unix kernel whose source happens to be Uiua would waste that. Official Uiua already moved from bytecode to a tree-based compiler, and its hosted system APIs assume an OS underneath. The interesting design is an OS whose objects are **values, transformations, capabilities, and placement**, running on the machine we actually have.

**Thesis:** Linux virtualizes a computer for processes. Tacit virtualizes Apple Silicon as an array-transformation machine: one memory pool, many engines, a graph the system still understands.

`OS = values + transformations + capabilities + placement`, not `processes + threads + files + syscalls`.

**Research test (not Unix compat):** a nontrivial Uiua program expresses computation once; the OS discovers dependencies, shares immutable data zero-copy over unified memory, and places work on P-cores, E-cores, NEON, SME, and later GPU or ANE — without threads, locks, Metal/CUDA dispatch, or byte-stream IPC.

Name: **Tacit**. Language: Uiua. IR: UIR. Reference hardware: Apple M4 Pro. First boot: QEMU `aarch64` virt on that Mac, not x86_64 and not a native iBoot image.

## What Changes

- Define **Tacit** as one freestanding system: Uiua → **UIR** → native/stepped AArch64 → QEMU `aarch64` virt, with **no Linux or macOS in the guest**.
- Treat the development host as an Apple Silicon Mac. Document a single build command and a single run command that uses HVF when present.
- Keep a **tiny microkernel** (about 10–15 operations). No kernel filesystem, TCP, POSIX, `fork`, or `ioctl`.
- Cross the kernel as an **operation array**. The kernel may batch, reorder, or fuse ops the dependency graph allows.
- The executable object is a **Transform** `T : A → B` with shape, effects, dependencies, capabilities, **home**, **engine**, and cache domain — not a thread.
- Isolation is a **Realm** (heap, transforms, capability table, quota, failure boundary) with **zero ambient authority**.
- Split **values** from **authorities**. Shape-aware **regions** replace `void*`. On this machine a region's home is unified memory (`uma`). Placement is which **engine** runs the transform and which **cache domain** the working set should occupy.
- Name five native primitives and force everything else to emerge from them: **values, transformations, composition, capabilities, evaluation**.
- The machine's primary interface is the **live computation graph**. `ps`/`top`/`strace`/`lsof`/`/proc` are projections of that graph.
- Agent execution is observe → propose → simulate → validate → commit → observe. An agent is a transform, not a process running Python.
- Name the **speed stack**: specialized unikernel image, fuse named UIR, send capabilities, keep the kernel off the data path, keep working sets in the system-level cache, place work on the engine that already matches the op.

**First QEMU milestone (this change, first tasks):** boot `aarch64` under QEMU virt (HVF on the Mac), text/framebuffer, 16 KiB-aware allocator, keyboard, one Realm, a machine description that names M4 Pro engines, a tiny Uiua program whose graph is still inspectable. Show Add, Multiply, and the edge between them. Print a console projection of that graph. One granted agent-shaped transform may summarize it. Only the boot CPU (`p-core`) is wired.

**Later research (specified now, not the first boot image):** SME/NEON kernels, GPU and ANE as engines of the same node, SLC-accurate fusion tiles, object store, Uiua shell, compositor, self-hosted compiler, time-travel replay, multi-agent planner, native Apple Silicon bring-up. The first *speed* number is fusing `C=(A+B)×D` and measuring memory traffic; that is a later task in this change, after named UIR loads.

## Non-goals

- Success defined as “can it run Unix programs?” That would measure the wrong OS.
- Unix clone, POSIX table, `fork`/`exec`/`pthread`, files-as-the-worldview.
- Dragging the official hosted Uiua runtime into EL1.
- Kernel GC, kernel TCP, kernel filesystem, giant driver frameworks.
- **x86_64 as a required first target.** The first image is AArch64.
- **Native iBoot / Asahi-style metal boot in this change.** That is a later machine change.
- **Metal, CoreML, or CUDA as the guest programming model.** Engines are placements of UIR, not Apple or NVIDIA APIs.
- **macOS as the guest kernel**, or Tacit as a macOS process that calls Accelerate.
- Wiring SME, GPU, or ANE on the first QEMU image. The machine description MUST name them; the stepper MUST NOT require them.
- Self-hosting, networking, or a graphical shell in the first milestone.
- Agents that drive a conventional desktop by clicking.
- A separate “agent framework” bolted onto a Unix kernel.
- Claiming a win on pointer-chasing C or Chrome. UnixBench is not the scoreboard.
- Shipping a full multi-agent planner or GPU dispatch on the first QEMU image.

## Seven laws

1. No abstraction enters the core merely because Unix has it. Host-versus-device memory does not enter the core merely because discrete GPUs have it.
2. Pure computation is separated from authority.
3. Concurrency follows dependencies, not manually created threads.
4. Arrays retain shape as far down the system as possible.
5. Capabilities are the only route to effects.
6. Determinism is the default; nondeterminism is explicit data.
7. The hot path stays simple even when global policy is sophisticated.

## Capabilities

### New Capabilities

- `system`: What the OS believes computation is; the equation; the seven laws; the research test; Apple Silicon as the first machine; the ban on a Unix-shaped or discrete-GPU-shaped core.
- `boot`: Freestanding load under QEMU `aarch64` virt from reset to ready, with no host OS in the guest. HVF is the documented Mac path.
- `microkernel`: Trusted mechanism only: maps, caps, events, execution, interrupts; operation-array ABI; AArch64 stub.
- `machine`: First-milestone hardware facing plus a machine description of M4 Pro engines, 16 KiB pages, 128-byte lines. Console, keyboard, physical memory. No kernel GC.
- `regions`: Shape-aware memory over unified memory; arrays as region + offset + shape + strides; 16 KiB alignment; cache domain recorded.
- `capabilities`: Unforgeable authorities; zero ambient authority; values vs authorities; pointer authentication as the intended AArch64 mechanism (software unforgeability is enough on virt).
- `realms`: Isolation, quotas, failure boundaries; not POSIX processes.
- `array-runtime`: Host compiler to UIR; first-milestone Uiua subset; compiler and scheduler share one model; engine and home recorded.
- `array-kernel`: Transforms, dependency scheduling, two-level placer over engines, events-as-arrays, determinism.
- `combinators`: The five primitives. Unix objects and CUDA/Metal objects must be derived or refused.
- `live-graph`: The running machine is a queryable graph. Inspection tools are projections.
- `effects`: Every effect names authority and predicted world. Propose, simulate, validate, commit.
- `agents`: Agents are transformations with a small compositional algebra. Authority flows as data. Independent branches are placeable on engines.
- `fusion`: Adjacent pure UIR nodes become one kernel; first milestone is Add-then-Multiply; traffic is measured; tiles may name a cache domain.
- `zero-copy`: Default send is a capability to an immutable UMA region; copy is opt-in and counted.
- `datapath`: Kernel stays off the data path; events and ops are arrays; no per-byte syscall.
- `placement`: Transforms name an engine and a memory home. First milestone is `engine = p-core`, `home = uma`. GPU/ANE/SME are later engines of the same node.

### Modified Capabilities

- None. Greenfield repo. `openspec/specs/` is empty.

## Impact

- Empty codebase. Implementation adds an AArch64 boot image, a `no_std` microkernel, a host UIR compiler, Uiua sources for policy and the first Realm, graph query, effect preview, one agent-shaped transform, and later fusion / cap-send benches.
- Required run target: QEMU `aarch64` virt. On an Apple Silicon Mac the documented run command MUST use HVF. Loader: Limine or equivalent for AArch64 virt.
- Reference machine for later engine wiring and for the machine description: Apple M4 Pro (P-cluster, E-cluster, SME, GPU, ANE, media, display, unified LPDDR5X).
- Acceptance of this change is a reviewed OpenSpec plan, not running code.
