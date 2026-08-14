# Scheduler: the graph runtime's per-iteration decision, in Uiua.
# Readiness is Rust-computed and exposed as column 6 of &graph-nodes
# (ready = !done && all inputs done); the *decision* — ordering and runtime
# engine placement — is this program.
# Placement: engine = 3*is_matmul*SME_online + 2*is_elementwise*(1-is_matmul).
#   matmul -> sme (or p-core when SME is offline); elementwise -> neon;
#   everything else (consts, effects, sources) -> p-core.
# Order: ascending node id (swap ⍏ for ⍖ to reverse).
# Returns [2, n]: row 0 = ordered ready node ids, row 1 = their engines.
# The schedule is the last node (kept alive); the host renders it for the
# boot log (a &display tee would consume and free the couple).
O ← ⊡ 1 &graph-nodes        # op column [N]
R ← ⊡ 6 &graph-nodes        # ready column [N]
M ← = 44 O                  # is_matmul mask [N]
W ← + = 23 O + = 4 O + = 3 O + = 2 O = 1 O   # is_elementwise mask [N]
S ← ⊡ 3 ⊡ 1 &machine        # sme_online scalar (machine row 3, col 1)
E ← + × 3 × M S × 2 × W - 1 M   # engine per node [N]
T ← ▽ R &graph-nodes        # ready rows [n,9]
I ← ⊡ 0 T                   # ready ids [n]
G ← ▽ R E                   # ready engines [n]
K ← ⍏ I                     # order permutation [n]
Sched ← ⊟ ⊏ K I ⊏ K G       # [2,n]: row0=ordered ids, row1=ordered engines
Sched                        # the schedule is the last node (kept alive)
