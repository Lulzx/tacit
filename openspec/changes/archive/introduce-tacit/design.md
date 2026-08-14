## Context

Greenfield. See `proposal.md` for why and the seventeen delta specs for the contract. See `research.md` for citations behind the speed bets.

The difference is not “Linux is C, this is Uiua.” Linux believes computation is **opaque programs on a virtual computer**: process, address space, threads, file descriptors, syscalls. By the time the kernel runs `C = (A + B) × D`, it sees loads, stores, and a thread. The fact that Add is elementwise, that millions of elements are independent, that Multiply consumes Add, that the work could live on SME or the GPU — that meaning is already gone.

Tacit believes computation is a **graph the system still understands**:

```text
A ──┐
    ├─ Add ──┐
B ──┘        ├─ Multiply ──> C
D ───────────┘
```

Compiler, runtime, and scheduler share that graph. The first machine is an Apple M4 Pro: one unified memory pool, P-cores and E-cores, NEON, SME, GPU, ANE, media, display. Linux and macOS then *reintroduce* a host/device split through processes, Metal, and command buffers. Tacit does not.

Official Uiua 0.14 is tree-based, not bytecode, and its hosted runtime assumes an OS. Do not link that runtime into EL1.

## Goals / Non-Goals

**Goals:**

- Name the system **Tacit** and make `values + transformations + capabilities + placement` the actual architecture.
- Keep semantic UIR long enough that the scheduler places **transforms on engines**, not threads on a fake host/device split.
- First QEMU milestone: a **unikernel-style** AArch64 image on `aarch64` virt (HVF on the Mac) — console, 16 KiB-aware allocator, keyboard, one Realm, a machine description of the M4 Pro, one tiny program whose graph is still inspectable as text. No general FS, TCP, or process table.
- Five primitives only. Derive or refuse the 1970s nouns and the CUDA/Metal nouns. Effects go through propose / simulate / validate / commit. Agents are transforms over arrays of machine state.
- After named UIR loads: fuse `C=(A+B)×D` so traffic drops; send by region capability; keep the kernel off the inner loop. First engine is `p-core`. Home is `uma`.

**Non-Goals:**

- “Can it run Unix programs?” as a success metric.
- Unix, POSIX, files-as-worldview, TempleOS cosplay, Linux-in-glyphs.
- Embedding the official hosted interpreter.
- Kernel GC, kernel TCP, kernel filesystem.
- x86_64 as a required target.
- Native iBoot / Asahi bring-up in this change.
- Metal, CoreML, CUDA, or Accelerate as the guest compute API.
- Wiring SME, GPU, or ANE on the first boot image.
- UnixBench, DPDK-in-EL1, or a CUDA-shaped API.

## Decisions

### Decision: Two universes

| Values | Authorities |
| --- | --- |
| numbers, characters, arrays, boxes, functions | region, device, channel, clock, realm, execution, engine |

Values are computed. Authorities are minted only by the kernel. This is law 2. Treating capabilities as just more arrays would let arithmetic forge a disk. Treating engines as just more integers would let arithmetic forge a GPU.

**Alternative considered:** capabilities as tagged integers in arrays. Rejected: forging becomes arithmetic.

### Decision: First machine is Apple Silicon, first boot is virt

The reference SoC is an Apple M4 Pro:

```text
P-cluster (8 or 10)     E-cluster (4)     SME      GPU (16 or 20)
L2 16 MiB / 5 cores     L2 4 MiB                   TBDR
        \                   |               |         /
         \                  |               |        /
                    System Level Cache
                           |
              Unified LPDDR5X  (273 GB/s)
              home = uma. no VRAM.

Also on the fabric: ANE 16-core, media, display, SEP, DART.
Pages 16 KiB. Cache lines 128 B. ISA ARMv9.2-A: NEON + SME, not SVE.
```

The first image boots on **QEMU `aarch64` virt**, accelerated with **HVF** on the Mac. That gives the right ISA and a fast loop. It does not give a real SLC, SME, GPU, or ANE. The machine description still names those engines and marks only the boot CPU online.

**Alternative considered:** QEMU x86_64, because Limine tutorials are x86. Rejected: the development machine is AArch64; an x86 guest teaches the wrong page size, the wrong vector story, and a host/device habit.

