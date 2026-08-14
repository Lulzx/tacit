## Purpose

Keeps the microkernel off the data path: events and operations stay batched arrays; no per-element syscall and no in-kernel TCP or file stack.

## ADDED Requirements

### Requirement: Batched crossing
A hot path that processes N independent array elements or N events MUST NOT perform N kernel crossings. Submission is an operation array or an event array.

#### Scenario: Keyboard line is one batch
- **GIVEN** a line of printable keys ending in Enter
- **WHEN** the reader transform runs
- **THEN** it consumes one event array or one character array
- **AND** it does not issue one syscall per key

#### Scenario: Fused kernel does not trap per element
- **GIVEN** a fused Add-Multiply kernel over a large shape
- **WHEN** it runs
- **THEN** there is one (or a documented tile count of) kernel entry
- **AND** not one kernel entry per element

### Requirement: No kernel TCP or file datapath
The speed-stack image MUST NOT add an in-kernel TCP stack or POSIX file datapath. If I/O beyond console and keyboard is added later, it MUST be a Realm or libOS over descriptor-ring arrays.

#### Scenario: Image still has no listen/accept
- **GIVEN** a built speed-stack image
- **WHEN** it is inspected
- **THEN** there is no listen/accept TCP path and no POSIX file read path in the microkernel
- **AND** fusion and cap-send benches still run

### Requirement: Policy stays off the inner loop
Per-element stepping MUST NOT recompute global placement or scheduling scores. Global policy runs on the ready array, then the micro-stepper runs nodes.

#### Scenario: Fusion inner loop is policy-free
- **GIVEN** a fused elementwise kernel
- **WHEN** it iterates elements
- **THEN** it does not call the global placer per element
