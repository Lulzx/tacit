## Purpose

Defines Realms: isolation, quotas, capability tables, and failure boundaries that are not POSIX processes.

## ADDED Requirements

### Requirement: Realm is the isolation boundary
A Realm MUST have a heap or region set, a set of transforms, a capability table, a resource quota, and a failure boundary. A Realm MUST NOT automatically receive stdin, stdout, a filesystem, a network stack, or an environment block.

#### Scenario: First milestone has one Realm
- **GIVEN** a successful boot
- **WHEN** the ready state is reached
- **THEN** exactly one initial Realm is running the bundled program
- **AND** that Realm's only external I/O is through granted capabilities

#### Scenario: No inherited Unix environment
- **GIVEN** a new Realm
- **WHEN** it is created
- **THEN** it has no file descriptors, working directory, or environment variables
- **AND** it cannot discover devices except through capabilities it is given

### Requirement: Failure is contained
A defined runtime fault inside a Realm MUST NOT reset the machine or corrupt another Realm's regions. The first milestone MAY halt that Realm in a diagnostic state if it is the only Realm.

#### Scenario: Rank error stays in the Realm
- **GIVEN** the tiny program hits a rank mismatch
- **WHEN** the error is raised
- **THEN** the display shows a runtime error
- **AND** the machine does not reset in a loop
- **AND** kernel objects of other, not-yet-created Realms are not required to exist

### Requirement: Quotas
A Realm MUST have a memory quota. Allocation that would exceed the quota MUST fail without taking memory from another Realm.

#### Scenario: Quota exceeded
- **GIVEN** a Realm whose remaining quota is smaller than a request
- **WHEN** it allocates
- **THEN** the allocation fails
- **AND** other Realms' live regions remain intact
