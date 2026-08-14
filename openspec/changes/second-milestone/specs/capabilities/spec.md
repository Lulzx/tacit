## Purpose

Extends capability unforgeability from software-only to pointer-authenticated: when FEAT_PACGA is present, a capability token is the `pacga` MAC of a kernel-generated nonce under a kernel-only GA key, and the kernel verifies it by recomputing the MAC over the stored nonce.

## ADDED Requirements

### Requirement: Tokens are PAC-signed when FEAT_PACGA is present
The kernel MUST probe `ID_AA64ISAR1_EL1.GPI` at boot.  When the probe passes, every minted capability token MUST be `pacga(nonce, modifier)` under a GA key set by the kernel, and `lookup` MUST accept a token only if `pacga(nonce, modifier)` recomputes to it.  The GA key MUST be freshly random per boot.

#### Scenario: Bit-flipped genuine token is rejected
- **GIVEN** a genuine display capability token
- **WHEN** a single bit of it is flipped and used to write the display
- **THEN** the write fails with a capability error
- **AND** the console is unchanged

#### Scenario: Forged integer is rejected
- **GIVEN** no display capability beyond what was granted
- **WHEN** an arbitrary integer is presented as a display capability
- **THEN** the write fails with a capability error

### Requirement: Software fallback keeps the spec unchanged
When FEAT_PACGA is absent, tokens MUST be software-unforgeable random values and all capability behavior MUST be unchanged.  A Realm's granted capabilities MUST continue to work identically in both modes.
