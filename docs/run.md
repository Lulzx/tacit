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
guest as an event array.

After the ready banner the guest, unattended:

1. publishes the M4 Pro machine description (`home = uma`, 16 KiB pages,
   128-byte lines, the boot CPU online with its NEON unit wired; e-core, SME,
   GPU, ANE, media, display named but offline),
2. runs the bundled program `C = (A + B) × D`, printing its live graph
   (Add, Multiply, the edge, caps, parallel axes) and the provenance of `C`
   plus per-node payload-byte counters,
3. runs three granted agent-shaped transforms over the live graph, ordered
   by a Uiua multi-agent planner (a priority key, highest first; the plan is
   `⊏ ⍖ ⊡ 1`, select on grade-down of the picked priority column),
4. exercises the subset (reduce, reshape, grade/select/keep, rank-wise map,
   the capabilities table),
5. demonstrates effects (propose → simulate → validate → commit; a missing,
   forged, or bit-flipped display cap leaves the console unchanged) and the
   operation-array ABI (batching with dependencies, unmet dependencies
   refused),
6. exercises the content-addressed object store (`id = H(data)`: storing the
   same value twice deduplicates to the same id; loading returns it),
7. demonstrates deterministic replay: clock reads are recorded, and
   `&replay-clock` returns the exact recorded values in order,
8. prints the fusion bench (fused bytes < unfused bytes), the zero-copy
   send bench (capability share vs explicit copy, unique-region in-place
   mutation vs immutable refusal), and the SME matmul bench (C00 = 192 and
   the per-engine entries for `&matmul`),
9. verifies the **self-hosted compiler**: the guest re-compiles every
   bundled Uiua source (embedded as text) and must produce byte-identical
   UIR to the host-compiled payloads it runs,
10. runs a scripted **store + shell** session (mkdir, echo, history, undo)
    and then drops into the interactive Tacit shell.

Pure elementwise nodes are placed on the **NEON engine**: the stepper
dispatches `engine = neon` to a 128-bit Advanced-SIMD kernel, and the fused
Add-then-Multiply runs as one NEON entry. `&stats` reports a per-engine
breakdown (e.g. `kernel entries: 1 (neon 1)` for the fused bench). This is
speed-stack level 6 — engines as placements of the same node.

`&matmul` is placed on the **SME engine**.  SME presence is probed from
`ID_AA64PFR1_EL1` (QEMU keeps the ID registers consistent with what it
exposes): on this Mac both HVF and TCG `-cpu max` report it online, and the
matmul bench shows `kernel entries: 1 (sme 1)`.  The first slice enters and
leaves streaming mode (`smstart`/`smstop`); the ZA-accumulating tile kernel
is the next slice.  GPU and ANE stay named-but-offline engines.

## Capability tokens are PAC-signed

Capability tokens are the `pacga` MAC of a kernel-generated nonce under a
kernel-only GA key (FEAT_PACGA): arithmetic cannot mint one, and a single
flipped bit fails the recomputed MAC.  Presence is probed from
`ID_AA64ISAR1_EL1.GPI` — on this Mac both HVF and TCG `-cpu max` report it,
and the kernel self-test prints `capability tokens: PACGA-signed (pacga)`.
When FEAT_PACGA is absent the kernel falls back to software unforgeability
(random table tokens), so the capability spec does not change.

## Store and Tacit shell

After the benches the guest inits a **content-addressed namespace** (blobs
and trees, names as refs) and runs a scripted session:

```text
tacit> pwd
/
tacit> mkdir home
tacit> cd home
tacit> echo "hello tacit" > hello.txt
tacit> ls
hello.txt
tacit> cat hello.txt
hello tacit
tacit> history hello.txt
1  create  ........
tacit> echo "hello world" > hello.txt
tacit> undo hello.txt
tacit> cat hello.txt
hello tacit
```

Data is immutable. A write is a new value plus an atomic name update, so
history and undo are Ref operations, not sector rewrites. There is no
`open`, `fd`, or `seek`.

Then the guest drops into an interactive **Tacit shell** (`tacit>`). Familiar
bash commands are sugar for transforms over the current tree capability
(`pwd`, `ls`, `cd`, `cat`, `echo`, `mkdir`, `cp`, `ln`, `mv`, `rm`,
`touch`, `head`, `tail`, `wc`, `find`, `tree`, `test`, …). `type ls` prints
the Tacit meaning. Process-only commands (`ps`, `kill`, `sudo`) are refused. A line that is not a store command is
compiled to UIR with the *same* compiler source as the host
(`crates/compile`) and stepped on the boot CPU. Bindings are values —
`A ← [1 2 3]` snapshots the result, so later lines can reference `A` as a
constant.

`graph` prints the last command as a value-transform chain (not a process
pipeline). A `|` line is that graph, not a byte pipe:

```text
tacit> echo "1 2 3 4" > numbers
tacit> cat numbers | parse | square | sum
30
tacit> graph
numbers
  |
 Parse
  |
 Square
  |
 Sum
```

`why name` walks how that name was produced. `explain name` adds type,
hash, inputs, and the tree capability used.

The same shell runs in the browser at `/console` (`python3 wasm/serve.py`).

Before the shell, the boot runs the **self-hosted compiler check**: the
guest re-compiles every bundled Uiua source (embedded as text) and requires
byte-identical UIR to the host-compiled payload it already runs.  Same
sources, new host.

```text
tacit> × 2 3
[6]
tacit> A ← [1 2 3]
tacit> × 2 A
[2 4 6]
tacit> /+ A
[6]
tacit> ⧻ A
[3]
```

Glyphs (`×`, `↯`, `←`, `¯`, `⍏`, …) arrive over the serial line as UTF-8 and
are decoded by the reader; any out-of-subset construct is rejected with a
compile error and a source location, and a runtime error leaves the shell
running (no reset loop).
