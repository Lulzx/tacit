## Purpose

Defines what Tacit believes computation is: a graph of array transformations the system can still see, running on Apple Silicon unified memory and engines, not opaque processes the kernel virtualizes.

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
- **AND** the source does not create threads, tasks, GPU kernels, or Metal command buffers to express that parallelism

### Requirement: First machine is Apple Silicon
The first machine the system is specified against MUST be Apple Silicon with unified memory and a documented engine set (P-cores, E-cores, NEON, SME, GPU, ANE, media, display). The first demonstration environment MUST be QEMU `aarch64` virt. The system MUST NOT treat a discrete GPU memory space as required to explain that machine.

#### Scenario: Research test names engines
- **GIVEN** the documented research test
- **WHEN** an operator reads what placement means
- **THEN** placement is across engines over unified memory
- **AND** it is not a CUDA-style host-to-device copy plus launch

### Requirement: Specialized image, not a general kernel
The first-milestone image MUST be a specialized payload: only the microkernel mechanism, the UIR stepper, the initial Realm, the machine description, and the devices that milestone needs. It MUST NOT include a general-purpose filesystem, TCP stack, process table, or unused device class.

#### Scenario: Unused subsystems are absent
- **GIVEN** a built first-milestone image
- **WHEN** the image is inspected
- **THEN** there is no POSIX file API, no listen/accept TCP path, and no process table
- **AND** the image still boots and runs the tiny program

### Requirement: OS state is arrays
Ready work, event batches, capability tables, region maps, the machine description, and later object directories MUST be representable as arrays that Uiua transforms can consume. The system MUST NOT require a lock-protected kernel tree as the only view of that state.

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
The system MUST obey the seven laws in `proposal.md`. An abstraction MUST NOT enter the microkernel solely because Unix has it. A host-versus-device memory split MUST NOT enter the core solely because discrete GPUs have it.

#### Scenario: Proposed POSIX object is rejected
- **GIVEN** a change that would add a kernel file, `ioctl`, or thread control block as a core object
- **WHEN** that change is reviewed against this spec
- **THEN** it is out of contract
- **AND** the equivalent must be expressed as values, transforms, capabilities, or placement, or rejected

#### Scenario: Proposed host/device split is rejected as the core
- **GIVEN** a change that would make CPU RAM versus GPU VRAM the only placement model
- **WHEN** that change is reviewed against this spec
- **THEN** it is out of contract for Apple Silicon
- **AND** the equivalent must name an engine and a memory home

### Requirement: Freestanding execution
The running system MUST execute with no Linux, macOS, Windows, or other general-purpose host OS underneath it. The only required demonstration environment is QEMU `aarch64` virt. The Apple Silicon Mac is the development host, not the guest kernel.

#### Scenario: QEMU boot has no host OS guest
- **GIVEN** a built system image
- **WHEN** the image is started under QEMU using the project's documented command
- **THEN** the guest does not boot a Linux, macOS, or Windows kernel
- **AND** the first user-visible output is produced by Tacit

#### Scenario: Hosted interpreter is not the runtime
- **GIVEN** the official hosted Uiua interpreter
- **WHEN** an operator looks for the supported way to run the OS
- **THEN** the supported path is the freestanding QEMU `aarch64` image
- **AND** embedding that hosted runtime in the guest kernel is not supported

#### Scenario: macOS process is not the OS
- **GIVEN** a proposed runtime that is a macOS process calling Metal, CoreML, or Accelerate as the guest compute path
- **WHEN** it is reviewed as the supported way to run Tacit
- **THEN** it is out of contract for the guest
- **AND** a later measurement scaffold on the host MAY exist only if it is documented as a lab, not as the OS

### Requirement: Mechanism versus policy
The microkernel MUST provide only trusted mechanism. Kernel policy, services, and programs MUST be Uiua compiled to UIR.

#### Scenario: Policy is Uiua source
- **GIVEN** the source tree after the first milestone
- **WHEN** an operator inspects ready-set order and resource grants
- **THEN** those policies are Uiua sources compiled to UIR
- **AND** changing them does not require editing the AArch64 exception stub
