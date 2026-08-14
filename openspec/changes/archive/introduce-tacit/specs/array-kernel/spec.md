## Purpose

Defines the array-native kernel above the microkernel: transforms, dependency-derived scheduling, event batches, value channels, and a two-level placer over Apple Silicon engines.

## ADDED Requirements

### Requirement: Transform is the executable object
The kernel MUST represent work as a Transform `T : A → B` with inputs, outputs, shape, effects, dependencies, capabilities, memory home, engine, cache-domain hint, and priority. It MUST NOT expose a thread create, yield, or sleep interface as the programming model. CPU threads MAY exist only as an implementation detail of the stepper.

#### Scenario: Independent transforms are both ready
- **GIVEN** two transforms that do not share unresolved inputs
- **WHEN** the graph is loaded
- **THEN** both are in the ready set
- **AND** either may run without waiting for the other

#### Scenario: Dependent transform waits
- **GIVEN** transform B whose only input is the output of A
- **WHEN** A has not completed
- **THEN** B is not in the ready set
- **AND** after A completes, B becomes ready with A's output

#### Scenario: No thread primitive
- **GIVEN** a program author
- **WHEN** they express two independent computations
- **THEN** they write two independent transforms
- **AND** they have no supported API to spawn a thread

### Requirement: Readiness is empty-dependency
A transform MUST be runnable if and only if its unresolved dependency set is empty and its required capabilities are present.

#### Scenario: Ready set is exact
- **GIVEN** a graph with a mix of ready and blocked nodes
- **WHEN** the ready set is observed
- **THEN** it contains exactly the nodes with no unresolved inputs and satisfied grants
- **AND** it does not contain blocked nodes

### Requirement: Scheduler is a transform over a work array
The global policy that orders and classifies ready work MUST be a Uiua/UIR transform whose input is an array of ready items and whose output is an ordered or partitioned work array. Changing that source and rebuilding MUST change order without a microkernel edit.

#### Scenario: Policy inversion
- **GIVEN** two otherwise equal ready nodes and a policy that orders them by a documented key
- **WHEN** the policy source is inverted and the image is rebuilt
- **THEN** the nodes run in the inverted order
- **AND** the microkernel source does not have to change

### Requirement: Two-level scheduling
A per-core or per-stepper micro-scheduler MUST only take ready work and run it. Global placement policy MUST NOT sit on every step. The first milestone MAY use a single stepper as the micro-scheduler. A later placer MAY score engines by latency, movement, queue depth, and energy (P-core versus E-core).

#### Scenario: First milestone has a cheap stepper
- **GIVEN** the first-milestone image
- **WHEN** a ready node is selected
- **THEN** the stepper executes it on the boot CPU
- **AND** it does not recompute a global score for every array element inside that node

### Requirement: Events are batches
Hardware IRQs MUST appear to Uiua as an event array of timestamp, source, and payload. Policy MUST turn that array into work by select, group, partition, or reduce. The kernel MUST be allowed to coalesce interrupts into batches when latency permits.

#### Scenario: Key press is an event
- **GIVEN** the ready state
- **WHEN** a key is pressed
- **THEN** one event is appended to the event array
- **AND** a transform, not a program ISR, consumes it

#### Scenario: Empty events
- **GIVEN** no pending interrupts
- **WHEN** the event-to-work transform runs
- **THEN** it produces no new work
- **AND** the system stays ready

### Requirement: IPC is values or region capabilities
Communication MUST move values or transfer region capabilities. It MUST NOT be a POSIX byte-stream pipe. Immutable sends MUST be allowed to transfer a region capability without copying the payload.

#### Scenario: Value on an edge
- **GIVEN** producer A and consumer B connected by an edge
- **WHEN** A produces array X
- **THEN** B's corresponding input becomes X
- **AND** no pipe or socket is created

#### Scenario: Fan-out is immutable
- **GIVEN** one producer and two consumers of the same output
- **WHEN** the producer completes
- **THEN** both consumers become ready with that value
- **AND** neither can mutate the other's view

### Requirement: Devices are arrays
Display and keyboard MUST be resources that produce or consume arrays. Program source MUST NOT address video memory by integer pointer or poll I/O ports.

#### Scenario: Display write is an array
- **GIVEN** a node with a display grant
- **WHEN** it produces a character or glyph array
- **THEN** the machine updates the visible console from that array

#### Scenario: Keyboard is an array
- **GIVEN** unread key events and a granted reader
- **WHEN** the reader runs
- **THEN** it receives an array of those events or characters

### Requirement: Placement is an engine, not a thread id
A transform record MUST remain valid without a thread id or a vendor compute API. The first milestone MUST run nodes on the boot CPU (`engine = p-core`, `home = uma`). Later placers MAY target NEON, SME, GPU, or ANE using shape, home, and engine already in UIR.

#### Scenario: First milestone runs on the boot CPU
- **GIVEN** the first-milestone image
- **WHEN** a ready node is selected
- **THEN** it executes on the boot CPU
- **AND** the tiny program still runs
- **AND** the node record still has an engine field
