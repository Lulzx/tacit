## Why

Tacit is not interesting as a hobby kernel in Uiua. It is interesting as the answer to: **what would an OS look like if Unix had never existed, and agents were first-class users?**

Unix is `process → bytes → pipe → process`. Tacit is `value → transformation → array → parallel transformation → value`. The program *is* the schedule. The live graph *is* `ps`, `top`, `strace`, `lsof`, `/proc`, and the debugger.

Agents should not click buttons or parse `ps` text. They should construct a valid transformation over structured state, with capabilities in the dataflow and effects visible before commit.

**Thesis:** everything is a composable transformation, and every effect carries authority and provenance.

**Agent-native equation:** structured state + composable actions + capabilities + observable effects + cheap parallelism.

## What Changes

- Name five native primitives and force everything else to emerge from them: **values, transformations, composition, capabilities, evaluation**.
- Files, processes, IPC, services, concurrency, and later GUI are derived from that vocabulary or they do not exist.
- The machine's primary interface is the **live computation graph**. `ps`/`top`/`strace`/`lsof`/`/proc` are projections of that graph, not separate subsystems.
- Visual design, when it exists, is the same object the scheduler executes. A pipeline on screen is the pipeline that runs.
- Agent execution is observe → propose → simulate → validate → commit → observe. No ambient shell.
- An agent is a transform `(state, capabilities, goal) → (state', evidence)`, not a process running Python.
- Parallel agent branches are the same kind of thing as independent array rows.

## Non-goals

- Linux-in-Uiua: bootloader, ELF, POSIX, shell, FS, drivers, GUI as a trophy list.
- Agents that drive a conventional desktop by clicking.
- A separate "agent framework" bolted onto a Unix kernel.
- Shipping a full multi-agent planner in the first QEMU milestone.
- Replacing the speed stack or the boot change.

## Capabilities

### New Capabilities

- `combinators`: The five primitives. Unix objects must be derived or refused.
- `live-graph`: The running machine is a queryable graph. Inspection tools are projections.
- `effects`: Every effect names authority and predicted world. Propose, simulate, validate, commit.
- `agents`: Agents are transformations with a small compositional algebra. Authority flows as data.

### Modified Capabilities

- None in `openspec/specs/` yet. This change sits beside `introduce-uiua-os` and `introduce-speed-stack`.

## Impact

- Planning only. Implementation of the first slice waits on a loaded UIR graph from `introduce-uiua-os`.
- Adds graph query, effect preview, and a one-agent composition over machine tables.
- Success is not "Uiua can implement POSIX." Success is "those abstractions were never required."
