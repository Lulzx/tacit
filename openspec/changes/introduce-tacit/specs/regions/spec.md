## Purpose

Defines shape-aware memory over Apple Silicon unified memory: arrays are views of regions with type, shape, stride, home, and cache domain, not bare pointer-plus-length.

## ADDED Requirements

### Requirement: Region has shape
A region MUST carry element type, shape, layout, memory home, and a cache domain. A framebuffer MUST be representable as a rank-3 array of pixels, not only as an untyped byte span.

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

### Requirement: Home is unified memory
On the Apple Silicon machine a region MUST have `home = uma`. The first milestone MUST allocate program arrays in that home. The model MUST allow a later non-unified machine to add homes such as `device` or `persistent` without changing how a transform names its arrays.

#### Scenario: First milestone arrays live in uma
- **GIVEN** the first-milestone image
- **WHEN** the tiny program allocates an array
- **THEN** the array's home is `uma`
- **AND** the program still runs

#### Scenario: Placement is not a raw RAM/VRAM enum in source
- **GIVEN** a compiled transform
- **WHEN** its inputs are inspected
- **THEN** they name regions
- **AND** they do not require the Uiua source to mention CPU RAM versus VRAM

### Requirement: Cache domain is recorded
A region MUST carry a cache domain from a documented set that includes at least `l1`, `l2`, `slc`, and `dram`. The first milestone MAY leave the domain as `dram` or unset. A later fusion pass MAY name a domain as the intended working-set home. The first-milestone virt guest MUST NOT be required to implement a real system-level cache.

#### Scenario: Domain is present on a fused region
- **GIVEN** a region produced by the fusion bench
- **WHEN** that region is described
- **THEN** it includes a cache-domain field
- **AND** the tiny program still runs if that field is `dram`

### Requirement: Sixteen-kibibyte alignment
Allocated regions MUST be aligned to the documented guest page size of 16 KiB. The documented cache line is 128 bytes. Sub-page views MUST be allowed as arrays with offset and strides; they MUST NOT require a new physical allocation.

#### Scenario: Fresh allocation is 16 KiB aligned
- **GIVEN** a successful region allocation
- **WHEN** its base address is inspected
- **THEN** the base is aligned to 16 KiB

#### Scenario: Slice is a view
- **GIVEN** a 16 KiB-aligned rank-2 region
- **WHEN** a transform takes a row slice
- **THEN** the slice may share the region
- **AND** no second 16 KiB allocation is required for that slice

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
