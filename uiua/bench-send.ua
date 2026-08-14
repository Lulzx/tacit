# Zero-copy send bench: immutable same-home share (no payload copy) vs an
# explicit payload copy, reported by the program itself.
&display "bench-send (immutable same-home share vs explicit copy):"
&zero
&send ↯ [8388608] 1.0
&display &fmt "share copied bytes: " &copied
&copy ↯ [8388608] 1.0
&display &fmt "copy copied bytes:  " &copied
