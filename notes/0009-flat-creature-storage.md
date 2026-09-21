# 0009 — Creatures live in flat vectors indexed by ID

Decided in `39b5e6f`. Status: active.

## Context

With the benchmark from record
[0008](0008-benchmark-as-performance-guard.md) in place, the cost of a turn on
a large, crowded board became measurable. The expensive parts were not the
rules themselves but the bookkeeping around them: finding a creature's entry in
the cell it occupies, removing it, and allocating fresh collections for the
per-turn snapshot and the dead lists on every single turn.

## Decision

Creature state is stored in flat `Vec`s indexed by creature ID, and the board
caches the position of each creature *within* its cell:

- `cell_slots[id]` is the creature's index in its cell's `aphids` or `ladybugs`
  vector, so removing a creature is a `swap_remove` at a known index instead of
  a linear scan. The swap moves one other creature, whose cached slot is
  updated in the same step.
- `active_creatures`, `turn_creatures`, `dead_creatures`, and `death_marks` are
  owned by the board and cleared and reused each turn rather than allocated.
- `write_creature_snapshots` fills a caller-provided `Vec`, so the GUI can keep
  one buffer across frames; `creature_snapshots` remains as the allocating
  convenience wrapper.

## Consequences

A turn on the large benchmark board got substantially cheaper, and the
per-turn allocation traffic went to roughly zero.

The cost is an invariant that the type system does not enforce:
`cell_slots[id]` must always agree with where `id` actually sits in its cell.
Every add, move, and remove has to maintain it. The code defends this with
`debug_assert_eq!` in `swap_remove_occupant` rather than with types.

`swap_remove` does not preserve order within a cell, so the order in which
creatures within one cell are visited is now an implementation detail. Record
[0002](0002-phased-turn-order.md)'s guarantee is about creation order across
the turn snapshot, which is unaffected.
