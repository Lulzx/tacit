## Context

Depends on `introduce-uiua-os` for boot, UIR, Realms, and caps. See that change for the machine. See `introduce-uiua-os/research.md` for citations.

This change is the speed program: Unikraft-style image + LionsOS-simple mechanism + fusion + cap-send + datapath + placement.

## Goals / Non-Goals

**Goals:**

- First number: fused `C=(A+B)×D` moves fewer bytes than unfused.
- Second number: cap-send of a large array copies fewer payload bytes than memcpy.
- Keep the image specialized. Keep the kernel off the inner loop.

**Non-Goals:**

- Bootstrapping the OS (other change).
- GPU in the first speed milestone.
- UnixBench, POSIX, DPDK-in-ring-0.

## Decisions

### Decision: Stack order

```text
5  placement     host now; GPU later as the same node
4  datapath      batched events/ops; no per-element trap
3  zero-copy     cap send default
2  fusion        first number  ← implement first
1  unikernel     already required; do not grow a general kernel
0  tiny mechanism / UIR names  (introduce-uiua-os)
```

Apply this change only after a stepper can load named Add and Multiply. Fusion is useless if UIR is already a blob.

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

### Decision: Counters live in the machine

The stepper records:

- payload bytes loaded/stored (or an honest proxy)
- payload bytes copied
- kernel entries (not per element)

Print them from the bench Realm. Do not trust host `perf` alone; QEMU still needs an in-image counter so the spec is testable.

### Decision: Cap send is grant, not memcpy

Same placement + immutable → increment a region ref / move a cap. Unique + mutate → in place. Different placement → explicit move node (later).

The memcpy path stays as a bench control, not the ABI.

### Decision: Datapath is already the ABI

Do not add io_uring or DPDK in this change. Enforce: fused kernel one entry; keyboard one array; no listen/accept. Junction/Demikernel are the later I/O change.

### Decision: Placement records, host only

UIR keeps `place = host`. Parallel axes stay. A later change may set `place = device` without a new source language.

## Risks / Trade-offs

- **[Risk] Stepper never exists, fusion is vapor.** → Mitigation: tasks start with “UIR still has named Add/Mul after load.” If that fails, stop; apply `introduce-uiua-os` first.
- **[Risk] QEMU counters lie.** → Mitigation: count region create/free and explicit copy ops in the allocator; that is enough to show 6N vs 4N.
- **[Risk] Fusion is wrong on fan-out.** → Mitigation: single-consumer rule; bench is a chain, not a diamond.
- **[Risk] We add a “general scheduler” while speeding up.** → Mitigation: datapath spec forbids per-element policy.

## Migration Plan

No production users. Sequence is `tasks.md`. Disable-fusion flag is the rollback.

## Open Questions

- Exact large shape for the bench (suggest `4096×4096` f64 or f32 if memory is tight on QEMU). Pick at implement time if the counter still shows 6N vs 4N.
- Whether the first fusion pass lives in the host compiler only (good) or also in the guest (not needed for v1).
