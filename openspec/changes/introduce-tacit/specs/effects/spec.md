## Purpose

Makes effects explicit: a transformation names its authority, predicted world, and irreversibility, and can be simulated before commit. Engine choice is placement, not an effect, unless it mutates the world.

## ADDED Requirements

### Requirement: Effects are data
An effectful transformation MUST declare input, effect class, required capabilities, and output. Pure transformations MUST declare that they have no effect class.

#### Scenario: Display write is classified
- **GIVEN** the tiny program's display write
- **WHEN** its node is inspected
- **THEN** it is effectful
- **AND** it names a display capability
- **AND** Add is marked pure

### Requirement: Propose, simulate, validate, commit
An agent or operator MUST be able to submit a transformation as a proposal, receive a predicted world (values and effects that would occur), and only then commit. Commit MUST fail if a required capability is missing or validation rejects the predicted effects.

#### Scenario: Preview a display write
- **GIVEN** a proposed write of array X to the display
- **WHEN** simulate runs
- **THEN** the result names the display, the shape of X, and that the console would change
- **AND** the console has not yet changed

#### Scenario: Commit applies the prediction
- **GIVEN** a simulated display write that validated
- **WHEN** commit runs
- **THEN** the console shows X
- **AND** provenance records the commit as the cause

#### Scenario: Missing cap fails before mutation
- **GIVEN** a proposed display write with no display capability
- **WHEN** validate or commit runs
- **THEN** it fails with a capability error
- **AND** the console is unchanged

### Requirement: Irreversible effects are marked
Effects that cannot be undone (later: delete object, send network, spend a quota) MUST be marked irreversible in the prediction. The first milestone MUST mark display overwrite as reversible only if the previous console contents are retained for undo, otherwise irreversible.

#### Scenario: Prediction names reversibility
- **GIVEN** a simulated display write
- **WHEN** the prediction is read
- **THEN** it states whether the previous console contents can be restored
- **AND** commit still requires explicit acceptance of that mark
