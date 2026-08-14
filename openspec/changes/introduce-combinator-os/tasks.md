## 1. Five primitives stay the vocabulary

- [ ] 1.1 Document the five primitives and a one-page map from Unix nouns to derived forms or "refused"
- [ ] 1.2 Add a review check that new core objects are values, transforms, composition, caps, or evaluation

## 2. Live graph

- [ ] 2.1 Keep the loaded UIR as the source of truth for running work (no process table)
- [ ] 2.2 Expose a query: nodes, edges, shapes, caps, ready set, as arrays
- [ ] 2.3 Print a console projection of the tiny program's graph (Add, Multiply, display)
- [ ] 2.4 Answer provenance for C: producer Multiply, inputs Add-result and D

## 3. Effects

- [ ] 3.1 Classify nodes as pure or effectful with required caps
- [ ] 3.2 Simulate a display write without changing the console
- [ ] 3.3 Commit after validate. Missing display cap leaves the console unchanged
- [ ] 3.4 Mark whether the previous console contents can be restored

## 4. One agent-shaped transform

- [ ] 4.1 Provide machine tables for transforms and capabilities as arrays
- [ ] 4.2 Run one granted transform that filters or summarizes the live graph and proposes a display write
- [ ] 4.3 Give that transform only the caps it needs. No ambient shell
- [ ] 4.4 Represent two independent summaries as fan-out nodes, even if both still run on the boot CPU
