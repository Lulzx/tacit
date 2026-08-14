## Purpose

Treats P-cores, E-cores, NEON, SME, GPU, ANE, media, and display as engines of the same transform over unified memory. Host-versus-device RAM and CUDA/Metal dispatch are not the programming model.

## ADDED Requirements

### Requirement: Placement is engine plus home
A transform record MUST name an engine and a memory home. On Apple Silicon the home MUST be unified memory (`uma`). The documented engines MUST include at least `p-core`, `e-core`, `neon`, `sme`, `gpu`, `ane`, `media`, and `display`. The first milestone MUST run fused kernels on `p-core` with `home = uma`.

#### Scenario: Fused Add-multiply runs on the boot CPU
- **GIVEN** fused `C = (A + B) × D`
- **WHEN** the fusion bench runs
- **THEN** the kernel executes on the boot CPU with `engine = p-core` and `home = uma`
- **AND** the UIR node still names the transform and its shape

#### Scenario: Source does not name an engine
- **GIVEN** the same Uiua source as the fusion bench
- **WHEN** its UIR is inspected
- **THEN** the program text does not mention P-cores, SME, GPU, or Metal
- **AND** the placer or a default records the engine

### Requirement: UMA chains have no bounce copy
Moving an array between homes MUST appear as an explicit move or copy transform. On Apple Silicon, CPU, GPU, and ANE already share one physical pool, so a same-home chain MUST NOT record a host-to-device or device-to-host payload copy. The programmer MUST NOT be required to write a CUDA- or Metal-style alloc/copy/launch/copy-back sequence.

#### Scenario: Same-home chain has no bounce
- **GIVEN** fused Add-Multiply where A, B, D, C all have `home = uma`
- **WHEN** the kernel runs
- **THEN** the bench does not record a host-to-device or device-to-host payload copy

#### Scenario: Later engine does not change source
- **GIVEN** the same Uiua source as the p-core bench
- **WHEN** a later change marks `engine = sme` or `engine = gpu` on that node
- **THEN** that change may retarget the node without rewriting the program as an SME or GPU kernel
- **AND** any required move is an explicit node
- **AND** home remains `uma` unless a later non-UMA machine is added

### Requirement: Discrete host/device is not the core model
The first-milestone placement vocabulary MUST NOT treat CPU RAM and GPU VRAM as the primary split. A later non-unified machine MAY add `home = device`. That addition MUST NOT be required to explain Apple Silicon.

#### Scenario: Host-versus-device enum is refused as the only place
- **GIVEN** a proposed core API whose only placements are `host` and `device`
- **WHEN** it is reviewed against this spec
- **THEN** it is out of contract
- **AND** the equivalent must name an engine and a memory home

### Requirement: Tiles not threads
A large independent axis MUST be executed as tiles or vector lanes of one transform. The system MUST NOT create one kernel thread or POSIX thread per element.

#### Scenario: Large shape is one transform
- **GIVEN** the documented fusion-bench shape
- **WHEN** the fused kernel runs
- **THEN** the unit of work is that transform (possibly tiled)
- **AND** the program did not spawn a worker per element

### Requirement: Offline engines stay legal records
UIR MUST be allowed to record an engine that the current guest has not wired. The first milestone MUST refuse to dispatch an offline engine and MUST run the node on the boot CPU or fail with a documented placement error that does not reset the machine.

#### Scenario: SME record does not require SME hardware
- **GIVEN** a node whose recorded engine is `sme` on the first-milestone virt guest
- **WHEN** the stepper reaches that node
- **THEN** it either runs an equivalent p-core lowering or fails with a placement error
- **AND** the machine does not reset
- **AND** the tiny program shipped with the image does not depend on SME
