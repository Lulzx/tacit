# First-milestone Uiua subset

The host compiler (`hostc`) lowers a subset of **Uiua** to UIR with shape,
purity, edges, regions, `home = uma`, `engine = p-core`, parallel axes, and
required capabilities. Anything else is a compile error with a source
location, and no image containing it is produced.

Uiua is a tacit array language: functions appear to the *left* of their
arguments and code is read *right-to-left* (`+ 2 3` is 2+3, `× 2 + 3 5` is
2×(3+5)). Lines execute top-to-bottom with the stack threading between them.

## Values

- integers `42`, floats `3.5`, negatives `¯3`
- strings `"hello"` (rank-1 `u8`)
- lists `[1 2 3 4]` (rank-1) and matrices `[[1 2][3 4]]` (rank-2)
- `&fill [rows cols] value` — a large array filled with `value`

## Bindings

- `Name ← value` binds a value (names are alphabetic, conventionally
  PascalCase).

## Arithmetic and shape (pure)

- `+ - × ÷` elementwise arithmetic (ASCII `*` → `×`, `%` → `÷`), with scalar
  broadcast.
- `/ +` (or `/+`) reduce by addition over the last axis.
- `↯ [dims]` reshape (must preserve element count); ASCII `reshape`.
- `⇌` reverse; ASCII `reverse`.
- `⧻` length (row count); ASCII `length`.
- `&rows` rank-wise map marker (records the leading axis as parallel).

## System functions (the `&name` convention, like Uiua's `&p`)

- `&display` — write the top of stack to the console (effect; a tee, like
  `&p`). Needs a display cap.
- `&keys` — read one line from the keyboard (effect). Needs a keyboard cap.
- `&graph-nodes` `&graph-edges` `&names` `&caps` `&machine` `&ready` — the
  live graph / machine tables as arrays.
- `&fmt "template"` — format a value (scalars, arrays, tables) as text.
- `&provenance n` — ask the runtime which transform produced node n's value.
- `&stats` `&bytes` `&copied` `&entries` — in-image traffic counters.
- `&zero` — reset the traffic counters.
- `&filter-pure` `&filter-effectful` `&sort-asc` `&sort-desc` `&order` — the
  table ops the scheduler/agent policies compose.
- `&send` — same-home send (a region-cap share/move, zero payload copy).
- `&copy` — explicit payload copy (the zero-copy bench control).
- `&fmt-machine` — render the machine description.

## Out of subset (rejected at compile time, with location)

Files, sockets, threads, Metal, CUDA, CoreML, Accelerate, `fork`, `exec`,
`ioctl`, modifiers (fork/gap/…) beyond the above, and any unimplemented
primitive.
