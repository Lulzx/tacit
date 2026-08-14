# Speed by design — research notes

Not “Linux is C, we are Uiua.” Linux is faster than a naive interpreter on almost everything. The only way this OS is *extremely* faster is on work whose meaning Linux has already thrown away — and on a machine whose meaning macOS throws away a second time, by pretending unified memory is two computers.

Claim we can defend:

> For array-shaped programs on Apple Silicon, keep UIR long enough to fuse, keep the working set in the system-level cache, transfer authority instead of bytes, and place the node on the engine that already matches the op. That is a different machine than “schedule opaque threads” and a different machine than “encode a Metal command buffer.”

Claim we must not make:

> Faster than Linux at running POSIX, Chrome, or pointer-chasing C. Faster because we deleted a host-to-device copy that this SoC does not have.

---

## The machine this is written for

Apple M4 Pro (reference):

- 12 or 14 CPU cores: 8 or 10 P + 4 E. P-cores: 128 KiB L1D, 16 MiB L2 per 5-core group. E-cores: 64 KiB L1D, 4 MiB shared L2.
- GPU 16 or 20 cores, tile-based. ANE 16-core, 38 TOPS.
- SME (ARM Scalable Matrix Extension) as the programmable face of the matrix coprocessor. No SVE. NEON is 128-bit.
- Unified LPDDR5X, up to 64 GB, 273 GB/s. One physical pool for CPU, GPU, ANE, display.
- System-level cache sits in front of DRAM for every engine.
- 16 KiB pages, 128-byte lines.
- Pointer authentication on AArch64.

QEMU `aarch64` virt + HVF is how we boot first. It is not this SoC. Speed claims that need SLC, SME, or GPU wait for metal or a documented lab runner. Fusion traffic (6N vs 4N) can still be counted in the guest.

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

One less full-array trip through the last-level cache and, if the set is large, through DRAM. Fusion literature on memory-bound GPU chains routinely reports about **1.5×–3×**, sometimes more when launch overhead dominates (small/medium tensors). That is not a microkernel trick. It is *seeing Add and Multiply as one graph*. Linux cannot do this: by the time it runs, those are loads and stores in a thread.

On this SoC a second cost sits under that: CPU, GPU, and ANE share 273 GB/s. An unfused intermediate that misses the SLC is not just “our DRAM tax.” It is bandwidth stolen from every other engine.

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

### 2. Keep the working set in the SLC; treat DRAM as shared

| Work | Year | What it shows |
| --- | --- | --- |
| Apple M-series cache topology (public + measured) | 2021–2026 | SLC is the last stop before DRAM for CPU and GPU. M4 Pro: 128 B lines, 16 KiB pages, 16 MiB P-L2 / 5 cores, 4 MiB E-L2. |
| Residual GPU cache state on M4 Pro (arXiv:2606.27098) | 2026 | Unified CPU–GPU memory still has a cache-state boundary after a GPU command. Occupancy can be measured. |
| Chips and Cheese / Eclectic Light on SLC | 2022–24 | SLC sizes from ~8 MB (base) into tens of MB on Pro/Max. It exists to cut DRAM demand from every block. |

**Steal:** tile fused kernels to P-cluster L2 or the SLC. Count DRAM spill, not a fake H2D. On virt, count the in-image traffic proxy and do not invent an SLC hit rate.

**macOS default:** Metal and processes still bounce ownership and command buffers even though the DRAM is shared.

### 3. Placement is an engine, not a trip to another memory

| Work | Year | What it shows |
| --- | --- | --- |
| Apple M4 SME (tzakharko; Jena Hello SME; LLVM M4 notes) | 2024–25 | M4 is the first shipping SME. AMX is no longer only behind Accelerate. No SVE; LLVM flags M4 as ARMv8.7-A for that reason. |
| AMX reverse-engineering (Dougall Johnson; Zhou MIT 2025) | 2021–25 | Matrix coprocessor is outer-product tiles, X/Y/Z register files, not NEON-with-extra-steps. |
| Pathways (Barham et al., MLSys 2022) | 2022 | Sharded async dataflow + futures; gang-schedule heterogeneous accelerators. Control plane runs ahead of the data plane. |
| LithOS (Coppock et al., SOSP 2025, arXiv:2504.15465) | 2025 | A *GPU OS*: spatial schedule at TPC grain. vs NVIDIA MPS: **13×** better inference tail latency. |

**Steal:** `engine ∈ {p-core, e-core, neon, sme, gpu, ane, media, display}`. Matmul goes to SME when wired, not through Metal. E-cores take events and policy. H2D/D2H is not a placement decision on this chip; cache domain is.