**Alternative considered:** native iBoot/Asahi as task 1.1. Rejected: years of device bring-up before a ready banner. Native metal is a later machine change. Realms, UIR, and engines stay the same objects.

**Alternative considered:** Tacit as a macOS process using Accelerate/Metal. Rejected: that puts a general-purpose OS under the guest and fails the research test. A later *lab* runner on the host may measure SME/SLC. It is not the OS.

### Decision: Microkernel is mechanism only

About 10–15 ops: `alloc` / `map` / `unmap`, `spawn` / `stop` / `yield`, `send` / `recv`, `grant` / `revoke`, `wait` / `signal`, `map-device`, `clock`.

No files, no TCP, no `fork`, no `ioctl`, no Metal encoder. Services (store, net, compositor, shell) are Realms.

The ABI is an **operation array** in and a **result array** out. Crossing into the kernel is itself an array transform. Independent ops may be reordered or fused.

Architecture code is an AArch64 stub: exception levels, page tables, interrupt entry, atomics, special registers. Policy is Uiua.

### Decision: Combinator OS, not process OS

```text
intent (human or agent)
        ↓
 composition graph
        ↓
   pure work     effects
        ↓           ↓
  engines        capabilities
  p/e/neon/sme
  gpu/ane/...
        ↓           ↓
      world state
        ↓
   provenance graph
```

Five primitives only: values, transformations, composition, capabilities, evaluation. Files, processes, IPC, services, concurrency, GUI, Metal command buffers, and CUDA streams are derived from that vocabulary or they do not exist.

Linux: everything is a file. Tacit: everything is a composable transformation, and every effect carries authority and provenance.

### Decision: What Linux asks vs what this OS asks

| Linux / macOS | Tacit on Apple Silicon |
| --- | --- |
| Which thread runs on this CPU? | Which ready transform runs, on which engine? |
| Programmer or runtime spawns workers | Elementwise/`rows` already *is* the parallelism |
| Kernel sees RIP, loads, stores, syscalls | Kernel/runtime see Map, Reduce, Scan, MatMul, edges |
| Threads + locks | Ready(T) iff all inputs exist |
| Memory is addr + length + perms | Memory is type, shape, strides, home, cache domain |
| CPU RAM vs VRAM | One `uma` pool; engine is the interesting split |
| GPU is Metal / another universe | GPU is `engine = gpu` of the same transform |
| AMX hidden behind Accelerate | SME is a named engine |
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
  home, engine, cache domain, priority
```

Runnable iff unresolved dependencies are empty and caps are present. CPU threads are how a stepper happens to run, not what a program authors.

### Decision: UIR, not official bytecode and not LLVM-first

```text
Uiua subset → semantic tree → UIR
                 ├─ scalar / p-core
                 ├─ NEON              (later)
                 ├─ SME               (later; not SVE)
                 └─ graph
                      ↓
                 stepper / native AArch64
