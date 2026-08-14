# Subset coverage: reduce (/+), reshape (↯), rank-wise map, capabilities.
M ← [[1 2 3][4 5 6]]
&display &fmt "row-sums: " /+ &rows M
&display &fmt "reshaped: " ↯ [3 2] M
&display &fmt "capabilities: " ⧻ &caps
