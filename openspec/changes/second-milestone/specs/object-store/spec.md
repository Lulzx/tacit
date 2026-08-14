## Purpose

A content-addressed object store for values: `id = H(data)`, deterministic, deduplicating, and loadable by id.  The store holds values, not bulk payloads (those belong to the datapath).

## ADDED Requirements

### Requirement: Content ids are deterministic and deduplicating
`&hash` MUST compute a deterministic id from a value's descriptor and payload.  `&store` MUST register a copy under that id and return it; storing an equal value a second time MUST return the same id without duplicating the object.  `&load id` MUST return the stored value.

#### Scenario: Same data, same id
- **GIVEN** `V ← [1 2 3 4]` and `H ← &store V`
- **WHEN** `= H &store V` is evaluated
- **THEN** it equals `1`

#### Scenario: Load returns the stored value
- **GIVEN** `V ← [1 2 3 4]` and `H ← &store V`
- **WHEN** `&load H` is displayed
- **THEN** the display shows `[1 2 3 4]`

### Requirement: Store is for values, not bulk payloads
The store MUST refuse payloads over 64 KiB; bulk data stays on the zero-copy datapath.
