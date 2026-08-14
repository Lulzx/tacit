## Purpose

Defines the trusted mechanism layer: a tiny AArch64 kernel that knows memory, capabilities, address spaces, interrupt delivery, timers, execution, device mappings, and message transfer — and nothing else.

## ADDED Requirements

### Requirement: Tiny operation set
The microkernel MUST expose at most a documented set of about 10–15 fundamental operations. That set MUST be drawn from allocation and mapping, execution control, message transfer, capability grant and revoke, wait and signal, device mapping, and clock. The kernel MUST NOT implement a filesystem, TCP, a POSIX process model, `fork`, `ioctl`, Metal, or CUDA.

#### Scenario: First-milestone ops exist
- **GIVEN** the first-milestone image
- **WHEN** the initial Realm submits operations to allocate, grant a display or keyboard capability, write an array to the display, wait on keyboard, and read the clock
- **THEN** each of those operations is implemented
- **AND** there is no kernel path that opens a file or forks a process

#### Scenario: Forbidden kernel service
- **GIVEN** a proposed kernel feature that implements directories, sockets, or a POSIX file descriptor table
- **WHEN** it is evaluated against this spec
- **THEN** it is out of contract
- **AND** it must live in a Realm as Uiua, or be rejected

### Requirement: Operation-array ABI
User-to-kernel crossing MUST be submission of an operation array and receipt of a result array. A batch MAY contain multiple operations. The kernel MUST refuse to execute an operation whose documented dependencies are unmet, and MUST be allowed to batch, reorder, or fuse independent operations.

#### Scenario: Batch submit
- **GIVEN** an operation array that maps a region and writes bytes to the console, with no dependence between those two ops
- **WHEN** the Realm submits the array
- **THEN** both operations complete
- **AND** the result array reports each outcome
- **AND** the caller did not issue two C-style syscalls

#### Scenario: Dependent ops keep order
- **GIVEN** an operation array where a wait-on-keyboard feeds a later send-to-console
- **WHEN** the kernel executes the batch
- **THEN** the send does not run before the wait completes

#### Scenario: Missing capability fails the op
- **GIVEN** a send-to-console operation without a display capability
- **WHEN** the batch is submitted
- **THEN** that operation's result is a capability error
- **AND** the console is unchanged by that operation

### Requirement: AArch64 mechanism only
Architecture-specific code MUST be limited to exception-level bring-up, page tables, interrupt entry, atomics, and special registers. Scheduler policy, fusion, and placement MUST NOT live in that stub. The first-milestone stub MUST target QEMU `aarch64` virt, not Apple iBoot.

#### Scenario: Policy inversion does not touch the stub
- **GIVEN** the documented ready-set order key
- **WHEN** that key is inverted in Uiua source and the image is rebuilt
- **THEN** run order changes
- **AND** the AArch64 exception stub does not have to change

### Requirement: Kernel has no garbage collector
Kernel memory MUST use regions, slabs, or arenas with explicit lifetimes. The microkernel MUST NOT run a tracing garbage collector on its own structures.

#### Scenario: Kernel allocation is explicit
- **GIVEN** the kernel creates a capability or region object
- **WHEN** that object is revoked or the owning Realm dies
- **THEN** the kernel reclaims the object by an explicit lifetime rule
- **AND** it does not wait for a kernel GC cycle
