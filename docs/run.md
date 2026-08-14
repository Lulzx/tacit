# Build and run

The development host is an Apple Silicon Mac. The guest is freestanding —
there is no Linux or macOS in the guest image.

## Build

```sh
./build.sh
```

This runs the host compiler (Uiua subset → UIR), then builds the freestanding
AArch64 microkernel image. The image lands at `build/tacit.elf` (and
`build/tacit.img`).

- The guest enables the MMU with **16 KiB-granule** identity-mapped page
  tables (RAM normal-WB, MMIO device), plus caches; the framebuffer is
  flushed to coherent RAM so QEMU's ramfb sees it.
- The first milestone target is QEMU `aarch64` virt. Requesting any other
  boot architecture fails with an explicit error:
  `ARCH=x86_64 ./build.sh` → *unsupported boot architecture 'x86_64'*.
- Scheduler/grant policy is Uiua compiled to UIR. To invert the documented
  order key, rebuild with the inverted policy source:
  `POLICY=policy-rev.ua ./build.sh`. No AArch64 stub edit is required.
- The fusion bench shape is `2048x2048` f32 (see `uiua/bench-fusion.ua`).

## Run

```sh
./qemu.sh
```

Boots the image under QEMU `aarch64` virt with **HVF** on this Mac (TCG is
the documented fallback when HVF is unavailable), `-m 1G`, a single boot CPU,
a ramfb framebuffer, and the serial console on stdio. Keyboard input arrives
over the serial console (the documented QEMU default) and is delivered to the
guest as an event array; type a line and press Enter to see it echoed.

After the ready banner the guest, unattended:

1. publishes the M4 Pro machine description (`home = uma`, 16 KiB pages,
   128-byte lines, the boot CPU online with its NEON unit wired; e-core, SME,
   GPU, ANE, media, display named but offline),
2. runs the bundled program `C = (A + B) × D`, printing its live graph
   (Add, Multiply, the edge, caps, parallel axes) and the provenance of `C`
   plus per-node payload-byte counters,
3. runs one granted agent-shaped transform that summarizes the live graph
   with two fan-out summaries (ordered by the policy),
4. exercises the subset (reduce, reshape, grade/select/keep, rank-wise map,
   the capabilities table),
5. demonstrates effects (propose → simulate → validate → commit; a missing or
   forged display cap leaves the console unchanged) and the operation-array
   ABI (batching with dependencies, unmet dependencies refused),
6. prints the fusion bench (fused bytes < unfused bytes) and the zero-copy
   send bench (capability share vs explicit copy, unique-region in-place
   mutation vs immutable refusal).

Pure elementwise nodes are placed on the **NEON engine**: the stepper
dispatches `engine = neon` to a 128-bit Advanced-SIMD kernel, and the fused
Add-then-Multiply runs as one NEON entry. `&stats` reports a per-engine
breakdown (e.g. `kernel entries: 1 (neon 1)` for the fused bench). This is
speed-stack level 6 — engines as placements of the same node — in its first
slice; SME, GPU, and ANE stay named-but-offline engines.
