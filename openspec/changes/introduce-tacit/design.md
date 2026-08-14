## Context

Greenfield. See `proposal.md` for why and the seventeen delta specs for the contract. See `research.md` for citations behind the speed bets.

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
- First QEMU milestone: a **unikernel-style** image — console, allocator, keyboard, one Realm, one tiny program whose graph is still inspectable as text. No general FS, TCP, or process table.
- Five primitives only. Derive or refuse the 1970s nouns. Effects go through propose / simulate / validate / commit. Agents are transforms over arrays of machine state.
- After named UIR loads: fuse `C=(A+B)×D` so traffic drops; send by region capability; keep the kernel off the inner loop. Host placement only.

**Non-Goals:**

- “Can it run Unix programs?” as a success metric.
- Unix, POSIX, files-as-worldview, TempleOS cosplay, Linux-in-glyphs.
- Embedding the official hosted interpreter.
- Kernel GC, kernel TCP, kernel filesystem.
- GPU, NIC, NVMe, compositor, LLM weights, or a multi-agent planner in the first boot image.
- UnixBench, DPDK-in-ring-0, or a CUDA-shaped API.

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

### Decision: Combinator OS, not process OS

```text
intent (human or agent)
        ↓
 composition graph
        ↓
   pure work     effects
        ↓           ↓
  CPU/GPU/NPU   capabilities
        ↓           ↓
      world state
        ↓
   provenance graph
```

Five primitives only: values, transformations, composition, capabilities, evaluation. Files, processes, IPC, services, concurrency, and later GUI are derived from that vocabulary or they do not exist.

Linux: everything is a file. Tacit: everything is a composable transformation, and every effect carries authority and provenance.

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

### Decision: Live graph is the machine

The running system is a queryable graph of transformations, values on edges, required capabilities, and resource use. There is no process table that is the source of truth. `ps`/`top`/`strace`/`lsof`/`/proc` are projections of that graph.

First inspectability is text: nodes, edges, shapes, caps on the console. A later GUI is another projection of the same object. Drag-to-compose is specified now so the GUI cannot become a launcher for Unix binaries.

### Decision: Effects propose, then commit

An effectful node declares input, effect class, required capabilities, and output. An agent or operator submits a transformation as a proposal, receives a predicted world, and only then commits. Simulate walks the graph with the same stepper, writing to a shadow region or a predicted-effect record. Display write predicts "console would show X" without touching the framebuffer until commit.

Do not run an LLM in simulate. Agent means a granted transform. No model in the guest for this change.

### Decision: Agent algebra is UIR

The agent does not emit bash. It emits or selects a UIR composition. Machine tables (transforms, caps, later files/net) are arrays. `filter(cpu > 0.8)` is a transform. Ten agents are ten nodes, not ten mystery PIDs.

First milestone: one granted transform that queries the live graph of the tiny program and commits a display summary.

### Decision: Events are batches; devices are arrays

```text
IRQ → {time, source, payload} → EventArray
      → select / group / partition / reduce
      → ready transforms
```

ISRs only acknowledge hardware. NIC/NVMe/GPU drivers, when they exist, are transforms over descriptor rings and completion arrays — those devices are already arrays. Not the first boot image.

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

### Decision: Speed stack order

```text
5  placement     host now; GPU later as the same node
4  datapath      batched events/ops; no per-element trap
3  zero-copy     cap send default
2  fusion        first number  ← implement first after named UIR
1  unikernel     already required; do not grow a general kernel
0  tiny mechanism / UIR names
```

Fusion is useless if UIR is already a blob. Apply fusion only after a stepper can load named Add and Multiply.

### Decision: Fusion is a UIR rewrite

Unfused:

```text
A,B → Add → T → Multiply(T,D) → C
traffic ≈ 2N + N + 2N + N = 6N
```

Fused:

```text
A,B,D → AddMul → C
traffic ≈ 3N + N = 4N
```

Pass: walk pure elementwise chains with a single consumer; emit one kernel; drop the intermediate region. Disable flag for the bench.

Do not fuse across effectful nodes or fan-out that would double-materialize unless the pass proves it is cheaper.

The stepper records payload bytes loaded/stored (or an honest proxy), payload bytes copied, and kernel entries. Print them from the bench Realm. Do not trust host `perf` alone; QEMU still needs an in-image counter.

### Decision: Cap send is grant, not memcpy

Same placement + immutable → increment a region ref / move a cap. Unique + mutate → in place. Different placement → explicit move node (later).

The memcpy path stays as a bench control, not the ABI.

### Decision: Datapath is already the ABI

Do not add io_uring or DPDK. Enforce: fused kernel one entry; keyboard one array; no listen/accept. Junction/Demikernel are a later I/O change.

