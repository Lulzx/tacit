## Purpose

Defines the freestanding Uiua subset compiler and UIR: the compiler and the scheduler share one semantic model of shape, purity, dependence, regions, capabilities, memory home, and engine.

## ADDED Requirements

### Requirement: UIR is the shared model
Uiua subset source MUST lower to UIR, a graph IR that records shape, purity, dependencies, memory regions, capabilities, parallel dimensions, memory home, engine constraints, and cache-domain hints. The scheduler MUST consume UIR, not a separate thread description.

#### Scenario: Compiler emits a graph
- **GIVEN** valid subset source with two independent pure ops
- **WHEN** the host compiler runs
- **THEN** the UIR contains two independent nodes
- **AND** both are marked ready once their inputs exist

#### Scenario: Official hosted runtime is not used
- **GIVEN** a built guest image
- **WHEN** the image runs under QEMU `aarch64` virt
- **THEN** it does not invoke the official hosted Uiua interpreter
- **AND** it does not require Linux or macOS system APIs that interpreter assumes

### Requirement: First-milestone language subset
The first milestone MUST compile numeric scalars, rank-1 and rank-2 numeric arrays, character arrays, stack or combinator reordering sufficient for the tiny program, elementwise arithmetic, reduce, reshape, rank-wise map, display write, and keyboard read. Features outside the subset MUST be rejected at compile time with a location and a reason.

#### Scenario: Accepted tiny program
- **GIVEN** the documented tiny program, which computes `C = (A + B) × D` (or equivalent) and writes C to the display
- **WHEN** the image is built and booted
- **THEN** the program is accepted
- **AND** its result appears on the display after the ready banner
- **AND** loaded UIR still has an Add node, a Multiply node, and a dependence edge from Add to Multiply

#### Scenario: Rejected construct
- **GIVEN** source that uses files, sockets, threads, Metal, CUDA, or an unimplemented primitive
- **WHEN** the compiler processes that source
- **THEN** compilation fails
- **AND** the diagnostic names the construct and the source location
- **AND** no boot image that contains that program is produced

### Requirement: Host compile, guest execute
Uiua source MUST be compiled on a development host into a UIR payload the guest can step or execute. The documented host is an Apple Silicon Mac. Compile errors MUST stay on the host.

#### Scenario: Image contains the payload
- **GIVEN** valid subset source
- **WHEN** the documented build command succeeds
- **THEN** the boot image contains the compiled UIR
- **AND** QEMU execution does not invoke the official hosted interpreter

#### Scenario: Compile error stays on the host
- **GIVEN** invalid subset source
- **WHEN** the build command runs
- **THEN** the build fails on the host
- **AND** QEMU is not required to report that compile error

### Requirement: Effect graph is visible
The compiler MUST classify each transform as pure or effectful. Effectful transforms MUST list the capability classes they need. Independent pure transforms MUST be reorderable.

#### Scenario: Display is effectful
- **GIVEN** a display-write transform
- **WHEN** its UIR node is inspected
- **THEN** it is marked effectful
- **AND** it names a display capability

#### Scenario: Arithmetic is pure
- **GIVEN** an elementwise add of two numeric arrays
- **WHEN** its UIR node is inspected
- **THEN** it is marked pure
- **AND** it may run in either order relative to another independent pure node

### Requirement: Named ops survive until fusion and placement
UIR MUST retain operator identity (at least elementwise arithmetic, reduce, reshape, rank-wise map) and shapes until after a documented fusion-and-placement stage. The first milestone MUST keep those names visible after load even if it does not fuse. A later change MAY fuse adjacent pure elementwise nodes into one kernel, and MAY lower a named matmul onto SME. The system MUST NOT lower the graph to an opaque instruction blob before that stage. The system MUST NOT assume SVE; vector lowering is NEON or SME.

#### Scenario: Add-multiply is still two named nodes after load
- **GIVEN** `C = (A + B) × D`
- **WHEN** UIR is loaded in the guest
- **THEN** Add and Multiply are distinct named nodes with a dependence edge
- **AND** a later fusion pass is allowed to replace them with one node
- **AND** the first milestone may still step them separately

### Requirement: Shape carries parallelism
When a transform is independent across an axis, UIR MUST record that axis as a parallel dimension so a later placer can split work across P-cores, NEON lanes, or tiles without recovering threads from machine code.

#### Scenario: Rows are independent
- **GIVEN** a rank-wise map over 16 rows
- **WHEN** the UIR is inspected
- **THEN** it records 16 independent units along that axis
- **AND** the first milestone may still run them sequentially on the boot CPU

### Requirement: Home and engine are recorded
Compiled UIR MUST record `home = uma` on first-milestone arrays and MUST record an engine on each runnable node. The first-milestone default engine MUST be `p-core`. A later pass MAY change the engine without changing the Uiua source.

#### Scenario: Tiny program nodes name uma and p-core
- **GIVEN** the bundled tiny program after load
- **WHEN** Add and Multiply are inspected
- **THEN** their regions have `home = uma`
- **AND** their engine is `p-core` or an equivalent boot-CPU default

### Requirement: Tiny program is the first-milestone demo
The built image MUST include one bundled tiny Uiua program that runs automatically after ready, uses at least one array transform, and writes an observable result.

#### Scenario: Automatic run after ready
- **GIVEN** a successful boot
- **WHEN** the ready state is reached
- **THEN** the bundled tiny program starts without further operator action
- **AND** its display output is distinguishable from the ready banner

#### Scenario: Program error does not reboot
- **GIVEN** a tiny program that hits a defined runtime error
- **WHEN** that error occurs
- **THEN** the display shows a runtime error
- **AND** the Realm stays halted or idle
- **AND** the machine does not reset in a loop
