# 0023 — A dead creature's ID is reused, from the next turn on

Decided after a review of the board's storage. Status: active. Amends
[0009](0009-flat-creature-storage.md), whose storage this keeps.

## Context

Record 0009 stores creatures in flat vectors indexed by ID. A new creature
always took the next ID, `creatures.len()`, and a dead one left `None` behind
for good. `creatures`, `cell_slots`, and `death_marks` therefore grew with every
creature ever born, not with how many were alive. Each of the GUI's 20 undo
snapshots clones the whole board, so the cost was paid again there.

At the default probabilities this never mattered: a typical run has a few dozen
births. It does matter once births are frequent. With aphid procreation at 0.6
and ladybug procreation at 0.5 on the default board, seed 1 reaches 2,440
aphids by turn 44 and keeps growing, adding over a thousand IDs a turn to a
table nothing ever shrank.

## Decision

The board keeps a list of free IDs. A creature that dies gives its ID to the
list, and a new creature takes one from it (the most recently freed) before the
table grows. The table is now as large as the most IDs in use at once.

IDs freed during `refresh` only become available when the turn ends. `refresh`
works over a snapshot of the IDs alive at the start of the turn. If a newborn
in phase 4 took the ID of a creature killed in phase 3, it would still be in
the snapshot under that ID and would starve in phase 5, although newborns do
not act until the next turn (0002). Edits through `remove_*_at` run between
turns, so they free the ID at once.

## Consequences

Seeded output is unchanged. Which ID a creature gets never decides what
happens: turn order comes from the active list, which records creation order
separately, and the order within a cell comes from the occupant lists. The
change was checked byte for byte against the previous build on 400 runs: seeds
1 to 100 on the example config, with prey life gains of 0 and 7, and with high
procreation for 40 turns, where IDs are reused hundreds of times a turn.

`CreatureSnapshot::id` is now stable only for a creature's life. The GUI's
`sync_creatures` matches sprites to creatures by ID once per frame, so it relies
on `step_simulation` running at most one turn per frame, which it already did.
With two turns per frame, a dead creature's sprite could glide to the newborn
that took its ID. The comment on `step_simulation` records this, so the limit
is not lifted without handling it.

Creation order can no longer be read off the IDs, so the test that checks the
board's bookkeeping compares the active list with the living creatures as a
set. The order itself is still checked by the seeded snapshots. Two tests
cover reuse: a newborn never takes an ID freed in its own turn, and the ID
table never grows past the most IDs in use at once.
