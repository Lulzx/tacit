## Purpose

Makes the running machine a live computation graph. Process lists, traces, and resource views are projections of that graph, not separate tools.

## ADDED Requirements

### Requirement: One graph is the machine
The running system MUST maintain a live graph of transformations, values on edges, required capabilities, and resource use. There MUST NOT be a separate process table that is the source of truth.

#### Scenario: Tiny program is a graph
- **GIVEN** the bundled `C = (A + B) × D` program after ready
- **WHEN** the graph is queried
- **THEN** Add, Multiply, and the display effect appear as nodes
- **AND** each edge names the value's shape
- **AND** no process-id table is required to answer that query

### Requirement: Inspection is projection
Views that would be `ps`, `top`, `strace`, `lsof`, or `/proc` MUST be queries or projections over the live graph. Adding a new view MUST NOT require a new kernel subsystem.

#### Scenario: Ready work is a query
- **GIVEN** a loaded graph with ready and blocked nodes
- **WHEN** a projection asks for ready transformations
- **THEN** the result is an array of those nodes
- **AND** it is not produced by walking a Unix runqueue

#### Scenario: Resource use is on the node
- **GIVEN** a running transform
- **WHEN** it is inspected
- **THEN** the view includes input shape, output shape, capabilities held, and a documented resource counter
- **AND** that data comes from the graph record

### Requirement: Provenance is native
For any live value, the system MUST answer which transform produced it and from which inputs. Provenance MUST NOT be an optional log scraped after the fact.

#### Scenario: Click the product
- **GIVEN** the array C from Add-then-Multiply
- **WHEN** provenance is requested
- **THEN** the producer is Multiply
- **AND** Multiply's inputs are the Add result and D

### Requirement: Visual language is the graph
If a display shows pipelines, nodes, or "apps," those objects MUST be the same graph the scheduler executes. Dragging a value onto a transform MUST construct a composition, not launch a Unix process with a file path.

#### Scenario: First milestone has a text projection
- **GIVEN** the first QEMU image
- **WHEN** the operator requests the graph view
- **THEN** a text projection of nodes and edges is shown on the console
- **AND** a later graphical projection may replace the renderer without changing the graph

#### Scenario: Later drag-compose uses the same object
- **GIVEN** a future graphical shell
- **WHEN** a value is composed onto a transform in the UI
- **THEN** the scheduler's graph gains that edge
- **AND** no hidden `exec` of a separate binary is required for that composition
