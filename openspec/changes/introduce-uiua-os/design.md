## Context

Greenfield. See `proposal.md` for why and the nine delta specs for the contract.

The difference is not “Linux is C, this is Uiua.” Linux believes computation is **opaque programs on a virtual computer**: process, address space, threads, file descriptors, syscalls. By the time the kernel runs `C = (A + B) × D`, it sees loads, stores, and a thread. The fact that Add is elementwise, that millions of elements are independent, that Multiply consumes Add, that the work could live on a GPU — that meaning is already gone.

Tacit believes computation is a **graph the system still understands**:

```text
A ──┐
    ├─ Add ──┐
B ──┘        ├─ Multiply ──> C
D ───────────┘
```

Compiler, runtime, and scheduler share that graph. Linux's opacity is why it can run COBOL and Chrome. Tacit gives that up on purpose: it participates in the computation.

C maps onto hardware; Uiua maps onto arrays. Official Uiua 0.14 is tree-based, not bytecode, and its hosted runtime assumes an OS. Do not link that runtime into ring 0.

## Goals / Non-Goals

**Goals:**

- Name the system **Tacit** and make `values + transformations + capabilities + placement` the actual architecture.
- Keep semantic UIR long enough that the scheduler places **transforms**, not threads.
- First QEMU milestone: a **unikernel-style** image — console, allocator, keyboard, one Realm, one tiny program whose graph is still inspectable. No general FS, TCP, or process table.
- Leave a straight path to the research test (zero-copy + multi-core/SIMD/GPU placement from one program) without building GPU or Unix compat now.

**Non-Goals:**

- “Can it run Unix programs?” as a success metric.
- Unix, POSIX, files-as-worldview, TempleOS cosplay, Linux-in-glyphs.
- Embedding the official hosted interpreter.
- Kernel GC, kernel TCP, kernel filesystem.
- GPU, NIC, NVMe, compositor, or time-travel debugger in this change's tasks.

## Decisions

### Decision: Two universes

| Values | Authorities |
| --- | --- |
| numbers, characters, arrays, boxes, functions | region, device, channel, clock, realm, execution |

Values are computed. Authorities are minted only by the kernel. This is law 2. Treating capabilities as just more arrays would let arithmetic forge a disk.

### Decision: Microkernel is mechanism only

About 10–15 ops: `alloc` / `map` / `unmap`, `spawn` / `stop` / `yield`, `send` / `recv`, `grant` / `revoke`, `wait` / `signal`, `map-device`, `clock`.

No files, no TCP, no `fork`, no `ioctl`. Services (store, net, compositor, shell) are Realms.

The ABI is an **operation array** in and a **result array** out. Crossing into the kernel is itself an array transform. Independent ops may be reordered or fused.

### Decision: What Linux asks vs what this OS asks

| Linux | Tacit |
| --- | --- |
| Which thread runs on this CPU? | Which ready transform runs, on what hardware? |
| Programmer or runtime spawns workers | Elementwise/`rows` already *is* the parallelism |
| Kernel sees RIP, loads, stores, syscalls | Kernel/runtime see Map, Reduce, Scan, MatMul, edges |
| Threads + locks | Ready(T) iff all inputs exist |
| Memory is addr + length + perms | Memory is type, shape, strides, placement |
| CPU vs GPU are different universes | GPU is a placement of the same transform |
| UID/GID + ambient home/net | Born with `{}`; only granted caps |
| Everything is a file / fd | Everything is a value or a transform over values |
| IPC often serialize → bytes → deserialize | Default send is a region capability, O(1) metadata |
| Interleavings are implicit | Independent order must not change meaning |
| Debugger: registers and frames | Debugger: which transform produced this array |
| Mutable named files | Later: content-addressed values |

Linux says: I manage hardware for arbitrary opaque programs.  
Tacit says: I understand the structure of computation well enough to help execute it.

