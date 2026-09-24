# 0014 — The RNG algorithm is named explicitly as `ChaCha12Rng`

Decided after a review of record [0001](0001-deterministic-seeded-simulation.md).
Status: active. Supersedes the choice of `StdRng` in 0001; the rest of 0001
stands.

## Context

Record 0001 put all randomness behind one `Random` wrapper over
`rand::rngs::StdRng` and claimed that runs are "reproducible across machines
and builds". The first half holds; the second does not. `rand` documents
`StdRng` as "the recommended general-purpose generator", and explicitly
reserves the right to change which algorithm backs it in any release. A
`cargo update` that crossed such a release would change every seeded run
without a single line of this project changing, and the snapshot test in
`tests/seeded_e2e_snapshots.rs` would fail for reasons unrelated to the
simulation.

## Decision

`Random` wraps `rand_chacha::ChaCha12Rng` directly. That is the algorithm
`StdRng` uses in `rand` 0.9, so seeded runs, and therefore the snapshot test,
are unchanged by the switch. `rand_chacha` was already in the dependency tree
as a dependency of `rand`, so no new crate is compiled.

`ChaCha8Rng` would be faster, but switching to it would change every seeded
run and the random draws are not where the simulation spends its time.

## Consequences

A seed now names a fixed sequence for as long as the project stays on the
`rand_chacha` 0.9 line. A major `rand_chacha` upgrade is still a deliberate
step, and the snapshot test is what shows whether it changed the stream.

The claim in 0001 that runs are reproducible across builds is true from this
record onward.
