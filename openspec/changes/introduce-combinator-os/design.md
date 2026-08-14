## Context

Sits on `introduce-uiua-os` (graph, Realms, caps) and does not replace `introduce-speed-stack`. See those proposals for boot and fusion.

The question this change answers: if Unix had never existed, and the primary users were humans *and* agents, what are the primitives?

## Goals / Non-Goals

**Goals:**

- Five primitives only. Derive or refuse the 1970s nouns.
- Live graph as the inspectable machine.
- Effects go through propose/simulate/validate/commit.
- Agents are transforms over arrays of machine state.

**Non-Goals:**

- A pretty desktop on a conventional kernel.
- LLM weights in the guest.
- Full multi-agent planner on the first QEMU image.
- POSIX compatibility as a migration path.

## Decisions

### Decision: Combinator OS, not process OS

```text
intent (human or agent)
        ↓
 composition graph
        ↓
   pure work     effects
        ↓           ↓
  CPU/GPU/NPU   capabilities
        ↓           ↓
      world state
        ↓
   provenance graph
```

Linux: everything is a file. Tacit: everything is a composable transformation, and every effect carries authority and provenance.

### Decision: Do not implement the Unix trophy list

Boot, allocator, keyboard, display, UIR, one Realm stay in `introduce-uiua-os`. This change adds query, preview, and one agent-shaped transform. It does not add ELF, POSIX, a shell, or a filesystem so we can say they were written in Uiua.

### Decision: First inspectability is text

The live graph is real in memory. The first view is a console projection: nodes, edges, shapes, caps. A later GUI is another projection of the same object. Drag-to-compose is specified now so the GUI cannot become a launcher for Unix binaries.

### Decision: Simulation is cheap because values are data

Simulate walks the graph with the same stepper, writing to a shadow region or a predicted-effect record. Display write predicts "console would show X" without touching the framebuffer until commit. Later irreversible ops (delete, net, spend) must be marked. Do not run an LLM in simulate.

### Decision: Agent algebra is UIR

The agent does not emit bash. It emits or selects a UIR composition. Machine tables (transforms, caps, later files/net) are arrays. `filter(cpu > 0.8)` is a transform. Ten agents are ten nodes, not ten mystery PIDs.

First milestone: one granted transform that queries the live graph of the tiny program and commits a display summary.

### Decision: Visual design follows the model

When a compositor exists, windows are `State × Events → Pixels` and the picture is the graph. Resource monitors show transforms flowing onto placements. That work is a later change. This change only forbids a skin on Unix.

## Risks / Trade-offs

- **[Risk] The graph store becomes a second kernel.** → Mitigation: it *is* the kernel's account of work. No shadow process table.
- **[Risk] Simulate is a full second OS.** → Mitigation: first slice only predicts display and cap checks.
- **[Risk] "Agent" invites a chatbot in ring 0.** → Mitigation: agent means a granted transform. No model in the guest for this change.
- **[Risk] We still build `ps` "in Uiua."** → Mitigation: tests query the graph. If a test parses process text, it fails the spec.

## Migration Plan

No users. Sequence is `tasks.md`. Depends on named UIR after boot.

## Open Questions

- Exact text format of the first graph projection. Pick at implement time if nodes, edges, and shapes are all present.
- When a graphical projection gets its own change. Not this one.
