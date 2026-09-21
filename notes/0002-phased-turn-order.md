# 0002 — A turn is seven phases over a start-of-turn snapshot

Decided in `42ebf2f` (initial commit). Status: active.

## Context

Within one turn, creatures interact: an aphid moves into a cell holding a
ladybug, they fight, survivors breed, and everyone eats. Resolving all of that
per creature, one creature at a time, makes the outcome depend on iteration
order in ways that are hard to reason about. A creature born early in the turn
would immediately act; a creature killed in combat could still breed if its
turn came up first.

## Decision

A turn takes a snapshot of the creatures alive at its start, then runs seven
phases over that snapshot in a fixed order:

1. Movement
2. Combat
3. Removal of creatures killed in combat
4. Procreation
5. Starvation
6. Removal of creatures killed by starvation
7. Food regeneration for cells below the cap

Within each phase, creatures are processed in creation order. Creatures born
during a turn are not in the snapshot, so they do not act until the next turn.
Deaths are applied in their own phases, after the phase that caused them, so a
creature killed in combat cannot breed or starve later in the same turn.

## Consequences

The rules become individually testable: a test can set up a board, run one
turn, and know exactly which phase produced the result.

Simultaneity is only approximate. Movement still resolves creature by creature,
so a creature that moves early in the phase is seen at its new position by a
creature that moves later. Making movement truly simultaneous would need a
second board buffer; the current behaviour is accepted as close enough for the
scale of the simulation.

The order is load-bearing for reproducibility. Changing it silently invalidates
every seeded run, which is why it is pinned by the snapshot test described in
record [0001](0001-deterministic-seeded-simulation.md).