That is a tradeoff, not a slogan. Generality is the cost.

### Decision: Transform, not thread

```text
T : A → B
  inputs, outputs, shape, effects
  dependencies, capabilities
  placement, priority
```

Runnable iff unresolved dependencies are empty and caps are present. CPU threads are how a stepper happens to run, not what a program authors.

### Decision: UIR, not official bytecode and not LLVM

```text
Uiua subset → semantic tree → UIR
                 ├─ scalar
                 ├─ SIMD          (later)
                 └─ graph
                      ↓
                 stepper / native
```

UIR records shape, purity, edges, regions, caps, parallel axes, placement. Compiler and scheduler share that record. Official Uiua's tree compiler is a hint, not a crate we link into the guest.

First-milestone subset: numeric rank 0–2, character vectors, arithmetic, reduce, reshape, rank-wise map, display write, keyboard read. Everything else is a compile error.

Rename from the earlier working name AIR: same idea, user's name is **UIR**.

### Decision: Realms, not processes

A Realm is heap/regions + transforms + cap table + quota + failure boundary. It is born with nothing. Boot grants the initial Realm display, keyboard, and a memory budget.

### Decision: Regions keep shape

```text
Array = { region, offset, shape, strides }
Region = { type, shape, layout, place }
place ∈ { host, device, shared, persistent, mapped-io }
```

First milestone implements `host` only. Slice/reshape/transpose are metadata when they can be. Large arrays are not refcount-copied on every view.

Kernel memory: arenas/slabs, explicit lifetimes. Realm memory: refcount + unique-region mutation + a later cycle story. No tracing GC in the kernel.

### Decision: Events are batches; devices are arrays

```text
IRQ → {time, source, payload} → EventArray
      → select / group / partition / reduce
      → ready transforms
```

ISRs only acknowledge hardware. NIC/NVMe/GPU drivers, when they exist, are transforms over descriptor rings and completion arrays — those devices are already arrays. Not this milestone.

### Decision: Two-level scheduler

- **Micro:** take next ready node, run it, record completion. This is the hot path (law 7).
- **Global placer:** periodically scores transforms by latency, movement, queue, energy. First milestone is one boot CPU, so the placer is a Uiua policy that only orders the ready array.

Later:

```text
Score(t,d) = αL + βM + γQ + δE
d* = argmin_d (compute(t,d) + move(inputs(t),d))
```

### Decision: Layers, then the real diagram

Vertical stack (services are later Realms):

```text
applications
services          (UI, store, net — later)
array runtime     (shape, regions, graphs, effects, placement)
realms
global placer
microkernel
arch / hardware
```

The structure that actually matters:

```text
VALUES → TRANSFORM GRAPH
            ├─ pure work
            ├─ effects
            └─ authorities
                 ↓
               placer
            ├─ CPU
            ├─ SIMD / GPU     (later)
            └─ device
```

### Decision: Boot

```text
UEFI or Limine → arch stub (page tables, stack, framebuffer, machine desc)
              → UIR runtime → initial Realm → tiny program
              → (later) device manager Realm → Uiua shell
```

Assembly/Rust is allowed only for: long mode, interrupt entry, atomics, special registers. Push that line down over time. Do not write the scheduler there.

### Decision: Steal deletions, not Linux features

Five construction styles beat Linux by *removing* layers. We use all five that fit; we do not become them.

| Style | Evidence | What we actually do |
| --- | --- | --- |
| Unikernel / libOS | Unikraft: 1.7–2.7× vs Linux VMs; 10–60% vs native; ~1 MB images | v1 image is one specialized payload. No unused subsystem is linked. |
| Kernel off datapath | Demikernel, Junction 1.6–7× | Op-array ABI + event arrays. Future NIC is a libOS, not in-kernel TCP. |
| Tiny modular microkernel | LionsOS 2025: saturates 1 Gb/s where Linux cannot; ~½ the CPU; “simplicity wins” | 10–15 mechanism ops. Policy is Uiua. No 30 kSLOC CFS. |
| SAS + language/caps | Theseus, RedLeaf ≈ DPDK | One address space for granted Realms. Caps, not CR3, on the hot path. |
| Data-centric OS | DBOS: Linux-competitive + time-travel/SQL | OS state is arrays. Transforms are the query language. No VoltDB in ring 0. |