**Linux/macOS default:** CPU thread → allocate → (sometimes copy) → Metal/CoreML encode → wait. The API tax can dominate the math even when the copy is gone.

### 4. Zero-copy as the *model*, and also as the hardware

| Work | Year | What it shows |
| --- | --- | --- |
| RedLeaf (OSDI 2020) | 2020 | Language isolation, no process page tables; **10 Gbps driver matching DPDK**. Isolation ≠ slow. |
| Theseus (OSDI 2020) | 2020 | Single address space, single privilege; compiler-enforced isolation; IPC is a function call. |
| Copier (He et al., SOSP 2025) | 2025 | Linux itself is adding *coordinated async copy* as a first-class OS service — because `memcpy` across apps and kernel is a first-class problem. |
| Apple UMA | 2020–26 | CPU, GPU, ANE, display already share physical DRAM. A capability to a region *is* the transfer. |

**Steal:** immutable send = grant a region cap. O(1) metadata. Unique regions mutate in place. Copy is the fallback, implemented once (Copier-shaped), not the default. Changing engine on `uma` is not a copy.

**Linux default:** pipe/socket serialize. `shm` / `io_uring` / splice exist, but they are opt-in and still byte-oriented. macOS default: process isolation plus Metal buffers.

### 5. Do not turn arrays into kernel threads

10⁷ elementwise ops are **one transform**, maybe many tiles — not 10⁷ `clone`s.

Futhark’s whole point: the program already said “this is independent.” The machine should pick NEON / P-cores / SME / GPU. A 2025 bakeoff of five parallel array languages shows the winners are the compilers that *keep* that structure, not the ones that lower to serial C early.

**Linux default:** someone must write OpenMP, a thread pool, or a Metal kernel. The kernel only sees those workers.

### 6. Isolation without a TTBR0 tax on every Realm

Context switch + TLB shootdown is why “lots of processes” is expensive. RedLeaf/Theseus show software isolation can sit on the fast path. Pointer authentication is the AArch64 way to make a capability pointer unforgeable without a new address space.

**Caveat:** Uiua is not Rust. We cannot copy type-system isolation. Our analogue is **unforgeable capabilities + immutable regions**, later PAC — not “safe Uiua implies no MMU.”

**Steal the cost model, not the type system.** One address space for cooperating Realms that only hold the caps they were given. Paging is for untrusted later, not for every transform. 16 KiB pages cut TLB pressure on large arrays.

### 7. Datapath out of the kernel (I/O)

| Work | Year | What it shows |
| --- | --- | --- |
| Demikernel (SOSP 2021; still the 2024 SoCC story) | 2021–24 | µs datacenter I/O cannot afford the kernel. LibOS + portable datapath API. |
| Junction (NSDI 2024) | 2024 | Kernel-bypass that still multiplexes: **1.6×–7.0×** throughput vs native Linux. |
| Skyloft (SOSP 2024) | 2024 | User-space scheduling + user-mode interrupts; cheap timers without a kernel trip. |
| ghOSt / Caladan / Shenango | 2019–21 | Policy off the hot path. Two-level scheduling is how you stay fast when policy is clever. |

**Steal:** operation-array ABI + event arrays *are* a batching datapath. NIC/NVMe, when they exist, are libOS transforms over descriptor rings — not in-kernel TCP. Micro-scheduler stays dumb (law 7). E-cores are a natural home for that policy.

### 8. Locks are optional if readiness is the sync

Dataflow: `Ready(T) ⇔ ∀ inputs available`. No lock, no priority inversion, no false sharing on a mutex cache line.

This is old (classic dataflow machines; event-driven DAG runtimes). It is still the right concurrency tax for Uiua. Linux’s thread+mutex model pays that tax even when the math did not need it.

---

## Honest limits

- **PyTorch + XLA already fuses inside one ML process.** The unique win is a graph that spans *compute, compositor, camera, NIC* as one OS object — not beating Apple’s GEMM on SME.
- **Opaque POSIX, JIT languages, fork servers, pointer-chasing:** Linux wins. Opacity is why it runs COBOL.
- **First milestone will be slower than hosted Uiua on macOS.** QEMU + a stepper is not a speed demo. Speed starts when fusion + region-cap send exist, and becomes honest when SME/SLC run on the metal.
- **Do not quote “100× than Linux.”** Defensible bands: **~2–4×** on fused memory-bound pipelines vs unfused kernel launches; **large factors** when you delete pipe copies of GB arrays; **engine-quality factors** when matmul stops going through Metal; **1.6–7×** class I/O if we ever do Junction-style bypass. Stack those only on array-shaped work.
- **Do not quote an H2D win on M4 Pro.** There is no host memory and device memory. Claiming that win is measuring a PC.

---

