## Purpose

Defines unforgeable authorities and the split between values, which may be computed, and authorities, which may not.

## ADDED Requirements

### Requirement: Zero ambient authority
A Realm MUST own no device, network, disk, display, keyboard, or other Realm unless it holds a capability for it. Boot MUST grant the initial Realm only the capabilities documented for the first milestone.

#### Scenario: Fresh Realm is empty
- **GIVEN** a newly created Realm with no grants
- **WHEN** it attempts to write the display or read the keyboard
- **THEN** the attempt fails with a capability error
- **AND** the display and keyboard are unchanged

#### Scenario: Initial Realm has only listed caps
- **GIVEN** the first-milestone boot
- **WHEN** the initial Realm starts
- **THEN** it holds only the documented starter set (at least display, keyboard, and a memory budget)
- **AND** it does not hold a disk, network, or arbitrary other-Realm capability

### Requirement: Values versus authorities
Values (numbers, characters, arrays, boxes, functions) MUST be freely transformable. Authorities (regions, devices, channels, clocks, realms, execution resources) MUST be unforgeable kernel objects. Computation on values MUST NOT produce a new authority.

#### Scenario: Array arithmetic cannot mint a cap
- **GIVEN** numeric arrays and no display capability
- **WHEN** a transform computes any array from those values
- **THEN** the result is a value
- **AND** it is not accepted as a display capability

#### Scenario: Capability is not an integer fd
- **GIVEN** a display capability
- **WHEN** a Realm inspects it
- **THEN** forging the same authority from an integer or a byte array fails
- **AND** the object is not a POSIX file descriptor

### Requirement: Grant, revoke, and narrow
The kernel MUST grant and revoke capabilities. A capability MUST be narrowable to a weaker authority. A revoked capability MUST fail subsequent use.

#### Scenario: Revoke stops display writes
- **GIVEN** a Realm with a display capability
- **WHEN** that capability is revoked and the Realm writes the display
- **THEN** the write fails
- **AND** the console is unchanged by that write

#### Scenario: Narrowed read-only image
- **GIVEN** a later storage capability (not required in the first milestone)
- **WHEN** it is narrowed to a single read-only object
- **THEN** the holder can read that object
- **AND** it cannot write it or see sibling objects

### Requirement: Effects require capabilities
A transform that performs an effect MUST name the capability that authorizes it. Pure transforms MUST NOT require capabilities.

#### Scenario: Display write is effectful
- **GIVEN** a transform that writes pixels or text
- **WHEN** it is classified by the compiler or kernel
- **THEN** it is effectful
- **AND** it does not run without a display capability

#### Scenario: Resize is pure
- **GIVEN** a transform that only changes array shape or samples
- **WHEN** it is classified
- **THEN** it is pure
- **AND** it may be reordered with other independent pure work