```

UIR records shape, purity, edges, regions, caps, parallel axes, home, engine, cache domain. Compiler and scheduler share that record. Official Uiua's tree compiler is a hint, not a crate we link into the guest.

M4 has SME and not SVE. Vector lowering is NEON (128-bit) or SME tiles. Do not design UIR as if scalable vectors exist.

First-milestone subset: numeric rank 0–2, character vectors, arithmetic, reduce, reshape, rank-wise map, display write, keyboard read. Everything else is a compile error.

### Decision: Realms, not processes

A Realm is heap/regions + transforms + cap table + quota + failure boundary. It is born with nothing. Boot grants the initial Realm display, keyboard, and a memory budget. It does not inherit a Metal device or a file table.

### Decision: Regions keep shape over UMA

```text
Array  = { region, offset, shape, strides }
Region = { type, shape, layout, home, cache }
home   ∈ { uma }          // later: device, persistent, mapped-io
cache  ∈ { l1, l2, slc, dram }
engine ∈ { p-core, e-core, neon, sme, gpu, ane, media, display }
```

First milestone implements `home = uma` and `engine = p-core` only. Slice/reshape/transpose are metadata when they can be. Allocations are 16 KiB aligned. Cache domain may be `dram` or unset on virt.

**Alternative considered:** keep `place ∈ { host, device, shared, … }` and treat UMA as `shared`. Rejected: that is the discrete-GPU model with a renamed case. Law 1 applies to GPU folklore too.

### Decision: Live graph is the machine

The running system is a queryable graph of transformations, values on edges, required capabilities, engines, and resource use. There is no process table that is the source of truth. `ps`/`top`/`strace`/`lsof`/`/proc` are projections of that graph.

First inspectability is text: nodes, edges, shapes, caps, engine on the console. A later GUI is another projection of the same object.

### Decision: Effects propose, then commit

An effectful node declares input, effect class, required capabilities, and output. An agent or operator submits a transformation as a proposal, receives a predicted world, and only then commits. Simulate walks the graph with the same stepper, writing to a shadow region or a predicted-effect record. Display write predicts "console would show X" without touching the framebuffer until commit.

Choosing `engine = sme` is placement, not an effect, unless it mutates the world.

Do not run an LLM in simulate. Agent means a granted transform. No model in the guest for this change.

### Decision: Agent algebra is UIR

The agent does not emit bash. It emits or selects a UIR composition. Machine tables (transforms, caps, engines, later files/net) are arrays. `filter(cpu > 0.8)` is a transform. Ten agents are ten nodes, not ten mystery PIDs.

First milestone: one granted transform that queries the live graph of the tiny program and commits a display summary.

### Decision: Events are batches; devices are arrays

```text
IRQ → {time, source, payload} → EventArray
      → select / group / partition / reduce
      → ready transforms
```

ISRs only acknowledge hardware. NIC/NVMe/GPU drivers, when they exist, are transforms over descriptor rings and completion arrays. Not the first boot image. Keyboard on virt is virtio-input or the documented QEMU default, not a required PS/2 driver.

### Decision: Two-level scheduler over engines

- **Micro:** take next ready node, run it on the assigned engine, record completion. This is the hot path (law 7).
- **Global placer:** periodically scores transforms by latency, movement, queue, energy. First milestone is one boot CPU, so the placer is a Uiua policy that only orders the ready array.

Later, on the metal:

```text
Score(t,d) = αL + βM + γQ + δE
d* = argmin_d (compute(t,d) + move(inputs(t),d))

fused elementwise     → p-core + NEON
matmul / outer product → sme
huge elementwise / UI  → gpu
learned transforms     → ane
events, queries, policy → e-core
```

On UMA, `move` is almost always cache-domain movement, not a memcpy.

### Decision: Layers, then the real diagram

Vertical stack (services are later Realms):

```text
applications
services          (UI, store, net — later)
array runtime     (shape, regions, graphs, effects, placement)
realms
global placer
microkernel
arch / AArch64 virt (later: Apple silicon metal)
```

The structure that actually matters:

```text
VALUES → TRANSFORM GRAPH
            ├─ pure work
            ├─ effects
            └─ authorities
                 ↓
               placer
            ├─ p-core / e-core
            ├─ neon / sme
            ├─ gpu / ane          (later)
            └─ media / display
                 ↓
              home = uma
```

### Decision: Boot

```text
QEMU aarch64 virt (+HVF on the Mac)
        → AArch64 stub (EL, page tables 16 KiB, stack, framebuffer, machine desc)
        → UIR runtime → initial Realm → tiny program
        → (later) device manager Realm → Uiua shell
        → (later still) native Apple Silicon path, same objects
