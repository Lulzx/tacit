# One granted agent-shaped transform: two independent summaries fan out from
# one graph query; the scheduler policy orders them.
G ← &graph-nodes
&display &fmt "pure: " ⧻ &filter-pure G
&display &fmt "effectful: " ⧻ &filter-effectful G
