# Subset coverage: reduce, reshape, rank-wise map, and the capabilities table.
M ← [[1 2 3][4 5 6]]
M &rows &reduce-sum &fmt "row-sums: " &display
M &reshape [3 2] &fmt "reshaped: " &display
&caps &count &fmt "capabilities: " &display