```

Assembly/Rust is allowed only for: exception levels, interrupt entry, atomics, special registers. Push that line down over time. Do not write the scheduler there.

### Decision: Speed stack order

```text
6  engines       p-core now; SME/GPU/ANE later as the same node
5  SLC tiles     size fusion to L2/SLC; virt reports a traffic proxy
4  datapath      batched events/ops; no per-element trap
3  zero-copy     cap send default; UMA makes this the hardware default
2  fusion        first number  ← implement first after named UIR
1  unikernel     already required; do not grow a general kernel
0  tiny mechanism / UIR names / machine desc
```

Fusion is useless if UIR is already a blob. Apply fusion only after a stepper can load named Add and Multiply.

The PC bet “delete H2D/D2H” is already true in hardware here. Do not build the OS as if bounce copies were the main tax. The remaining tax is wrong engine, SLC eviction, and shared-bandwidth contention.

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

The stepper records payload bytes loaded/stored (or an honest proxy), payload bytes copied, and kernel entries. Print them from the bench Realm. Do not trust host `perf` alone; QEMU still needs an in-image counter. Do not print a fake SLC hit rate on virt.

When tiling exists, document the tile against P-cluster L2 (16 MiB / 5 cores on the 14-core M4 Pro) or the SLC. Virt may run untiled.

### Decision: Cap send is grant, not memcpy

Same home + immutable → increment a region ref / move a cap. Unique + mutate → in place. Changing engine on `uma` is not a copy. Different home (later, non-UMA machines) → explicit move node.

The memcpy path stays as a bench control, not the ABI.

### Decision: Datapath is already the ABI

Do not add io_uring, DPDK, or Metal. Enforce: fused kernel one entry; keyboard one array; no listen/accept. Junction/Demikernel are a later I/O change.

### Decision: PAC is the intended cap mechanism

Law 5 is unforgeable authority. On Apple Silicon, pointer authentication can sign capability pointers so arithmetic cannot mint one. First milestone on virt implements the same rule in software and does not require PAC to boot. A later machine change may turn on PAC without changing the capability spec.

**Alternative considered:** CHERI or MTE as the first isolation story. Rejected: virt may not provide them; PAC is the hardware this Mac already has.

### Decision: Steal deletions, not Linux features

Five construction styles beat Linux by *removing* layers. We use all five that fit; we do not become them.

| Style | Evidence | What we actually do |
| --- | --- | --- |
| Unikernel / libOS | Unikraft: 1.7–2.7× vs Linux VMs; 10–60% vs native; ~1 MB images | v1 image is one specialized AArch64 payload. No unused subsystem is linked. |
| Kernel off datapath | Demikernel, Junction 1.6–7× | Op-array ABI + event arrays. Future NIC is a libOS, not in-kernel TCP. |
| Tiny modular microkernel | LionsOS 2025: saturates 1 Gb/s where Linux cannot; ~½ the CPU; “simplicity wins” | 10–15 mechanism ops. Policy is Uiua. No 30 kSLOC CFS. |
| SAS + language/caps | Theseus, RedLeaf ≈ DPDK | One address space for granted Realms. Caps (later PAC), not a CR3 tax, on the hot path. |
| Data-centric OS | DBOS: Linux-competitive + time-travel/SQL | OS state is arrays. Transforms are the query language. No VoltDB in EL1. |

v1 is already (1)+(3)+(5) in miniature. Fusion and engine placement come after named UIR. Do not add a POSIX layer or a Metal layer “for convenience.”

### Decision: Faster than Linux only where Linux is blind

See `research.md`. The OS is not faster because it is Uiua. It is faster when it still has the graph, and when it uses UMA as UMA.

**First boot image (legal, not required to fire):** UIR keeps named ops, shapes, `home = uma`, and `engine = p-core` after load. Adjacent pure elementwise nodes are fusible. Sends are region caps. The machine description names the M4 Pro engines.

**First speed number (later tasks in this change):** fuse `C=(A+B)×D` into one kernel; measure traffic. That is the first number that can beat an unfused launch of two kernels.

**Later stack, only on array work, on the metal:**

| Bet | Why Linux/macOS lose | Evidence |
| --- | --- | --- |
| Fusion | Kernel never saw Add and Mul | TVM/IREE/FuseFlow; 1.5–3× memory-bound |
| SLC-sized tiles | Working set falls to 273 GB/s DRAM shared with GPU/ANE | M4 Pro cache topology; residual-GPU-cache work 2026 |
| Engine placement | Matmul goes through Metal or sits on an E-core | SME on M4; Accelerate hides AMX |
| Cap send | Pipes copy; shm is opt-in | RedLeaf, Theseus, Copier SOSP'25; UMA makes share free |
| One transform, many lanes | 10⁷ elems are not 10⁷ threads | Futhark; array-language bakeoff 2025 |
| Bypass datapath | Syscall + kernel NIC | Junction NSDI'24 1.6–7×; Demikernel |
| Dumb hot path | CFS does not know the DAG | ghOSt, Skyloft SOSP'24, Caladan |

Do not claim a win on POSIX or pointer-chasing C. Do not use UnixBench as the scoreboard. Do not claim an H2D win on a machine that has no H2D.

### Decision: Later, not now

These are compatible with the model and forbidden as first-milestone work:

- SME / NEON lowering of named UIR.
- GPU / ANE as online engines.
- Native Apple Silicon boot (iBoot, AIC, DART, DCP).
- PAC-signed capability pointers.
- SLC occupancy counters that require metal.
- Content-addressed object store (`id = H(data)`).
- Shell is Uiua; system tables are arrays (`Realms` → select → sort → take).
- Windows are `State × Events → Pixels`; the compositor is an array program.
- Record effect inputs (keys, clock, random, device completions) and replay a Realm.
- Self-hosted compiler: same UIR sources, new host.
- Multi-agent planner.

## Risks / Trade-offs

- **[Risk] The Rust stepper becomes the OS.** → Mitigation: the policy-inversion scenario must pass with a Uiua-only edit.
- **[Risk] Official Uiua drifts.** → Mitigation: the subset is listed in-tree; each primitive is a later spec delta.
- **[Risk] Operation-array ABI is slow.** → Mitigation: first milestone is correctness; batching is the point of the ABI, not per-op roundtrips in the tiny program.
- **[Risk] Shape metadata is a lie and we store `void*` anyway.** → Mitigation: display region must report H×W×C; tests fail if it is only a pointer length.
- **[Risk] Readers hear "Linux in Uiua" or "macOS in glyphs."** → Mitigation: law 1; success is the research test, not POSIX; `fork`, files, and Metal are out of contract.
- **[Risk] UIR is lowered to a blob before the stepper, and we are Linux with extra steps.** → Mitigation: the Add-then-Multiply scenario must still show two nodes and an edge after load. Fusion is a later rewrite of that named graph.
- **[Risk] Isolation without page tables is weak.** → Mitigation: first milestone is one Realm. Paging is a later machine change; PAC is a later cap mechanism; Realms stay the same object.
- **[Risk] The graph store becomes a second kernel.** → Mitigation: it *is* the kernel's account of work. No shadow process table.
- **[Risk] Simulate is a full second OS.** → Mitigation: first slice only predicts display and cap checks.
- **[Risk] "Agent" invites a chatbot in EL1.** → Mitigation: agent means a granted transform. No model in the guest.
- **[Risk] QEMU counters lie.** → Mitigation: count region create/free and explicit copy ops in the allocator; that is enough to show 6N vs 4N. Do not invent SLC numbers.
- **[Risk] Fusion is wrong on fan-out.** → Mitigation: single-consumer rule; bench is a chain, not a diamond.
- **[Risk] Virt teaches a fake machine.** → Mitigation: the machine description names offline engines from day one; first-milestone code must not require them.
- **[Risk] Someone wires Metal to "use the GPU."** → Mitigation: combinators spec refuses vendor compute APIs as core objects. GPU is `engine = gpu`.
- **[Risk] Native boot becomes the implicit next task.** → Mitigation: boot spec forbids iBoot/AIC/DART as a first-milestone dependency.

## Migration Plan

No system exists. Sequence is `tasks.md`: boot the AArch64 image, publish the machine description, keep named UIR, expose the graph, then fuse. Rollback is "do not boot the image." Disable-fusion flag is the rollback for the speed tasks. A later native-metal change retargets the stub only.

## Open Questions

- Exact UIR opcode list beyond the subset — implementer's choice if the graph fields stay.
- Framebuffer glyphs vs virtio-gpu vs RAM framebuffer — any visible console satisfies the console spec.
- Event payload: scancode vs Unicode. First milestone only needs a character line.
- Exact text format of the first graph projection. Pick at implement time if nodes, edges, shapes, and engine are all present.
- Exact large shape for the fusion bench (suggest `4096×4096` f32 if memory is tight on QEMU). Pick at implement time if the counter still shows 6N vs 4N.
- Whether the first fusion pass lives in the host compiler only (good) or also in the guest (not needed for v1).
- virtio-input versus the QEMU default keyboard device. Pick at implement time if keys become an event array.
- When SME, GPU, ANE, PAC, native metal, object store, shell, compositor, graphical projection, and replay each get their own later change.
