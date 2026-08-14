# Content-addressed object store: id = H(data).  Same data, same id; loading
# by id returns the same value.  The store holds values, not bulk payloads
# (those stay on the datapath).
V ← [1 2 3 4]
H ← &store V
&display &fmt "content id: " H
&display &fmt "dedup same id: " = H &store V
&display &fmt "loaded: " &load H
