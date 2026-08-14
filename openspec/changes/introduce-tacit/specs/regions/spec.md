## Purpose

Defines shape-aware memory: arrays are views of regions with type, shape, stride, and placement, not bare pointer-plus-length.

## ADDED Requirements

### Requirement: Region has shape
A region MUST carry element type, shape, layout, and placement. A framebuffer MUST be representable as a rank-3 array of pixels, not only as an untyped byte span.

#### Scenario: Display region has image shape
- **GIVEN** the first-milestone console
- **WHEN** the display region is inspected
- **THEN** it has a documented height, width, and channel count
- **AND** a granted writer produces an array whose shape is compatible with that region

#### Scenario: Byte-only description is insufficient
- **GIVEN** a region allocated for a numeric matrix
- **WHEN** that region is described to the runtime
- **THEN** the description includes element type and shape
- **AND** it is not only a pointer and a byte length

### Requirement: Array is a view
An array MUST be a region reference plus offset, shape, and strides. Slice, reshape, reverse, and transpose MUST be allowed to change only that metadata when the underlying bytes can be shared.

#### Scenario: Transpose does not copy
- **GIVEN** a unique or immutable rank-2 region
- **WHEN** a transpose transform runs
- **THEN** the result may share the same region with swapped strides
- **AND** the original bytes are not required to be duplicated

### Requirement: Placement is abstract
A region MUST have a placement from a documented set that includes at least host memory. The first milestone MUST run on host (CPU) memory only. The model MUST allow later placements such as device, shared, persistent, or mapped I/O without changing how a transform names its arrays.

#### Scenario: First milestone is host placement
- **GIVEN** the first-milestone image
- **WHEN** the tiny program allocates an array
- **THEN** the array lives in host memory
- **AND** the program still runs

#### Scenario: Placement is not a raw RAM/VRAM enum in source
- **GIVEN** a compiled transform
- **WHEN** its inputs are inspected
- **THEN** they name regions
- **AND** they do not require the Uiua source to mention CPU RAM versus VRAM

### Requirement: Immutability is the default
A region MUST be immutable unless it is uniquely owned. Unique ownership MUST allow in-place update. Shared immutable regions MUST be safely mapped into more than one Realm. Mutation of a shared region MUST copy or produce a new region.

#### Scenario: Shared send does not copy by default
- **GIVEN** an immutable array in Realm A
- **WHEN** A sends it to Realm B
- **THEN** B may receive a capability to the same region
- **AND** neither Realm is required to memcpy the payload

#### Scenario: Unique region may mutate
- **GIVEN** a region with exactly one owner
- **WHEN** that owner applies an in-place update
- **THEN** the update is allowed
- **AND** no other Realm holds a live view of the old bytes
