## Purpose

The guest is self-hosting for its own subset: an interactive Uiua shell compiles each typed line to UIR in-guest with the same compiler source the host uses, and the boot verifies that the guest re-compiles every bundled Uiua source to byte-identical UIR.

## ADDED Requirements

### Requirement: One compiler, host and guest
The Uiua-subset compiler MUST live in a shared crate usable from the host compiler and from the freestanding guest.  The guest MUST be able to compile a line of the subset entirely in-guest.

#### Scenario: Shell line compiles and runs in-guest
- **GIVEN** the interactive shell prompt
- **WHEN** the operator types `× 2 3`
- **THEN** the shell prints `[6]`
- **AND** no host-side compilation occurred for that line

#### Scenario: Shell bindings persist as values
- **GIVEN** the shell
- **WHEN** the operator types `A ← [1 2 3]` and then `× 2 A`
- **THEN** the second line prints `[2 4 6]`

### Requirement: Self-hosted compilation is byte-identical
For every bundled Uiua program compiled without the host fusion pass, the guest MUST re-compile the embedded source and produce byte-identical UIR to the host-compiled payload it runs; a mismatch MUST be reported as a failure.

#### Scenario: Self-hosted check passes
- **GIVEN** the boot after the benches
- **WHEN** the self-hosted compiler check runs
- **THEN** every bundled source is reported byte-identical
- **AND** the check reports success only if all of them match
