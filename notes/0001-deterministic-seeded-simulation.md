# 0001 — A seeded random wrapper makes every run reproducible

Decided in `42ebf2f` (initial commit). Status: active.

## Context

The simulation is driven almost entirely by chance. Movement, combat,
procreation, starvation, and food regrowth each consult a random number
generator, several times per creature per turn. That makes the interesting
behaviour — population booms, collapses, a species going extinct — emergent,
and it makes bugs very hard to pin down: a rule that misfires once in a hundred
turns cannot be investigated if the hundred turns are different every time.

The same problem applies to tests. Asserting anything about a turn requires
knowing which way each coin came up.

## Decision

All randomness goes through a single `Random` wrapper over
`rand::rngs::StdRng`, and nothing else in the codebase touches an RNG. The
wrapper can be built two ways: `Random::new` seeds from operating-system
randomness, and `Random::with_seed` takes a `u64`. The CLI exposes the second
form as `--seed`.

Because every draw comes from one generator in a deterministic order, a given
starting configuration plus a given seed always produces the same run.

## Consequences

Runs are reproducible across machines and builds, which is what makes
`tests/seeded_e2e_snapshots.rs` possible: it runs the binary against a fixed
config and seed and compares the printed output.

Every function that can make a random decision has to take `&mut Random`, which
threads the parameter through a large part of the API — `add_ladybug` needs it
only to pick a starting direction set, for example. This is accepted as the
price of having exactly one source of randomness.

Determinism is a property of the *sequence* of draws, so it is fragile in one
specific way: reordering the phases of a turn, or the creatures within a phase,
changes every seeded run. Record
[0002](0002-phased-turn-order.md) fixes that order deliberately, and the
snapshot test is what catches an accidental change to it.
