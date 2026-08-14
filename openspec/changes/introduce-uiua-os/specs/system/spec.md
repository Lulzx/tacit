## Purpose

Defines what Tacit believes computation is: a graph of array transformations the system can still see, not opaque processes the kernel virtualizes.

## ADDED Requirements

### Requirement: Tacit equation
The system MUST implement `OS = values + transformations + capabilities + placement`. It MUST NOT implement `OS = processes + threads + files + syscalls` as its programming model.

#### Scenario: Named worldview
- **GIVEN** project documentation and the first-milestone image
- **WHEN** an operator asks what the OS is
- **THEN** the documented name is Tacit
- **AND** the documented worldview is that everything is an array transformation

#### Scenario: Unix objects are absent
- **GIVEN** a program written for the first-milestone language and kernel
- **WHEN** the program is compiled
- **THEN** there is no supported interface for `fork`, POSIX syscalls, file descriptors, or thread create
- **AND** work is expressed as transforms over arrays with capabilities

### Requirement: The OS participates in the computation
The system MUST retain a semantic graph of the program after compile. For a program equivalent to `C = (A + B) × D`, the running system MUST still know that Add is elementwise and pure, that its outputs feed Multiply, and that independent elements may run in parallel. It MUST NOT reduce that program to an opaque instruction stream plus a thread before the scheduler sees it.

#### Scenario: Add-then-multiply remains a graph
- **GIVEN** subset source that adds two same-shaped arrays and multiplies the result by a third
- **WHEN** the program is compiled to UIR and loaded
- **THEN** the graph has an Add node whose inputs are A and B
- **AND** a Multiply node that depends on Add and D
- **AND** Add is marked pure and elementwise
- **AND** the scheduler's unit of work is those nodes, not a thread control block

#### Scenario: Parallelism is derived, not spawned
- **GIVEN** an elementwise transform over a documented shape with many independent elements
- **WHEN** UIR is produced
- **THEN** the independent axis is recorded
- **AND** the source does not create threads, tasks, or GPU kernels to express that parallelism

### Requirement: Specialized image, not a general kernel
The first-milestone image MUST be a specialized payload: only the microkernel mechanism, the UIR stepper, the initial Realm, and the devices that milestone needs. It MUST NOT include a general-purpose filesystem, TCP stack, process table, or unused device class.

#### Scenario: Unused subsystems are absent
- **GIVEN** a built first-milestone image
- **WHEN** the image is inspected
- **THEN** there is no POSIX file API, no listen/accept TCP path, and no process table
- **AND** the image still boots and runs the tiny program

### Requirement: OS state is arrays
Ready work, event batches, capability tables, region maps, and later object directories MUST be representable as arrays that Uiua transforms can consume. The system MUST NOT require a lock-protected kernel tree as the only view of that state.

#### Scenario: Ready set is an array
- **GIVEN** a loaded graph with a mix of ready and blocked nodes
- **WHEN** policy runs
- **THEN** its input is an array of ready items
- **AND** it does not walk a Linux-style runqueue as the programming model

### Requirement: Research success is not Unix compatibility
The project MUST treat the research test as success, and MUST NOT treat “runs Linux binaries” or “implements POSIX” as success. The first milestone MUST demonstrate the graph for a tiny program. A later milestone MUST demonstrate the full research test on a nontrivial program.

#### Scenario: First milestone shows the graph
- **GIVEN** the first-milestone image and its bundled program
- **WHEN** an operator inspects the loaded UIR
- **THEN** at least one array transform and its display effect are visible as distinct nodes
- **AND** no Unix process table is required for that inspection

#### Scenario: Unix compat is not a goal
- **GIVEN** a proposed change whose acceptance is “it runs a Linux userspace”
- **WHEN** that change is reviewed against this spec
- **THEN** it is out of contract unless it also preserves the transformation-graph model

### Requirement: Seven laws
The system MUST obey the seven laws in `proposal.md`. An abstraction MUST NOT enter the microkernel solely because Unix has it.

#### Scenario: Proposed POSIX object is rejected
- **GIVEN** a change that would add a kernel file, `ioctl`, or thread control block as a core object
- **WHEN** that change is reviewed against this spec
- **THEN** it is out of contract
- **AND** the equivalent must be expressed as values, transforms, capabilities, or placement, or rejected

### Requirement: Freestanding execution
The running system MUST execute with no Linux, macOS, Windows, or other general-purpose host OS underneath it. The only required demonstration environment is QEMU.

#### Scenario: QEMU boot has no host OS guest
- **GIVEN** a built system image
- **WHEN** the image is started under QEMU using the project's documented command
- **THEN** the guest does not boot a Linux, macOS, or Windows kernel
- **AND** the first user-visible output is produced by Tacit

#### Scenario: Hosted interpreter is not the runtime
- **GIVEN** the official hosted Uiua interpreter
- **WHEN** an operator looks for the supported way to run the OS
- **THEN** the supported path is the freestanding QEMU image
- **AND** embedding that hosted runtime in the guest kernel is not supported

### Requirement: Mechanism versus policy
The microkernel MUST provide only trusted mechanism. Kernel policy, services, and programs MUST be Uiua compiled to UIR.

#### Scenario: Policy is Uiua source
- **GIVEN** the source tree after the first milestone
- **WHEN** an operator inspects ready-set order and resource grants
- **THEN** those rules exist as Uiua source
- **AND** they are not the only copy of the rules, hardcoded in the microkernel

### Requirement: Determinism is default
Given the same input values and the same capability responses, a Realm MUST produce the same outputs regardless of core count or scheduling order of independent transforms.

#### Scenario: Independent transforms commute
- **GIVEN** two independent pure transforms in one Realm
- **WHEN** they run in either order
- **THEN** the Realm's outputs are identical

#### Scenario: Nondeterminism is explicit data
- **GIVEN** keyboard, clock, or random
- **WHEN** a transform depends on them
- **THEN** those sources appear as capability-backed inputs
- **AND** they are not implicit global side channels

### Requirement: Self-hosting is not blocked
The architecture MUST allow a later system in which a Uiua compiler written in Uiua runs on Tacit and the scheduler remains a Uiua transform. The first milestone MUST NOT require that compiler.

#### Scenario: First milestone uses a host compiler
- **GIVEN** the first-milestone build
- **WHEN** Uiua source is compiled
- **THEN** compilation may run on the development host
- **AND** the guest executes only the compiled UIR payload
