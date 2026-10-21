//! Statistical check that the default rules still produce the same ecology.
//!
//! The seeded snapshots pin exact output, so they flag every change to the
//! random draws but cannot tell a refactor from a broken rule. This test runs
//! many seeds of the example config and checks the shape of the runs instead:
//! how much breeding happens, how large the population gets, how long both
//! species coexist, and which one usually dies out first. Each range is wider
//! than the spread between disjoint sets of seeds on the current rules, so a
//! change that only reorders random draws passes, while a change to how the
//! ecology behaves fails. See `notes/0024` for how the ranges were measured.

use creature_life_cycle::{Random, parse_simulation_config};
use std::ops::RangeInclusive;

/// Seeds per run of the test. With 50, the median births of two disjoint seed
/// sets on the same rules differed by a factor of two; with 200 the spread is
/// small enough to set useful ranges.
const SEEDS: u64 = 200;
/// Turn limit per run. The first species dies out by turn 400 in nearly every
/// run, and what follows is usually a few survivors wandering until they
/// starve, which can take thousands of turns and says little about the rules.
const MAX_TURNS: usize = 1000;
/// Population at which a run is treated as growing without limit and stopped.
/// The largest population any of 5,000 seeds reached is 150.
const POPULATION_LIMIT: usize = 300;

/// Ranges for the medians over `SEEDS` seeds. Across 25 disjoint sets of 200
/// seeds, median births ranged from 30 to 46, the median peak from 17 to 21,
/// and the median turn the first species died out from 79 to 95.
const MEDIAN_BIRTHS: RangeInclusive<usize> = 24..=60;
const MEDIAN_PEAK: RangeInclusive<usize> = 15..=24;
const MEDIAN_FIRST_EXTINCTION: RangeInclusive<usize> = 65..=115;
/// Runs, out of `SEEDS`, in which the aphids die out while ladybugs are still
/// alive. The same 25 sets ranged from 144 to 171.
const APHIDS_DIE_FIRST: RangeInclusive<usize> = 130..=185;

/// What one seeded run of the example config did.
struct Run {
    seed: u64,
    /// Creatures born over the whole run.
    births: usize,
    /// Most creatures alive at once, counting the starting ones.
    peak: usize,
    /// Turn on which the first species died out, or `MAX_TURNS` if neither did.
    first_extinction: usize,
    /// Whether the aphids died out on a turn that left ladybugs alive.
    aphids_died_first: bool,
}

/// Runs the example config from `seed` until both species are gone, the turn
/// limit is reached, or the population passes `POPULATION_LIMIT`.
fn run(seed: u64) -> Run {
    let mut random = Random::with_seed(seed);
    let mut board =
        parse_simulation_config(include_str!("../simulation.example.toml"), &mut random)
            .expect("example config is valid")
            .board;
    let start = board.summary();
    let mut run = Run {
        seed,
        births: 0,
        peak: start.aphids + start.ladybugs,
        first_extinction: MAX_TURNS,
        aphids_died_first: false,
    };

    for turn in 1..=MAX_TURNS {
        let stats = board.refresh(&mut random);
        let summary = stats.summary;
        let population = summary.aphids + summary.ladybugs;
        run.births += stats.births;
        run.peak = run.peak.max(population);
        if run.first_extinction == MAX_TURNS && (summary.aphids == 0 || summary.ladybugs == 0) {
            run.first_extinction = turn;
            run.aphids_died_first = summary.aphids == 0 && summary.ladybugs > 0;
        }
        if summary.is_extinct() || population > POPULATION_LIMIT {
            break;
        }
    }

    run
}

/// Returns the upper median, which for an even count is the larger of the two
/// middle values. It avoids averaging, so the result stays a count.
fn median(mut values: Vec<usize>) -> usize {
    values.sort_unstable();
    values[values.len() / 2]
}

#[test]
fn default_rules_keep_their_ecology_across_seeds() {
    let runs: Vec<Run> = (1..=SEEDS).map(run).collect();

    let runaway: Vec<u64> = runs
        .iter()
        .filter(|run| run.peak > POPULATION_LIMIT)
        .map(|run| run.seed)
        .collect();
    assert!(
        runaway.is_empty(),
        "population passed {POPULATION_LIMIT} on seeds {runaway:?}"
    );

    let births = median(runs.iter().map(|run| run.births).collect());
    let peak = median(runs.iter().map(|run| run.peak).collect());
    let first_extinction = median(runs.iter().map(|run| run.first_extinction).collect());
    let aphids_first = runs.iter().filter(|run| run.aphids_died_first).count();
    let measured = format!(
        "median births {births}, median peak {peak}, median first extinction at turn \
         {first_extinction}, aphids died first in {aphids_first} of {SEEDS} runs"
    );

    assert!(
        MEDIAN_BIRTHS.contains(&births),
        "births out of {MEDIAN_BIRTHS:?}: {measured}"
    );
    assert!(
        MEDIAN_PEAK.contains(&peak),
        "peak out of {MEDIAN_PEAK:?}: {measured}"
    );
    assert!(
        MEDIAN_FIRST_EXTINCTION.contains(&first_extinction),
        "first extinction out of {MEDIAN_FIRST_EXTINCTION:?}: {measured}"
    );
    assert!(
        APHIDS_DIE_FIRST.contains(&aphids_first),
        "aphids died first out of {APHIDS_DIE_FIRST:?}: {measured}"
    );
}
