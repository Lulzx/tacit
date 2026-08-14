# One granted agent-shaped transform: two independent summaries fan out from
# one graph query; the scheduler policy orders them.  Filtering is `▽` keep
# with a mask (`= 1` over the purity column `⊡ 2`); sorting is `⍏` grade.
G ← &graph-nodes
&display &fmt "pure: " ⧻ ▽ = 1 ⊡ 2 G
&display &fmt "effectful: " ⧻ ▽ = 0 ⊡ 2 G
