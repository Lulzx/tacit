## Purpose

Extends the agent story from one granted transform to several, ordered by a Uiua planner: the agent set is a table `[agent_id, priority]`, a Uiua plan sorts it by priority, and the runtime runs the agents in plan order.

## ADDED Requirements

### Requirement: Agents are ordered by a Uiua plan
Given an agent table, a Uiua plan program MUST return the plan (the sorted table's first column is the run order).  Each agent MUST run as a granted transform with only the capabilities it needs, and MAY run with the node-level scheduling policy.

#### Scenario: Highest priority runs first
- **GIVEN** three agents with priorities 1, 3, and 2
- **WHEN** the planner orders them (`⊏ ⍖ ⊡ 1 &ready` over the agent table)
- **THEN** the priority-3 agent runs first, then priority 2, then priority 1

#### Scenario: Each agent is a Uiua program over the live graph
- **GIVEN** the live graph of the bundled tiny program
- **WHEN** the agents run
- **THEN** each agent's summary is derived from the graph tables (nodes, purity column) with the subset's glyphs
