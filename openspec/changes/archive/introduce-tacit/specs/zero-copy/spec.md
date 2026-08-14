## Purpose

Defines default communication as handing off an immutable unified-memory region capability instead of copying bytes. On Apple Silicon that is also the hardware default.

## ADDED Requirements

### Requirement: Immutable send is a capability
Sending an immutable array from one transform or Realm to another MUST transfer or share a region capability. The payload bytes MUST NOT be copied unless a documented fallback applies (unique mutation needed, incompatible home, or an explicit copy op). Same-home sends on `uma` MUST NOT copy.

#### Scenario: Same-home send does not copy
- **GIVEN** an immutable `uma` region of documented large size in Realm A
- **WHEN** A sends it to Realm B (or to another transform in the same Realm) with the same home
- **THEN** B receives a capability to that region
- **AND** a traffic counter does not record a payload-sized copy

#### Scenario: Unique owner may mutate without a send-copy
- **GIVEN** a uniquely owned region
- **WHEN** its owner applies an in-place update
- **THEN** no second payload-sized region is required
- **AND** no other Realm holds a live view of the old bytes

### Requirement: Copy is explicit and counted
When a copy is required, it MUST appear as an explicit copy transform or a documented fallback. The system MUST count payload bytes copied on the fusion and cap-send benches. Changing engine on the same `uma` home MUST NOT by itself force a payload copy.

#### Scenario: Forced copy is visible
- **GIVEN** a send that the implementation must copy (documented fallback)
- **WHEN** the send completes
- **THEN** the bench log records a copy of the payload size
- **AND** the destination holds the same values

#### Scenario: Engine retarget is not a copy
- **GIVEN** an immutable `uma` region consumed by a later `engine = sme` or `engine = gpu` node
- **WHEN** that node becomes ready on a machine that can dispatch it
- **THEN** the send of that region is still a capability share
- **AND** a payload copy is recorded only if a documented fallback applies

### Requirement: Cap send is measured against memcpy
The project MUST ship a bench that sends a documented large array by capability and by payload copy, and reports both times and byte counts.

#### Scenario: Cap path is cheaper on large arrays
- **GIVEN** the documented size (large enough that memcpy is visible)
- **WHEN** both paths run
- **THEN** the capability path reports fewer payload bytes copied
- **AND** both destinations compare equal
