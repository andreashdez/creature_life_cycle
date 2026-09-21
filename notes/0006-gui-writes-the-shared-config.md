# 0006 — The GUI edits the board and saves to the CLI's config file

Decided in `2f50845`. Status: active.

## Context

Setting up an interesting starting board meant editing coordinate pairs by hand
and re-running to see what they looked like. The GUI could already draw a board
but not change one, so the two halves of the workflow — arranging creatures and
watching them — lived in different tools.

## Decision

The GUI gained placement tools (aphid, ladybug, erase) that edit the live board,
and a **Save setup** action that serializes the current creature positions and
probabilities back to the same `simulation.toml` the CLI reads. Serialization
lives in the library as `format_simulation_config` and `save_configured_board`,
next to the parsing code, so writing and reading cannot drift apart.

Only the setup is saved. Food values, creature life values, and the current
turn are not, because the file describes a starting board, not a snapshot of a
run in progress.

## Consequences

The GUI became the editor for the CLI. A board arranged by clicking can be
replayed headless with `--seed`, which is the fast path for reproducing
something odd that was spotted visually.

Round-tripping is now a correctness requirement: whatever the GUI writes, the
parser has to read back into the same board.

Saving is explicit rather than automatic, so experimenting with sliders does
not overwrite a setup. The consequence is a second piece of state to track —
what is on screen versus what was last saved — which the GUI surfaces as the
bullet marks on changed parameters and as what **Reset** restores.
