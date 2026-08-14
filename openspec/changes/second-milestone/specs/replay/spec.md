## Purpose

Deterministic replay of world reads: every effect input (`&keys`, `&clock`) is recorded with a sequence number, and the replay functions consume the recorded input instead of the live device, so a sequence run twice is deterministic.

## ADDED Requirements

### Requirement: World reads are recorded
Every `&keys` line and `&clock` read MUST be appended to an effect-input trace with a monotonic sequence number.  `&trace` MUST project the trace as a table `[seq, kind, bytes]`.

### Requirement: Replay consumes recorded input in order
`&replay-keys` and `&replay-clock` MUST return the next recorded input of that kind, in the order recorded, instead of reading the live device.  Replaying a sequence MUST reproduce the recorded inputs exactly.

#### Scenario: Replayed clock equals recorded clock
- **GIVEN** a `&clock` read that returned value `t`
- **WHEN** `&replay-clock` is evaluated immediately afterward
- **THEN** it returns `t`

#### Scenario: Keyboard line replays
- **GIVEN** a keyboard line `hi` read through `&keys` and recorded
- **WHEN** `&replay-keys` is evaluated
- **THEN** it returns `hi`
