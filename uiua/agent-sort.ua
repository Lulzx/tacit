# Agent: which nodes are effectful (the display node), as rows of the live
# graph table — keep on a mask over the purity column.
G ← &graph-nodes
&display &fmt "effectful rows: " ▽ = 0 ⊡ 2 G
