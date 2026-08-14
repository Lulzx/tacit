## Purpose

Treats agents as transformations over structured state, not processes that click or parse Unix text. Authority and parallelism follow the graph. Independent branches are placeable on engines.

## ADDED Requirements

### Requirement: Machine state is arrays
Observable machine state MUST be available as arrays or tables (at least: transformations, capabilities, the machine description, and later files, network, and agents). An observer MUST query those arrays rather than parse process-list text.

#### Scenario: Filter hot work
- **GIVEN** running transforms with resource counters
- **WHEN** a query keeps rows above a documented CPU or byte threshold
- **THEN** the result is an array of those transforms
- **AND** the query does not invoke `ps` or parse whitespace columns

### Requirement: Small action algebra
Agent-visible actions MUST be compositions of documented transforms (select, filter, group, aggregate, grant, archive, display, and others added by later changes). The system MUST NOT require an agent to emit shell, Python, or mouse events as the native interface.

#### Scenario: One composition, not five programs
- **GIVEN** a table of experiment rows with a status column
- **WHEN** an agent composes select → filter failed → group by type → aggregate rates
- **THEN** the result is one graph
- **AND** no shell pipeline of distinct executables is required

### Requirement: Agent is a transform
An agent MUST be representable as state plus transformation plus capabilities plus objective. Invoking it MUST be evaluation of that transform, not creation of a POSIX process. Parallel agents MUST be independent nodes.

#### Scenario: Two research branches are fan-out
- **GIVEN** a goal that decomposes into two independent research transforms
- **WHEN** the graph is loaded
- **THEN** both branches are ready together
- **AND** the synthesize node waits on both
- **AND** neither branch is a `clone`/`fork` process

#### Scenario: First milestone has one agent-shaped transform
- **GIVEN** the first QEMU `aarch64` image
- **WHEN** a documented agent-shaped transform queries the live graph and writes a summary to the display
- **THEN** it holds only the capabilities it was granted
- **AND** it does not receive an ambient shell

### Requirement: Authority is dataflow
An agent MUST perform an effect only if the required capability is an input to that node. Ambient "access to the computer" MUST NOT exist.

#### Scenario: No network cap, no network
- **GIVEN** an agent with only a display capability
- **WHEN** it proposes a network send
- **THEN** validate fails
- **AND** no packet is sent

### Requirement: Placement of agent work
Independent agent branches MUST be placeable like independent array rows. The first milestone MUST run them on the boot CPU (`engine = p-core`, `home = uma`). A later change MAY place them on another P-core, an E-core, SME, GPU, ANE, a sandbox, or another machine without the agent naming that hardware.

#### Scenario: First milestone runs branches on p-core
- **GIVEN** two independent agent nodes
- **WHEN** they run on the first image
- **THEN** both execute with `engine = p-core` and `home = uma`
- **AND** the graph still records them as independent transforms
