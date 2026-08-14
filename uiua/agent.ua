# One granted agent-shaped transform: query the live graph and write two
# independent summaries.  The two summary branches fan out from the same
# graph-query node, so they are independent ready nodes of equal depth; the
# scheduler policy (Uiua compiled to UIR) decides which runs first.
G ← &graph-nodes
G &filter-pure &count &fmt "pure: " &display
G &filter-effectful &count &fmt "effectful: " &display
