# Speed by design — research notes

Not “Linux is C, we are Uiua.” Linux is faster than a naive interpreter on almost everything. The only way this OS is *extremely* faster is on work whose meaning Linux has already thrown away.

Claim we can defend:

> For array-shaped programs, keep UIR long enough to fuse, place, and transfer authority instead of bytes. That is a different machine than “schedule opaque threads.”

Claim we must not make:

> Faster than Linux at running POSIX, Chrome, or pointer-chasing C.

---

## Where the time actually goes

Elementwise `C = (A+B)×D` on large arrays is **memory-bound**. Roofline: `P ≤ min(P_peak, I · B)`. Add then multiply as two kernels does:

```text
read A, B  →  write T  →  read T, D  →  write C
```

Fused:

```text
read A, B, D  →  write C
```

One less full-array trip through DRAM. Fusion literature on memory-bound GPU chains routinely reports about **1.5×–3×**, sometimes more when launch overhead dominates (small/medium tensors). That is not a microkernel trick. It is *seeing Add and Multiply as one graph*. Linux cannot do this: by the time it runs, those are loads and stores in a thread.

---

## Bets, strongest first

### 1. Fuse across what used to be the compiler/kernel/app boundary

| Work | Year | What it shows |
| --- | --- | --- |
| TVM / XLA / IREE / PyTorch JIT | 2018–2025 | Operator fusion is the main win for tensor programs. IREE keeps a tensor IR all the way to devices. |
| FuseFlow (arXiv:2511.04768) | 2025 | Cross-expression fusion of sparse ops into a dataflow graph, plus ordering and blocking. |
| CUDA fusion measurements | 2025–26 | 1.5×–3.13× on memory-bound elementwise / activations; launch cost matters. |
| CUDA Graphs vs fusion | 2025 | Graphs cut *launch* latency; fusion cuts *memory traffic*. Need both. |
| Futhark + 2025 array-language bakeoff (arXiv:2505.08906) | 2025 | Pure array languages can match conventional CPU/GPU code *when the compiler keeps the parallel structure*. |

**Steal:** UIR nodes stay named (`Add`, `Mul`, `Reduce`, `Reshape`) until a placer/fusion pass. The tiny program `C=(A+B)×D` is the first fusion candidate.

**Linux cannot:** it scheduled a thread. The math is gone.

### 2. Placement, not “call into the GPU universe”

| Work | Year | What it shows |
| --- | --- | --- |
| Pathways (Barham et al., MLSys 2022) | 2022 | Sharded async dataflow + futures; gang-schedule heterogeneous accelerators; ~100% TPU util on 2048 chips. Control plane runs ahead of the data plane. |
| LithOS (Coppock et al., SOSP 2025, arXiv:2504.15465) | 2025 | A *GPU OS*: spatial schedule at TPC grain, atomize kernels, right-size resources. vs NVIDIA MPS: **13×** better inference tail latency; vs best prior: **3×** tail, **1.6×** throughput. Energy/capacity savings ~25% for a few percent perf. |
| Holoscan GPU-resident graphs | 2024–25 | Keep the pipeline on the GPU. Host bounce is the tax. |
| CXL pooling / GPU-CXL DMA (2025) | 2025 | Shared memory fabric so GPUs do not NIC-hop KV/blocks. Placement becomes a region attribute. |

**Steal:** a transform already in device memory stays there. H2D/D2H is a placement decision, not an API. LithOS is what a GPU looks like if it is a first-class compute resource instead of `ioctl`.

**Linux default:** CPU thread → allocate → copy → launch → sync → copy back. That path can dominate the math.

### 3. Zero-copy as the *model*, not an optimization

| Work | Year | What it shows |
| --- | --- | --- |
| RedLeaf (OSDI 2020) | 2020 | Language isolation, no process page tables; **10 Gbps driver matching DPDK**. Isolation ≠ slow. |
| Theseus (OSDI 2020) | 2020 | Single address space, single privilege; compiler-enforced isolation; IPC is a function call. Intralingual: close the compiler/hardware gap. |
| Copier (He et al., SOSP 2025) | 2025 | Linux itself is adding *coordinated async copy* as a first-class OS service — because `memcpy` across apps and kernel is a first-class problem. |
| CXL / unified regions | 2025–26 | If the fabric is load/store, a capability to a region is the transfer. |

**Steal:** immutable send = grant a region cap. O(1) metadata. Unique regions mutate in place. Copy is the fallback, implemented once (Copier-shaped), not the default.