### Decision: Placement records, host only

UIR keeps `place = host`. Parallel axes stay. A later change may set `place = device` without a new source language. Bounce copies are explicit and counted. A large independent axis is tiles or vector lanes of one transform, not one thread per element.

### Decision: Steal deletions, not Linux features

Five construction styles beat Linux by *removing* layers. We use all five that fit; we do not become them.

| Style | Evidence | What we actually do |
| --- | --- | --- |
| Unikernel / libOS | Unikraft: 1.7–2.7× vs Linux VMs; 10–60% vs native; ~1 MB images | v1 image is one specialized payload. No unused subsystem is linked. |
| Kernel off datapath | Demikernel, Junction 1.6–7× | Op-array ABI + event arrays. Future NIC is a libOS, not in-kernel TCP. |
| Tiny modular microkernel | LionsOS 2025: saturates 1 Gb/s where Linux cannot; ~½ the CPU; “simplicity wins” | 10–15 mechanism ops. Policy is Uiua. No 30 kSLOC CFS. |
| SAS + language/caps | Theseus, RedLeaf ≈ DPDK | One address space for granted Realms. Caps, not CR3, on the hot path. |
| Data-centric OS | DBOS: Linux-competitive + time-travel/SQL | OS state is arrays. Transforms are the query language. No VoltDB in ring 0. |

v1 is already (1)+(3)+(5) in miniature. Fusion and GPU placement come after named UIR. Do not add a POSIX layer “for convenience.”

### Decision: Faster than Linux only where Linux is blind

See `research.md`. The OS is not faster because it is Uiua. It is faster when it still has the graph.

**First boot image (legal, not required to fire):** UIR keeps named ops and shapes after load. Adjacent pure elementwise nodes are fusible. Sends are region caps.

**First speed number (later tasks in this change):** fuse `C=(A+B)×D` into one kernel; measure DRAM traffic. That is the first number that can beat an unfused Linux userspace launch of two kernels.

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
- Device / GPU placement of the same UIR node.
- Multi-agent planner.

## Risks / Trade-offs

- **[Risk] The Rust stepper becomes the OS.** → Mitigation: the policy-inversion scenario must pass with a Uiua-only edit.
- **[Risk] Official Uiua drifts.** → Mitigation: the subset is listed in-tree; each primitive is a later spec delta.
- **[Risk] Operation-array ABI is slow.** → Mitigation: first milestone is correctness; batching is the point of the ABI, not per-op roundtrips in the tiny program.
- **[Risk] Shape metadata is a lie and we store `void*` anyway.** → Mitigation: display region must report H×W×C; tests fail if it is only a pointer length.
- **[Risk] Readers hear "Linux in Uiua."** → Mitigation: law 1; success is the research test, not POSIX; `fork` and files are out of contract.
- **[Risk] UIR is lowered to a blob before the stepper, and we are Linux with extra steps.** → Mitigation: the Add-then-Multiply scenario must still show two nodes and an edge after load. Fusion is a later rewrite of that named graph.
- **[Risk] Isolation without page tables is weak.** → Mitigation: first milestone is one Realm. Paging is a later machine change; Realms stay the same object.
- **[Risk] The graph store becomes a second kernel.** → Mitigation: it *is* the kernel's account of work. No shadow process table.
- **[Risk] Simulate is a full second OS.** → Mitigation: first slice only predicts display and cap checks.
- **[Risk] "Agent" invites a chatbot in ring 0.** → Mitigation: agent means a granted transform. No model in the guest.
- **[Risk] QEMU counters lie.** → Mitigation: count region create/free and explicit copy ops in the allocator; that is enough to show 6N vs 4N.
- **[Risk] Fusion is wrong on fan-out.** → Mitigation: single-consumer rule; bench is a chain, not a diamond.

## Migration Plan

No system exists. Sequence is `tasks.md`: boot the image, keep named UIR, expose the graph, then fuse. Rollback is "do not boot the image." Disable-fusion flag is the rollback for the speed tasks.

## Open Questions

- Exact UIR opcode list beyond the subset — implementer's choice if the graph fields stay.
- Framebuffer glyphs vs VGA text — both satisfy the console spec.
- Event payload: scancode vs Unicode. First milestone only needs a character line.
- Exact text format of the first graph projection. Pick at implement time if nodes, edges, and shapes are all present.
- Exact large shape for the fusion bench (suggest `4096×4096` f64 or f32 if memory is tight on QEMU). Pick at implement time if the counter still shows 6N vs 4N.
- Whether the first fusion pass lives in the host compiler only (good) or also in the guest (not needed for v1).
- When object store, shell, compositor, SIMD/GPU, graphical projection, and replay each get their own later change.
