# 0022 — Ladybugs regain life from the aphids they kill

Decided after a review of the simulation's dynamics. Status: active.

## Context

Both species ate the same plant food from their cell, and a ladybug that
killed an aphid only removed it. Predation was one-sided: aphids lost
population to ladybugs, but ladybugs gained nothing from hunting, so there was
no predator–prey coupling in the model at all.

Running the default board to extinction on seeds 1 to 50 showed how little
happened. The median run had 27 births and a peak of 16 creatures, against the
9 it starts with. Ten runs were still going at the 5,000-turn limit, typically
with one creature left wandering a board whose food it could never exhaust.

## Decision

A ladybug that kills an aphid regains life, set by `[ladybug] prey_life_gain`
and defaulting to `3`. The gain is applied in the combat phase, at the kill,
so it already counts in the starvation phase of the same turn. Life is capped
at `LADYBUG_START_LIFE = 15`, the life a starting ladybug has, so a ladybug in
a swarm cannot bank food for later. A ladybug above the cap, which only a
caller of `add_ladybug` can create, keeps its life rather than being cut down
by a meal. Aphids gain nothing from killing a ladybug.

The key is optional within `[ladybug]`, unlike the four probabilities there.
Making it required would have turned every config saved before the rule
existed into an error, which the CLI refuses to run (see 0015). Values outside
`0..=15` are rejected with a new `InvalidConfig::PreyLifeGain` variant: any
gain of `14` or more already refills a ladybug completely, so a larger number
is more likely a typo than an intent.

The gain is an integer, not a probability, so the GUI's parameters tab has no
control for it. `LadybugParams` carries it through the GUI unchanged and Save
setup writes it back out.

## Consequences

Feeding draws no random numbers, so the order and count of RNG calls is
unchanged. With `prey_life_gain = 0` every seed gives byte-for-byte the output
of the previous rules. With the default gain the output first diverges only
when a ladybug would otherwise have starved: at turn 27 on seed 8, and after
turn 100 on most other seeds that diverge at all. The end-to-end snapshots run
1 and 6 turns and stay unchanged. The rule is covered by unit tests instead,
including one that runs a full `refresh` in which a meal saves a ladybug at 1
life.

The effect on the default board is modest. Over the same 50 seeds, median
births rose from 27 to 56 and the median peak from 16 to 20, but run length
and the share of runs ladybugs outlast barely moved. Ladybugs still eat plant
food, which is plentiful, so they rarely come close to starving and the extra
life seldom matters. A trial in which ladybugs ate only aphids made predation
decisive, but ladybugs then died out first on all 50 seeds within a median
254 turns. That is a larger change to the model and was left for a separate
decision.

A randomized test now recounts the board's cached bookkeeping (cell occupant
lists, cell slots, the active list, and the summary) after every edit and turn
across 200 random boards. It was added alongside this rule because the rule
changes the combat phase, and a slip in the swap-remove bookkeeping of 0009
would otherwise show up only as an unexplained change in seeded output.