v1 is already (1)+(3)+(5) in miniature. (2) and GPU placement are later. Do not add a POSIX layer “for convenience.”

### Decision: Faster than Linux only where Linux is blind

See `research.md`. The OS is not faster because it is Uiua. It is faster when it still has the graph.

**v1 (legal, not required to fire):** UIR keeps named ops and shapes after load. Adjacent pure elementwise nodes are fusible. Sends are region caps.

**Next speed milestone (own change):** fuse `C=(A+B)×D` into one kernel; measure DRAM traffic. That is the first number that can beat an unfused Linux userspace launch of two kernels.

**Later stack, only on array work:**

| Bet | Why Linux loses | Evidence |
| --- | --- | --- |
| Fusion | Kernel never saw Add and Mul | TVM/IREE/FuseFlow; 1.5–3× memory-bound |
| Placement | GPU is another universe + bounce copies | Pathways, LithOS SOSP'25, GPU-resident graphs |
| Cap send | Pipes copy; shm is opt-in | RedLeaf, Theseus, Copier SOSP'25 |
| One transform, many lanes | 10⁷ elems are not 10⁷ threads | Futhark; array-language bakeoff 2025 |
| Bypass datapath | Syscall + kernel NIC | Junction NSDI'24 1.6–7×; Demikernel |
| Dumb hot path | CFS does not know the DAG | ghOSt, Skyloft SOSP'24, Caladan |

Do not claim a win on POSIX or pointer-chasing C. Do not use UnixBench as the scoreboard.

### Decision: Later, not now

These are compatible with the model and forbidden as first-milestone work:

- Content-addressed object store (`id = H(data)`); a Unix file API can be a service on top.
- Shell is Uiua; system tables are arrays (`Realms` → select → sort → take).
- Windows are `State × Events → Pixels`; the compositor is an array program.
- Record effect inputs (keys, clock, random, device completions) and replay a Realm.
- Self-hosted compiler: same UIR sources, new host.

## Risks / Trade-offs

- **[Risk] The Rust stepper becomes the OS.** → Mitigation: the policy-inversion scenario must pass with a Uiua-only edit.
- **[Risk] Official Uiua drifts.** → Mitigation: the subset is listed in-tree; each primitive is a later spec delta.
- **[Risk] Operation-array ABI is slow.** → Mitigation: first milestone is correctness; batching is the point of the ABI, not per-op roundtrips in the tiny program.
- **[Risk] Shape metadata is a lie and we store `void*` anyway.** → Mitigation: display region must report H×W×C; tests fail if it is only a pointer length.
- **[Risk] Readers hear "Linux in Uiua."** → Mitigation: law 1; success is the research test, not POSIX; `fork` and files are out of contract.
- **[Risk] UIR is lowered to a blob before the stepper, and we are Linux with extra steps.** → Mitigation: the Add-then-Multiply scenario must still show two nodes and an edge after load.
- **[Risk] Isolation without page tables is weak.** → Mitigation: first milestone is one Realm. Paging is a later machine change; Realms stay the same object.

## Migration Plan

No system exists. Sequence is `tasks.md`. Rollback is "do not boot the image."

## Open Questions

- Exact UIR opcode list beyond the subset — implementer's choice if the graph fields stay.
- Framebuffer glyphs vs VGA text — both satisfy the console spec.
- Event payload: scancode vs Unicode. First milestone only needs a character line.
- When object store, shell, compositor, SIMD/GPU, and replay each get their own change. Not this one.
