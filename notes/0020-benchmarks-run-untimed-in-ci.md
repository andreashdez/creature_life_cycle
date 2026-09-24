# 0020 — CI runs the benchmarks once, untimed

Decided after a review of how the benchmark is exercised. Status: active.
Supersedes the "not run in CI" consequence of record
[0008](0008-benchmark-as-performance-guard.md); the benchmark itself and its
use as a local before-and-after tool stand.

## Context

Record 0008 kept the benchmark out of CI because timing it there would need a
baseline and an allowance for runner variance. That still holds for timings.
But leaving it out entirely meant CI only compiled `benches/board_refresh.rs`
(through `cargo clippy --all-targets` and `cargo check --all-targets`). Nothing
ran it. `cargo test` skips bench targets by default, so a benchmark that
compiled but then panicked would go unnoticed until someone ran `cargo bench`.
One example is `board_with_config`'s `expect("benchmark config is valid")`
after a change to the config format.

Timing the benchmark on CI was considered and rejected. GitHub's hosted
runners are shared machines, and the same benchmark can easily differ by 10%
or more between runs. A threshold tight enough to catch a real regression would fail at
random.

## Decision

The Linux CI job runs `cargo test --locked --bench '*'`. Criterion's test mode
runs every benchmark function once, without measuring it. The glob selects
only bench targets, so the library and GUI unit tests are not run a second
time, and a new file in `benches/` is picked up without editing the workflow.
Locally the step takes about four seconds once the test build exists.

## Consequences

A benchmark that panics or can no longer build its boards fails CI.

CI still says nothing about speed. `cargo bench`, run before and after a change
on one machine, remains the way to check `Board::refresh`, as `AGENTS.md`
requires for changes that touch it. A CI gate on speed would need a
measurement that does not vary between runs, such as instruction counts under
Valgrind, and is left for a later decision.
