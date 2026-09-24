# 0018 — A `Board` is built in one step

Decided after a review of the library's public API. Status: active.

## Context

`Board::new()`, and `Default`, which called it, returned a 0 × 0 board with no
cells. It did nothing useful until it was passed to `parse_simulation_config`
or `load_standard_data`, which filled it in through a private `create_field`.
Every caller wrote the same two lines, `let mut board = Board::new();` and
then a call that filled it in, and nothing stopped a caller from skipping the
second. A skipped call gave a board that `refresh` would treat as empty and
that `cell_counts` would answer `None` for everywhere. `create_field` also had
to clear eight collections and reset two counters in case it was handed a
board that had already been used, although no caller ever did that.

## Decision

A board only comes out of a constructor that finishes it:

- `Board::standard(random)` builds the built-in 10 × 10 board that
  `simulation.example.toml` describes. It replaces
  `load_standard_data(&mut board, random)`.
- `parse_simulation_config(contents, random)` builds the board a config
  describes. It returns it in the same `LoadedBoard` that the load functions
  return, together with any notices (record
  [0017](0017-library-returns-typed-errors.md)), and no longer takes a
  `&mut Board`.

Both build on a private `Board::with_field(rows, cols, random)`, which lays out
the cells with their random starting food. `Board::new`, `Default for Board`,
`load_standard_data` and `create_field` are removed.

`parse_simulation_config` stays a free function next to
`format_simulation_config` instead of becoming `Board::from_config`, so that
reading and writing the config format stay side by side.

## Consequences

A half-built board can no longer be written, and the clearing code went away
with `create_field`.

Starting food is drawn from `random` in the same row-major order as before,
and an invalid config is still rejected before anything is drawn. Seeded runs
are unchanged, and the snapshot tests pass as they were. Two tests now pin
this down: one checks that `Board::standard` matches the example config, and
one checks that a failed parse followed by the fallback draws the same numbers
as the fallback alone.

The GUI's Reset, its tests, and the benchmark each lost a line of setup.
