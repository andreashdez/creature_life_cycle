# 0008 — A Criterion benchmark guards `Board::refresh`

Decided in `2967bc4`. Status: active.

## Context

`Board::refresh` advances the whole simulation by one turn and is the only hot
function in the project: the GUI calls it on every frame it is playing, and a
CLI run with `--delay-ms 0` calls it as fast as it can. Its cost scales with
both board area and creature count, and neither the test suite nor the visible
frame rate would catch a change that made it quietly twice as slow.

## Decision

`benches/board_refresh.rs` measures `Board::refresh` with Criterion, on a 25×25
board and a 100×100 board with 800 creatures per species, over both single-turn
and multi-turn runs, from seed `42`. The benchmark builds its boards through
`parse_simulation_config`, the same path the binaries use, so it exercises real
configuration rather than a hand-built fixture.

Criterion runs with `harness = false` as a dedicated bench target, and is a dev
dependency only.

## Consequences

There is a number to compare before and after a change to the simulation core,
which is what made record [0009](0009-flat-creature-storage.md) possible to
justify.

The fixed seed means the benchmark does the same work every run, so results are
comparable across builds rather than noisy.

The benchmark is not run in CI. It is a local tool, invoked deliberately with
`cargo bench`; wiring it into CI would need a baseline to compare against and a
tolerance for runner variance, neither of which exists.
