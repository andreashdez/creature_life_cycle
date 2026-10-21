# 0024 — A statistical test checks the default ecology

Decided after a review of the test suite. Status: active. Corrects the
measurements in [0022](0022-ladybugs-feed-on-aphids.md).

## Context

The seeded snapshots (0001) compare exact output, so they fail on any change
to the order or number of random draws, whether or not the rules changed. The
unit tests check one rule at a time on hand-built boards. Nothing checked that
the rules together still produce a working ecology: that creatures breed, that
both species meet before one dies out, and that the population settles instead
of growing without limit. A rule change could pass every test and still leave
the default board dead within a few turns, or flooded with aphids.

## Decision

`tests/default_ecology.rs` runs the example config on seeds 1 to 200 for up to
1,000 turns each and checks four medians and counts against ranges:

| Measure                                    | Range   | Spread over 25 seed sets |
| ------------------------------------------ | ------- | ------------------------ |
| Median births over a run                   | 24–60   | 30–46                    |
| Median peak population                     | 15–24   | 17–21                    |
| Median turn the first species dies out     | 65–115  | 79–95                    |
| Runs in which aphids die out first, of 200 | 130–185 | 144–171                  |

It also fails if any run passes 300 creatures. The largest population any of
5,000 seeds reached was 150, and a run that keeps growing would otherwise make
the test slow instead of failing it.

The spread column is the lowest and highest value over 25 disjoint sets of 200
seeds (seeds 1 to 5,000) on the current rules. Each range is wider than that
spread, so a change that only reorders random draws passes, while a change that
moves the ecology well outside its current shape fails. The test reads the
board through the public API only and takes about 3 seconds in a debug build.

The test does not use run length, which the first review proposed. Most runs
end with one or two creatures wandering until they starve, which takes from
under 100 turns to over 5,000. Over ten disjoint sets of 50 seeds run for up
to 5,000 turns, the median run length ranged from about 700 to 1,650 turns on
the same rules, so it cannot tell rules apart. The turn the first species
dies out is the stable measure of how long the two coexist.

Fifty seeds, as first proposed, are too few. Median births over the same ten
sets of 50 seeds ranged from 28.5 to 56.

## What it catches

Each of these, applied one at a time, fails the test:

- No procreation, no food regrowth, or creatures moving with probability 0.2
  instead of 0.7.
- Aphid procreation at 0.3 or 0.5 instead of 0.4, or food regrowth at 0.3
  instead of 0.1. The last two make the population grow without limit.
- Newborn aphids starting with 10 life instead of 2, or newborn ladybugs with
  15 instead of 5.
- Either species losing life every turn, fed or not, or creatures never eating.
- Creatures breeding alone instead of needing a partner in the cell.

It does not catch disabling kills, or setting `prey_life_gain` to 0. Both
change the medians by less than the spread between seed sets. Unit tests in
`src/lib.rs` cover both rules directly.

## Consequences

A change to the rules that is meant to change the ecology fails this test, as
it fails the snapshots. The ranges then need to be measured again over many
seed sets, not just moved to fit seeds 1 to 200.

The measurements in 0022 were taken on seeds 1 to 50, and the effect they
report is mostly the luck of that seed set. Over 1,000 seeds run to extinction,
the default gain of 3 raises median births from 36 to 42 and the median peak
from 18 to 19, against 27 to 56 and 16 to 20 in 0022. Its conclusion still
holds, more strongly: the rule's effect on the default board is small.