## OS families that are faster *by construction*

Linux’s tax is generality: many layers, a shared kernel datapath, one scheduler/FS/net for everyone, KPTI, syscalls, demux. macOS’s extra tax on this chip is a userspace compute stack (Metal, CoreML, process isolation) sitting on top of UMA. Systems that *delete* those layers beat Linux without speaking Uiua. We steal the deletions.

### Unikernels / library OS (specialize the image)

**Unikraft** (EuroSys 2021 Best Paper; Linux Foundation). Modular micro-libraries. Images ~1 MB, <10 MB RAM, ~1 ms boot on top of the VMM. Nginx/Redis/SQLite: **1.7×–2.7×** vs Linux guests; **10–60% faster than native Linux** (syscall + KPTI gone; tighter allocator; specialized stacks).

Kin: OSv, Nanos, MirageOS. Same idea: one address space, only the components you linked.

**Fit:** the first QEMU `aarch64` image *is* a unikernel. One Realm, no POSIX, no FS, no TCP, no Metal. Uiua is a good language for *generating* that image: the unused kernel is not compiled in.

### Kernel off the data path

Arrakis (OSDI 2014), IX, then **Demikernel** (SOSP 2021): portable datapath API; ns-scale overhead once ported. **Junction** (NSDI 2024): 1.6×–7× vs Linux, fewer cores, still multiplexes.

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

Theseus, RedLeaf (OSDI 2020): isolation from the language, not TTBR0. RedLeaf NIC/NVMe approach DPDK/SPDK. Theseus: compiler owns resource rules.

**Fit:** cooperating Realms in one address space; caps, later PAC, not a page-table tax on the hot path. Uiua is *not* Rust — do not claim type-system isolation. Capability + immutability is our analogue.

### Data-centric OS (best conceptual match)

**DBOS** (Skiadopoulos et al. VLDB; MIT/Stanford → company 2024). Invert the stack: OS services are queries over a high-performance DBMS. Prototype performance **competitive with Linux**, plus time-travel, SQL observability. Commercial focus narrowed to durable workflows; the *vision* is the OS-as-data.

Kin: Twizzler (persistent memory as objects), Solros (heterogeneous data-centric), ExtOS (minimize movement).

**Fit:** we do not embed VoltDB. We do the Uiua version: Realms, ready sets, event batches, device queues, the machine description, and later object store are **arrays**. Bulk transforms replace lock-protected kernel trees.

### Other

- **Miosix fluid kernels:** hybrid embedded unikernel / general kernel; reported ~3.5× avg vs Linux. Treat as supporting evidence for specialization.
- **Asterinas framekernel:** Rust, Linux ABI, small TCB. Interesting for ABI, not our goal (we refuse Linux ABI).
- **Asahi Linux:** existence proof that Apple Silicon can boot something other than XNU. We steal the later metal map (AIC, DART, DCP), not the Linux ABI, and not as task 1.1.

### Principles that keep showing up

1. Specialize. Delete unused abstractions.
2. Kernel off the data path.
3. Zero-copy, bulk, array interfaces.
4. Single address space + language or capability isolation.
5. Tiny components (LionsOS).
6. Data is the citizen (DBOS / Uiua).
7. Use-case policy, not one scheduler to rule them all.
8. On UMA, engines and cache domains are the placement problem. Host/device is not.

### Practical path for this OS

Not a traditional multi-process kernel.

1. **Unikernel-style AArch64 image** specialized to array work (v1 already).
2. **Data-centric core** — important state is an array; ops are Uiua transforms.
3. **Bypass / zero-copy** on I/O when it exists.
4. **One address space** for granted Realms.
5. **No general FS/net/CFS/Metal** unless a later change proves it is needed.

Large constant-factor (sometimes asymptotic) wins come from refusing Linux’s compatibility layers *and* macOS’s compute-stack layers. Recent systems show that is practical.

---

## What we take into the design

1. UIR must remain *named ops + shapes + edges + home + engine* until after fusion and placement. Lowering to a blob before the stepper is how we become Linux.
2. Fusion of adjacent pure elementwise/reduce nodes is a first-class later milestone, legal in v1 UIR. Tile sizes name L2 or SLC when tiling exists.
3. Send is a capability. Copy is Copier, not `write(pipe)`. Engine change on `uma` is not a copy.
4. GPU, SME, and ANE are engines, not syscall universes and not Metal.
5. Two-level scheduler (Skyloft/ghOSt), events as batches (Demikernel), devices as arrays (descriptor rings). P vs E is the energy term.
6. PAC is the intended hardware for unforgeable caps. Software is enough on virt.
7. Measure the research test, not UnixBench, and not an H2D counter this chip does not have.
