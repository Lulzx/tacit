## Why

Uiua thinks in arrays and tacit transformations. A Unix kernel whose source happens to be Uiua would waste that. Official Uiua already moved from bytecode to a tree-based compiler, and its hosted system APIs assume an OS underneath. The interesting design is an OS whose objects are **values, transformations, capabilities, and placement**.

**Thesis:** Linux virtualizes a computer for processes. Stride virtualizes the computer as an array-transformation machine. The difference is what the OS believes computation *is*, not which language the kernel is written in.

`OS = values + transformations + capabilities + placement`, not `processes + threads + files + syscalls`.

**Research test (not Unix compat):** a nontrivial Uiua program expresses computation once; the OS discovers dependencies, shares immutable data zero-copy, and places work across cores / SIMD / GPU without threads, locks, explicit GPU dispatch, or byte-stream IPC.

Name: **Stride**. Language: Uiua. IR: UIR.

## What Changes

- Define **Stride** as a freestanding system: Uiua → **UIR** (project IR) → native/stepped code → QEMU, with **no Linux or macOS in the guest**.
- Keep a **tiny microkernel** (about 10–15 operations). No kernel filesystem, TCP, POSIX, `fork`, or `ioctl`.
- Cross the kernel as an **operation array**, not C-shaped syscalls. The kernel may batch, reorder, or fuse ops the dependency graph allows.
- The executable object is a **Transform** `T : A → B` with shape, effects, dependencies, capabilities, and placement — not a thread.
- Isolation is a **Realm** (heap, transforms, capability table, quota, failure boundary) with **zero ambient authority**.
- Split **values** (freely transformed) from **authorities** (unforgeable capabilities). Shape-aware **regions** replace `void*`.
- First milestone: boot, text/framebuffer, allocator, keyboard, one Realm, then run a tiny Uiua program on bare metal.
- Later, not this milestone: object store, Uiua shell, compositor, SIMD/GPU placement, self-hosted compiler, time-travel replay.
- Speed-by-design (see `research.md`): **unikernel image** + **data-centric state** + **tiny mechanism** (LionsOS lesson) + later bypass/fusion/placement. Refuse Linux compatibility layers. Success is not UnixBench. The first *speed* milestone is fusing `C=(A+B)×D` — a later change.

## Non-goals

- Success defined as “can it run Unix programs?” That would measure the wrong OS.
- Unix clone, POSIX table, `fork`/`exec`/`pthread`, files-as-the-worldview.
- Dragging the official hosted Uiua runtime into ring 0.
- Kernel GC, kernel TCP, kernel filesystem, giant driver frameworks.
- Self-hosting, GPU dispatch, networking, or a graphical shell in the first milestone.
- Running the OS inside the hosted interpreter.

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

### Modified Capabilities

- None. Greenfield repo.

## Impact

- Empty codebase. Implementation adds a boot image, a `no_std` microkernel, a host UIR compiler, and Uiua sources for policy and the first Realm.
- Required run target: QEMU x86_64. Loader: Limine or equivalent.
- Acceptance of this change is a reviewed OpenSpec plan, not running code.
