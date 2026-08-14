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
