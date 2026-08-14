## 1. Freestanding QEMU boot

- [ ] 1.1 Add a `no_std` microkernel crate, Limine (or equivalent) config, and a documented `build` command that emits an x86_64 image
- [ ] 1.2 Add a documented `qemu` command that boots that image with no guest Linux/macOS
- [ ] 1.3 Reach a halt-safe ready banner that names Tacit, with one initial Realm
- [ ] 1.4 On missing memory map or display init failure, halt with a distinct diagnostic instead of a silent hang

## 2. Machine: console, memory, keyboard, events

- [ ] 2.1 Bring up a visible text console (framebuffer glyphs or VGA text) as a shaped region, not a `void*`
- [ ] 2.2 Allocate and free host-placement regions that do not overlap image, stack, or display; fail oversized allocs cleanly
- [ ] 2.3 Capture PS/2 (or QEMU default) IRQs into an event array; ignore unmapped keys without crashing
- [ ] 2.4 Deliver printable keys and Enter as a character array to a granted reader; echo unless silent

## 3. Microkernel ABI, capabilities, Realm

- [ ] 3.1 Implement the first-milestone op set (alloc/map, grant/revoke, display send, keyboard wait, clock) as an operation-array ABI
- [ ] 3.2 Create the initial Realm with only documented starter caps; a write without a display cap must fail and leave the console unchanged
- [ ] 3.3 Enforce a Realm memory quota; exceeding it fails without corrupting kernel arenas
- [ ] 3.4 Reclaim revoked or dead-Realm objects with explicit lifetimes — no kernel GC

## 4. Host compiler: Uiua subset → UIR

- [ ] 4.1 Document the first-milestone subset and lower it to UIR with shape, purity, edges, regions, and required caps
- [ ] 4.2 Accept a bundled program that is at least `C = (A + B) × D` plus a display write; embed UIR that still shows Add, Multiply, and the edge between them
- [ ] 4.3 Reject one out-of-subset construct with source location, and do not emit an image that contains it
- [ ] 4.4 Record independent axes as parallel dimensions even if the guest still runs them in order

## 5. Guest stepper and policy

- [ ] 5.1 Step UIR on the boot CPU using region views (offset/shape/strides) and unique-vs-immutable rules
- [ ] 5.2 After the ready banner, run the bundled tiny program with no extra operator action and show its result
- [ ] 5.3 On a defined runtime error, show the error and keep the Realm halted or idle — no reset loop
- [ ] 5.4 Make scheduler/grant policy Uiua compiled to UIR; inverting the documented order key changes run order without a microkernel edit
- [ ] 5.5 Bind keyboard as an event-array source so a read-line program echoes `hi` after the operator types it

## 6. Live graph and five primitives

- [ ] 6.1 Document the five primitives and a one-page map from Unix nouns to derived forms or "refused"
- [ ] 6.2 Keep the loaded UIR as the source of truth for running work (no process table)
- [ ] 6.3 Expose a query: nodes, edges, shapes, caps, ready set, as arrays
- [ ] 6.4 Print a console projection of the tiny program's graph (Add, Multiply, display)
- [ ] 6.5 Answer provenance for C: producer Multiply, inputs Add-result and D

## 7. Effects and one agent-shaped transform

- [ ] 7.1 Classify nodes as pure or effectful with required caps
- [ ] 7.2 Simulate a display write without changing the console; commit after validate
- [ ] 7.3 Missing display cap leaves the console unchanged; mark whether previous console contents can be restored
- [ ] 7.4 Provide machine tables for transforms and capabilities as arrays
- [ ] 7.5 Run one granted transform that filters or summarizes the live graph and proposes a display write, with only the caps it needs
- [ ] 7.6 Represent two independent summaries as fan-out nodes, even if both still run on the boot CPU

## 8. Fusion (first speed number)

- [ ] 8.1 Add in-image counters: payload bytes moved, payload bytes copied, kernel entries
- [ ] 8.2 Implement a host fusion pass for single-consumer pure elementwise chains
- [ ] 8.3 Fuse Add-then-Multiply so T = A+B is not allocated; keep a documented unfused mode
- [ ] 8.4 Refuse to fuse across display/keyboard effects and across undocumented fan-out
- [ ] 8.5 Ship `bench-fusion`: same C, fused bytes < unfused bytes, both printed

## 9. Zero-copy send, datapath, placement

- [ ] 9.1 Make immutable same-placement send a region-capability share or move; keep unique-region in-place mutation
- [ ] 9.2 Keep an explicit copy transform as the bench control; ship `bench-send` on a documented large array
- [ ] 9.3 Run the fused kernel as one entry (or documented tiles), not one trap per element
- [ ] 9.4 Deliver keyboard as one array per line, not one syscall per key
- [ ] 9.5 Record `place = host` on fused nodes; do not add a CUDA-style API
- [ ] 9.6 Confirm the image has no POSIX file path, no listen/accept, and only the devices that milestone needs
