# 0003 — TOML replaces the positional `.conf` files

Decided in `fd0d343`, completed in `cb057a7`. Status: active.

## Context

The project shipped with three hand-rolled configuration files: `board.conf`,
`aphid.conf`, and `ladybug.conf`. They were positional — every line's meaning
came from its position in the file, with no keys and no comments. `board.conf`
began:

```text
10 10
5
3 5
4 8
```

That is board dimensions, then an aphid count, then that many coordinate pairs.
`aphid.conf` was four bare probabilities whose order had to be memorised or
looked up in the parser. Nothing was optional, nothing was labelled, and an
off-by-one line shifted the meaning of every line after it. Each file needed
its own parser, which grew to roughly 130 lines of `src/lib.rs`.

## Decision

All three files are replaced by a single `simulation.toml` with named sections
and keys, deserialized with `serde` and the `toml` crate:

```toml
[board]
rows = 10
columns = 10

aphids = [{ x = 3, y = 5 }]

[aphid]
move_probability = 0.7
```

The `[aphid]`, `[ladybug]`, and `[food]` sections are optional; an omitted
section falls back to the built-in defaults. The hand-written parsers were
deleted outright in `cb057a7`.

## Consequences

Configuration became self-describing and partially specified: a user who only
wants to change one probability writes one section instead of reproducing every
value in order. Validation improved too, since parse errors now name the key
that is wrong.

Two dependencies were added, `serde` and `toml`, both small. The deserialized
config types are kept separate from the runtime parameter types
(`AphidConfig` versus `AphidParams`), so validation has a place to live between
the two: probabilities are checked to be within `0.0..=1.0` on the way across.

The example file moved with the format and is what
`tests/seeded_e2e_snapshots.rs` feeds the binary, so the shipped example is
covered by the test suite.
