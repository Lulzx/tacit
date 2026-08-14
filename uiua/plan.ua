# Multi-agent planner: order the agent table by its priority key (column 1),
# highest first — Uiua's sort train over the picked column: select on
# grade-down of `⊡ 1`.  The runtime reads the plan's first column (the agent
# ids) and runs the agents in that order.
⊏ ⍖ ⊡ 1 &ready
