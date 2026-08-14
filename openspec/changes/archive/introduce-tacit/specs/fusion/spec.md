## Purpose

Defines operator fusion: adjacent pure UIR nodes become one kernel so array chains do not write intermediates through the last-level cache or DRAM.

## ADDED Requirements

### Requirement: Fuse adjacent pure elementwise nodes
The system MUST fuse a chain of adjacent pure elementwise transforms into a single kernel when all intermediates are used only by the next node in that chain. The first required case is `C = (A + B) × D` (Add then Multiply). Fusion MUST NOT change the numerical result.

#### Scenario: Add-multiply fuses
- **GIVEN** subset source equivalent to `C = (A + B) × D` with A, B, D the same shape
- **WHEN** the program is compiled and the fusion pass runs
- **THEN** the executed graph has one kernel for that chain
- **AND** there is no live intermediate array T = A+B after the kernel
- **AND** C equals the unfused result elementwise

#### Scenario: Dependence blocks fusion
- **GIVEN** Add whose result is consumed by two independent later nodes
- **WHEN** fusion runs
- **THEN** Add is not fused away if that would force a second materialization that is not documented
- **AND** both consumers still see the same values

#### Scenario: Effectful node is a fence
- **GIVEN** a pure Add followed by a display write
- **WHEN** fusion runs
- **THEN** the display write is not fused into Add
- **AND** the write still requires a display capability

### Requirement: Fusion is measured
The project MUST ship a documented bench that runs fused and unfused `C = (A + B) × D` on a documented large shape and reports runtime and a memory-traffic proxy (bytes moved or load/store counts). The fused path MUST move fewer bytes than the unfused path on that shape. On Apple Silicon the interesting later metric is whether the working set stays in the system-level cache; the first bench on QEMU virt MUST still report the byte-traffic proxy and MUST NOT claim a real SLC occupancy number unless the guest can measure one.

#### Scenario: Traffic drops
- **GIVEN** the documented bench and shape
- **WHEN** fused and unfused runs complete
- **THEN** both write the same C
- **AND** the fused run reports strictly fewer memory bytes than the unfused run
- **AND** the numbers are printed or written to a documented log

#### Scenario: Unfused remains available
- **GIVEN** a documented flag or build mode
- **WHEN** fusion is disabled
- **THEN** Add and Multiply execute as separate nodes
- **AND** the bench can still compare the two modes

#### Scenario: Virt does not invent SLC counters
- **GIVEN** the first-milestone QEMU virt guest
- **WHEN** the fusion bench prints its counters
- **THEN** it reports the in-image traffic proxy
- **AND** it does not print a claimed SLC hit rate unless that counter is documented as measured

### Requirement: Tiles may name a cache domain
A fused kernel MAY be tiled. When tiles are used, the documented tile size MUST be justified against a cache domain (`l2` or `slc` on the M4 Pro description). The first milestone MAY run an untiled kernel if the bench still shows fewer bytes moved.

#### Scenario: Tile size is documented when tiling exists
- **GIVEN** a fused kernel that tiles the independent axis
- **WHEN** the bench documentation is read
- **THEN** it names the tile shape and the cache domain it is sized for
- **AND** the numerical result still matches the unfused run
