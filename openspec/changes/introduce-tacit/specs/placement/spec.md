## Purpose

Treats CPU, later SIMD, and later GPU as placements of the same transform, not separate programming models. Bounce copies are explicit and counted.

## ADDED Requirements

### Requirement: Host placement is required
Every first-milestone fused kernel MUST run on host (CPU) placement. UIR MUST still record shape and parallel axes so a later placer can retarget the same node.

#### Scenario: Fused Add-multiply runs on host
- **GIVEN** fused `C = (A + B) × D`
- **WHEN** the fusion bench runs
- **THEN** the kernel executes on the boot CPU or host stepper
- **AND** the UIR node still names the transform and its shape

### Requirement: Bounce copies are not implicit
Moving an array between placements MUST appear as an explicit move or copy transform. The programmer MUST NOT be required to write a CUDA-style alloc/copy/launch/copy-back sequence.

#### Scenario: Same-place chain has no bounce
- **GIVEN** fused Add-Multiply where A, B, D, C are all host
- **WHEN** the kernel runs
- **THEN** the bench does not record a host-to-device or device-to-host payload copy

#### Scenario: Later device place does not change source
- **GIVEN** the same Uiua source as the host bench
- **WHEN** a later change adds a device placement
- **THEN** that change may retarget the node without rewriting the program as GPU kernels
- **AND** any required move is an explicit node

### Requirement: Tiles not threads
A large independent axis MUST be executed as tiles or vector lanes of one transform. The system MUST NOT create one kernel thread or POSIX thread per element.

#### Scenario: Large shape is one transform
- **GIVEN** the documented fusion-bench shape
- **WHEN** the fused kernel runs
- **THEN** the unit of work is that transform (possibly tiled)
- **AND** the program did not spawn a worker per element
