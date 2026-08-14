# First-milestone Uiua subset

The host compiler lowers this subset to UIR with shape, purity, edges,
regions, `home = uma`, `engine = p-core`, parallel axes, and required
capabilities. Anything else is a compile error with a source location, and no
image containing it is produced.

## Values

- integer literals `42`, float literals `3.5`
- character strings `"hello"` (a rank-1 `u8` array)
- numeric lists `[1 2 3 4]` (rank-1 `i64`) and matrices `[[1 2][3 4]]` (rank-2)
- `&fill [rows cols] value` — a large array filled with `value` (used by the
  fusion bench)

## Bindings and composition

- `name ← expr` binds a value; the body is the remaining lines, read
  left-to-right on a stack.
- `A B + D ×` computes `(A + B) × D`.

## Arithmetic and shape (pure)

- `+` add, `-` subtract, `×` (or `*`) multiply, `÷` (or `/`) divide, with
  scalar broadcast against an array.
- `&reduce-sum` — reduce over the last axis (rank-1 → scalar, rank-2 → rank-1).
- `&reshape [dims]` — change shape, metadata only (must preserve element count).
- `&rows` — rank-wise map marker: the leading axis is recorded as an
  independent parallel dimension (the guest still runs it in order).
- `&reverse` — reverse a 1-d array or the rows of a table.

## Effects (require a capability)

- `&display` — write the top of stack to the console (needs a display cap)
- `&keys` — read one line from the keyboard (needs a keyboard cap)

## Graph and machine query (pure sources)

- `&graph-nodes`, `&graph-edges`, `&machine`, `&ready`, `&caps`
- `&count`, `&filter-pure`, `&filter-effectful`, `&sort-asc`, `&sort-desc`,
  `&order`
- `&fmt "template"` — format a scalar/array as text
- `&bytes`, `&copied`, `&entries` — the in-image traffic counters

## Out of subset (rejected at compile time, with location)

Files, sockets, threads, Metal, CUDA, CoreML, Accelerate, `fork`, `exec`,
`ioctl`, and any unimplemented primitive.
