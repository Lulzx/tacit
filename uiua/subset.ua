# Subset coverage: reduce (/+), reshape (↯), rank-wise map, capabilities,
# grade/select sort (⊏⍏).
M ← [[1 2 3][4 5 6]]
T ← ↯ [4 2] [3 1 4 1 5 9 2 6]
&display &fmt "row-sums: " /+ &rows M
&display &fmt "reshaped: " ↯ [3 2] M
&display &fmt "sorted rows: " ⊏ ⍏ T
&display &fmt "capabilities: " ⧻ &caps
