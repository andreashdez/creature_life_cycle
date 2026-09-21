# 0005 — Food regrows slowly and is capped per cell

Decided in `1d3690b`. Status: active.

## Context

Each cell started with a random food value and could only ever lose it: a
creature standing on a cell with food ate one unit, a creature on an empty cell
lost a life point. Board food therefore decreased monotonically, and every run
ended the same way regardless of the probabilities — the board stripped itself
bare and everything starved. Interesting behaviour only appeared in the first
few dozen turns.

## Decision

After the starvation deaths are removed, every cell below a cap has an
independent chance to regain one unit of food. The cap is a constant,
`MAX_CELL_FOOD = 9`; the chance is configurable as
`food.regeneration_probability`, defaulting to
`DEFAULT_FOOD_REGEN_PROBABILITY = 0.1`. Regeneration is the seventh and last
phase of a turn, so food never regrows under a creature within the same turn
that the creature ate it.

## Consequences

The board gained a carrying capacity. Populations now oscillate — a dense
cluster strips its cells, starves back, and the cleared ground refills — which
is the behaviour the population chart in the GUI was later built to show.

Food became a third tunable alongside the two species, which is why the config
grew a `[food]` section and the GUI a third parameter group ("Environment").

The cap is a constant rather than a config key. Nine is the largest value the
CLI's single-character board output can render, so raising it would break that
display; the two are coupled on purpose.
