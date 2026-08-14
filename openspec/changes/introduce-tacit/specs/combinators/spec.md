## Purpose

Defines Tacit's native vocabulary: values, transformations, composition, capabilities, and evaluation. Nothing else is fundamental.

## ADDED Requirements

### Requirement: Five primitives
The system MUST treat values, transformations, composition, capabilities, and evaluation as the only core objects. A proposed file, process, pipe, thread, or file-descriptor API MUST be expressed as those five or rejected.

#### Scenario: Independent work is composition, not threads
- **GIVEN** a program that applies f, g, and h to the same value and then combines the results
- **WHEN** it is loaded as UIR
- **THEN** f, g, and h are independent transformations
- **AND** there is no thread-create interface used to express that independence

#### Scenario: Storage is a value, not open/read/seek/write/close
- **GIVEN** a later stored object (not required on the first QEMU image)
- **WHEN** a program uses it
- **THEN** it appears as a value or region plus a capability
- **AND** the program does not call a POSIX open/read/seek/write/close sequence

#### Scenario: Unix leftover is refused
- **GIVEN** a change that adds ELF process loading or a POSIX pipe as a core object
- **WHEN** it is reviewed against this spec
- **THEN** it is out of contract unless it is derived from the five primitives without becoming a second worldview

### Requirement: Program representation is the schedule
A loaded program MUST expose data dependence as the schedule. The kernel MUST NOT recover dependence by watching instruction streams.

#### Scenario: Fan-out is visible
- **GIVEN** one value feeding three independent transforms
- **WHEN** the graph is inspected
- **THEN** the three nodes are ready together
- **AND** the combine node waits on all three
