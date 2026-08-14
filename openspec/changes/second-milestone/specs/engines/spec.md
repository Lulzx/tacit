## Purpose

Extends placement from "recorded but p-core" to engines that actually execute: NEON for elementwise work and the SME engine (streaming mode) for `&matmul`, both probe-gated and observable through per-engine kernel-entry counters.

## ADDED Requirements

### Requirement: Elementwise work runs on the NEON engine
Pure elementwise nodes (arithmetic, fused Add-then-Multiply) MUST be recorded as `engine = neon` and MUST execute as a 128-bit Advanced-SIMD kernel when their dtype is f32.  The machine description MUST report the NEON unit online.

#### Scenario: Fused kernel is one NEON entry
- **GIVEN** the fused `C = (A + B) × D` bench
- **WHEN** it runs with `&zero` followed by `&stats`
- **THEN** the stats report exactly one kernel entry attributed to `neon`

#### Scenario: Unfused Add then Multiply is two NEON entries
- **GIVEN** the same bench without fusion
- **WHEN** it runs and reports stats
- **THEN** the stats report two kernel entries attributed to `neon`

### Requirement: Matmul engages the SME engine
`&matmul A B` for f32 rank-2 matrices MUST be recorded as `engine = sme` and MUST enter and leave streaming mode when SME is implemented on the boot CPU.  Presence MUST be probed from `ID_AA64PFR1_EL1.SME`, and the machine description MUST report SME online only when the probe passes; otherwise the kernel MUST fall back to p-core and the counters MUST report the engine that actually ran.

#### Scenario: SME matmul under TCG max
- **GIVEN** a CPU with FEAT_SME (`-cpu max`)
- **WHEN** the matmul bench runs `&matmul A B` with `A = ↯ [64 64] 1.5`, `B = ↯ [64 64] 2.0`
- **THEN** the top-left element of the result is `192.0`
- **AND** the stats report one kernel entry attributed to `sme`

#### Scenario: No SME present falls back
- **GIVEN** a CPU without FEAT_SME
- **WHEN** the matmul bench runs
- **THEN** the product is still computed correctly
- **AND** the stats report the fallback engine (p-core), not `sme`

### Requirement: Streaming mode is enabled correctly
The kernel MUST set `CPACR_EL1.SMEN` (bits 25:24) before executing `smstart`; an unconfigured SME traps with `EC_SMETRAP` (ESR class 0x1D) and MUST NOT be treated as the engine path.