**Linux default:** pipe/socket serialize. `shm` / `io_uring` / splice exist, but they are opt-in and still byte-oriented.

### 4. Do not turn arrays into kernel threads

10⁷ elementwise ops are **one transform**, maybe many tiles — not 10⁷ `clone`s.

Futhark’s whole point: the program already said “this is independent.” The machine should pick SIMD / multicore / GPU. A 2025 bakeoff of five parallel array languages shows the winners are the compilers that *keep* that structure, not the ones that lower to serial C early.

**Linux default:** someone must write OpenMP, a thread pool, or a CUDA kernel. The kernel only sees those workers.

### 5. Isolation without a CR3 tax on every Realm

Context switch + TLB shootdown is why “lots of processes” is expensive. RedLeaf/Theseus show software isolation can sit on the fast path.

**Caveat:** Uiua is not Rust. We cannot copy their type-system isolation. Our analogue is **unforgeable capabilities + immutable regions**, later maybe MPK/CHERI on the machine layer — not “safe Uiua implies no MMU.”

**Steal the cost model, not the type system.** One address space for cooperating Realms that only hold the caps they were given. Paging is for untrusted later, not for every transform.

### 6. Datapath out of the kernel (I/O)

| Work | Year | What it shows |
| --- | --- | --- |
| Demikernel (SOSP 2021; still the 2024 SoCC story) | 2021–24 | µs datacenter I/O cannot afford the kernel. LibOS + portable datapath API across DPDK/RDMA/io_uring. |
| Junction (NSDI 2024) | 2024 | Kernel-bypass that still multiplexes: **1.6×–7.0×** throughput vs native Linux, **1.2×–3.8×** fewer cores, 19–62× more instances than prior bypass systems. |
| Skyloft (SOSP 2024) | 2024 | User-space scheduling + user-mode interrupts; cheap timers without a kernel trip. |
| ghOSt / Caladan / Shenango | 2019–21 | Policy off the hot path. Two-level scheduling is how you stay fast when policy is clever. |

**Steal:** operation-array ABI + event arrays *are* a batching datapath. NIC/NVMe, when they exist, are libOS transforms over descriptor rings — not in-kernel TCP. Micro-scheduler stays dumb (law 7). Skyloft-style user interrupts fit “events become arrays.”

### 7. Locks are optional if readiness is the sync

Dataflow: `Ready(T) ⇔ ∀ inputs available`. No lock, no priority inversion, no false sharing on a mutex cache line.

This is old (classic dataflow machines; event-driven DAG runtimes). It is still the right concurrency tax for Uiua. Linux’s thread+mutex model pays that tax even when the math did not need it.

---

## Honest limits

- **PyTorch + XLA already fuses inside one ML process.** The unique win is a graph that spans *compute, compositor, camera, NIC* as one OS object — not beating cuDNN at matmul.
- **Opaque POSIX, JIT languages, fork servers, pointer-chasing:** Linux wins. Opacity is why it runs COBOL.
- **First milestone will be slower than Linux userspace Uiua.** QEMU + a stepper is not a speed demo. Speed starts when fusion + region-cap send exist on real hardware.
- **Do not quote “100× than Linux.”** Defensible bands: **~2–4×** on fused memory-bound pipelines vs unfused kernel launches; **large factors** when you delete H2D/D2H or pipe copies of GB arrays; **1.6–7×** class I/O if we ever do Junction-style bypass. Stack those only on array-shaped work.

---

## OS families that are faster *by construction*

Linux’s tax is generality: many layers, a shared kernel datapath, one scheduler/FS/net for everyone, KPTI, syscalls, demux. Systems that *delete* those layers beat Linux without speaking Uiua. We steal the deletions.

### Unikernels / library OS (specialize the image)

**Unikraft** (EuroSys 2021 Best Paper; Linux Foundation). Modular micro-libraries. Images ~1 MB, <10 MB RAM, ~1 ms boot on top of the VMM. Nginx/Redis/SQLite: **1.7×–2.7×** vs Linux guests; **10–60% faster than native Linux** (syscall + KPTI gone; tighter allocator; specialized stacks). Extreme density is the product, not a benchmark footnote.

Kin: OSv, Nanos, MirageOS. Same idea: one address space, only the components you linked.

**Fit:** the first QEMU image *is* a unikernel. One Realm, no POSIX, no FS, no TCP. Uiua is a good language for *generating* that image: the unused kernel is not compiled in.

### Kernel off the data path

