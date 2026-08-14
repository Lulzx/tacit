# Deterministic replay: every effect input (keyboard line, clock) is recorded
# with a sequence number; the replay functions consume the recorded input
# instead of the live device, so a sequence run twice is deterministic.
&display &fmt "clock: " &clock
&display &fmt "replay: " &replay-clock
&display &fmt "clock: " &clock
&display &fmt "replay: " &replay-clock
&display "trace (seq, kind, bytes):"
&display &trace
