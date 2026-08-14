# Fusion bench: C = (A + B) × D over a documented large shape (2048x2048 f32),
# with the in-image byte-traffic proxy reported by the program itself.
# Compiled both fused (one AddMul kernel) and unfused (Add then Multiply).
A ← ↯ [2048 2048] 1.0
B ← ↯ [2048 2048] 2.0
D ← ↯ [2048 2048] 3.0
&display "bench-fusion (2048x2048 f32, home=uma, engine=neon):"
&zero
× D + B A
&display &stats
&display "in-image byte-traffic proxy (no SLC hit rate claimed on virt)"