Arrakis (OSDI 2014), IX, then **Demikernel** (SOSP 2021, Rust, still the 2024 SoCC story): portable datapath API over DPDK/RDMA/io_uring; ns-scale overhead once ported. **Junction** (NSDI 2024): 1.6×–7× vs Linux, fewer cores, still multiplexes.

**Fit:** operation-array ABI + event arrays. NIC/NVMe, if they ever exist, are libOS transforms over rings — not in-kernel TCP.

### Simple microkernel that still wins

**seL4:** fastest verified IPC (hundreds of cycles).

**LionsOS** (Heiser et al., arXiv:2501.06234, Jan 2025; seL4 Summit 2025). Static architecture, tiny components, *use-case-specific policies*. Drivers 4.5–8.4× smaller than Linux. Despite *more* context switches than Linux:

- Arm 1 Gb/s UDP echo: LionsOS saturates the NIC with CPU left; Linux tops out ~600 Mb/s with the core maxed (~2× CPU for the same work).
- x86 10 Gb/s: LionsOS ~7 Gb/s; Linux peaks ~3.5 Gb/s then *collapses*.
- RTT: LionsOS <200 µs vs Linux ~1000 µs on that bench.

Their sentence: **“Simplicity wins — it more than compensates for the high context-switch rates.”**

**Fit:** our microkernel stays 10–15 ops. Policy is a Uiua transform, not a 30 kSLOC CFS. Do not grow a universal scheduler.

### Language / single-address-space isolation

Theseus, RedLeaf (OSDI 2020): isolation from the language, not CR3. RedLeaf NIC/NVMe approach DPDK/SPDK. Theseus: compiler owns resource rules.

**Fit:** cooperating Realms in one address space; caps, not page tables, on the hot path. Uiua is *not* Rust — do not claim type-system isolation. Capability + immutability is our analogue.

### Data-centric OS (best conceptual match)

**DBOS** (Skiadopoulos et al. VLDB; MIT/Stanford → company 2024). Invert the stack: OS services are queries over a high-performance DBMS. Process/file/message state lives in tables. Prototype performance **competitive with Linux**, plus time-travel, SQL observability, multi-node without Kubernetes. Commercial focus narrowed to durable workflows; the *vision* is the OS-as-data.

Kin: Twizzler (persistent memory as objects), Solros (heterogeneous data-centric), ExtOS (minimize movement).

**Fit:** we do not embed VoltDB. We do the Uiua version: Realms, ready sets, event batches, device queues, and later object store are **arrays**. Bulk transforms replace lock-protected kernel trees. DBOS proves “OS state is a table” is fast enough; Uiua is the query language.

### Other

- **Miosix fluid kernels:** hybrid embedded unikernel / general kernel; reported ~3.5× avg vs Linux (up to 15×) and large code-size cuts. Treat as supporting evidence for specialization, not a crate to fork.
- **Asterinas framekernel:** Rust, Linux ABI, small TCB. On par with Linux — interesting for ABI, not our goal (we refuse Linux ABI).
- **CXL / DPU / SmartNIC:** design regions and placement for fabrics, do not retrofit later.

### Principles that keep showing up

1. Specialize. Delete unused abstractions.
2. Kernel off the data path.
3. Zero-copy, bulk, array interfaces.
4. Single address space + language or capability isolation.
5. Tiny components (LionsOS).
6. Data is the citizen (DBOS / Uiua).
7. Use-case policy, not one scheduler to rule them all.

### Practical path for this OS

Not a traditional multi-process kernel.

1. **Unikernel-style image** specialized to array work (v1 already).
2. **Data-centric core** — important state is an array; ops are Uiua transforms.
3. **Bypass / zero-copy** on I/O when it exists.
4. **One address space** for granted Realms.
5. **No general FS/net/CFS** unless a later change proves it is needed.

Large constant-factor (sometimes asymptotic) wins come from refusing Linux’s compatibility layers. Recent systems show that is practical.

---

## What we take into the design

1. UIR must remain *named ops + shapes + edges* until after fusion and placement. Lowering to a blob before the stepper is how we become Linux.
2. Fusion of adjacent pure elementwise/reduce nodes is a first-class later milestone, legal in v1 UIR.
3. Send is a capability. Copy is Copier, not `write(pipe)`.
4. GPU is a placement and a LithOS-like resource, not a syscall universe.
5. Two-level scheduler (Skyloft/ghOSt), events as batches (Demikernel), devices as arrays (descriptor rings).
6. Measure the research test, not UnixBench.
