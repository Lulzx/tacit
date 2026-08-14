# First-milestone Uiua subset

The host compiler (`hostc`) lowers a subset of **Uiua** to UIR with shape,
purity, edges, regions, `home = uma`, parallel axes, and required
capabilities. Placement is recorded per node: pure elementwise work is
`engine = neon` (the boot CPU's SIMD unit), everything else is
`engine = p-core`. Anything else is a compile error with a source
location, and no image containing it is produced.

Uiua is a tacit array language: functions appear to the *left* of their
arguments and code is read *right-to-left* (`+ 2 3` is 2+3, `× 2 + 3 5` is
2×(3+5)). Lines execute top-to-bottom with the stack threading between them.

## Values

- integers `42`, floats `3.5`, negatives `¯3`
- strings `"hello"` (rank-1 `u8`)
- lists `[1 2 3 4]` (rank-1) and matrices `[[1 2][3 4]]` (rank-2)

## Bindings

- `Name ← value` binds a value (names are alphabetic, conventionally
  PascalCase).

## Arithmetic, shape, and order (pure)

- `+ - × ÷` elementwise arithmetic (ASCII `*` → `×`, `%` → `÷`), with scalar
  broadcast.
- `=` equals — elementwise, an i64 0/1 mask; ASCII `eq`.
- `/ +` (or `/+`) reduce by addition over the last axis.
- `↯ [dims]` reshape (must preserve element count); reshaping a scalar fills,
  so `↯ [rows cols] value` makes a new array filled with `value`; ASCII
  `reshape`.
- `⇌` reverse; ASCII `reverse`.
- `⧻` length (row count); ASCII `length`.
- `⍏` `⍖` grade up / grade down — the row indices that sort a rank-1 array by
  value or a rank-2 table by its first column; ASCII `up` / `down`.
- `⊏` select rows (or elements) by an index vector; `⊏⍏` sorts ascending and
  `⊏⍖` sorts descending — Uiua's sort train; ASCII `sel`.
- `⊡ n` pick column `n` of a table (element `n` of a list); ASCII `pick`.
- `▽` keep — `▽ mask table` keeps the rows where the mask is nonzero; ASCII
  `keep`.
- `&rows` rank-wise map marker (records the leading axis as parallel).

## System functions (the `&name` convention, like Uiua's `&p`)

- `&display` — write the top of stack to the console (effect; a tee, like
  `&p`). Needs a display cap.
- `&keys` — read one line from the keyboard (effect). Needs a keyboard cap.
- `&graph-nodes` `&graph-edges` `&names` `&caps` `&machine` `&ready` — the
  live graph / machine tables as arrays.  These are OS surface: no core Uiua
  primitive reads the live graph, so they follow Uiua's `&name` convention
  (like `&p`) rather than a glyph.
- `&fmt "template"` — format a value (scalars, arrays, tables) as text.
- `&provenance n` — ask the runtime which transform produced node n's value.
- `&stats` `&bytes` `&copied` `&entries` — in-image traffic counters.
- `&zero` — reset the traffic counters.
- `&send` — same-home send (a region-cap share/move, zero payload copy).
- `&copy` — explicit payload copy (the zero-copy bench control).
- `&fmt-machine` — render the machine description.

The table operations the agent/scheduler policies need are the core glyphs
above: `▽ = … ⊡ …` filters a table by a column (Uiua's keep on a mask), and
`⍏`/`⍖`/`⊏⍏`/`⊏⍖` grade and sort.

## Out of subset (rejected at compile time, with location)

Files, sockets, threads, Metal, CUDA, CoreML, Accelerate, `fork`, `exec`,
`ioctl`, modifiers (fork/gap/…) beyond the above, and any unimplemented
primitive.
