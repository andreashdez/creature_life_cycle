//! Simulation library for the aphid and ladybug life-cycle model.
//!
//! The library owns the board state, creature rules, configuration loading/parsing, and random
//! number generator wrapper. The binaries own command-line parsing, rendering, and sleeping.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::Deserialize;
use std::fs;
use std::str::SplitWhitespace;

/// Zero-based board coordinate: `x` is the row and `y` is the column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coordinates {
    pub x: usize,
    pub y: usize,
}

/// One-step movement direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    /// No movement this turn.
    Same,
    NorthWest,
    North,
    NorthEast,
    West,
    East,
    SouthWest,
    South,
    SouthEast,
}

/// Creature discriminator used when a full creature value is not needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CreatureKind {
    Aphid,
    Ladybug,
}

/// Top-level runtime TOML configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulationConfig {
    /// Board dimensions and starting creatures.
    board: BoardConfig,
    /// Optional aphid behavior parameters. Defaults are used when omitted.
    aphid: Option<AphidConfig>,
    /// Optional ladybug behavior parameters. Defaults are used when omitted.
    ladybug: Option<LadybugConfig>,
}

/// TOML board configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoardConfig {
    /// Board row count.
    rows: usize,
    /// Board column count.
    columns: usize,
    /// Starting aphid positions.
    aphids: Vec<ConfigCoordinates>,
    /// Starting ladybug positions.
    ladybugs: Vec<ConfigCoordinates>,
}

/// TOML coordinate value.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigCoordinates {
    /// Zero-based row.
    x: usize,
    /// Zero-based column.
    y: usize,
}

/// TOML aphid behavior configuration.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AphidConfig {
    /// Chance that an aphid moves during movement phase.
    move_probability: f64,
    /// Base chance that an aphid kills a ladybug in the same cell.
    kill_probability: f64,
    /// Extra kill chance contributed by each other aphid in the same cell.
    accomplice_probability: f64,
    /// Chance that an aphid creates a child when sharing a cell with another aphid.
    procreation_probability: f64,
}

/// TOML ladybug behavior configuration.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LadybugConfig {
    /// Chance that a ladybug moves during movement phase.
    move_probability: f64,
    /// Chance that a ladybug kills an aphid in the same cell.
    kill_probability: f64,
    /// Chance that a ladybug chooses a new preferred direction set before moving.
    direction_change_probability: f64,
    /// Chance that a ladybug creates a child when sharing a cell with another ladybug.
    procreation_probability: f64,
}

/// One board cell.
///
/// Creature lists store IDs into `Board::creatures`, which keeps movement and removal cheap
/// without cloning whole creature values into every location.
#[derive(Clone, Debug)]
struct Location {
    /// Aphid IDs currently occupying this cell.
    aphids: Vec<usize>,
    /// Ladybug IDs currently occupying this cell.
    ladybugs: Vec<usize>,
    /// Food available in the cell. This can become negative as creatures consume it.
    food: i32,
}

/// Tunable aphid probabilities loaded from `simulation.toml`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AphidParams {
    /// Chance that an aphid moves during movement phase.
    pub prob_move: f64,
    /// Base chance that an aphid kills a ladybug in the same cell.
    pub prob_kill: f64,
    /// Extra kill chance contributed by each other aphid in the same cell.
    pub prob_accomplice: f64,
    /// Chance that an aphid creates a child when sharing a cell with another aphid.
    pub prob_procreate: f64,
}

impl Default for AphidParams {
    fn default() -> Self {
        Self {
            prob_move: 0.7,
            prob_kill: 0.2,
            prob_accomplice: 0.1,
            prob_procreate: 0.4,
        }
    }
}

/// Tunable ladybug probabilities loaded from `simulation.toml`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LadybugParams {
    /// Chance that a ladybug moves during movement phase.
    pub prob_move: f64,
    /// Chance that a ladybug kills an aphid in the same cell.
    pub prob_kill: f64,
    /// Chance that a ladybug chooses a new preferred direction set before moving.
    pub prob_direction: f64,
    /// Chance that a ladybug creates a child when sharing a cell with another ladybug.
    pub prob_procreate: f64,
}

impl Default for LadybugParams {
    fn default() -> Self {
        Self {
            prob_move: 0.7,
            prob_kill: 0.2,
            prob_direction: 0.4,
            prob_procreate: 0.2,
        }
    }
}

/// Current board totals for human-readable summaries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoardSummary {
    /// Number of living aphids.
    pub aphids: usize,
    /// Number of living ladybugs.
    pub ladybugs: usize,
    /// Sum of food across all cells.
    pub food: i32,
}

impl BoardSummary {
    /// Returns whether no living creatures remain.
    pub fn is_extinct(&self) -> bool {
        self.aphids == 0 && self.ladybugs == 0
    }
}

/// Read-only cell data for rendering or reporting board state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSnapshot {
    /// Aphids currently occupying this cell.
    pub aphids: usize,
    /// Ladybugs currently occupying this cell.
    pub ladybugs: usize,
    /// Food available in this cell.
    pub food: i32,
}

/// Public creature discriminator for read-only board snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureSnapshotKind {
    /// Aphid creature.
    Aphid,
    /// Ladybug creature.
    Ladybug,
}

/// Read-only living creature data for renderers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureSnapshot {
    /// Stable creature ID.
    pub id: usize,
    /// Creature kind.
    pub kind: CreatureSnapshotKind,
    /// Current board coordinate.
    pub location: Coordinates,
}

/// Changes and totals produced by one completed turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurnStats {
    /// Creatures born during this turn.
    pub births: usize,
    /// Creatures killed by combat or starvation during this turn.
    pub deaths: usize,
    /// Board totals after all turn phases finish.
    pub summary: BoardSummary,
}

/// Aphid state.
#[derive(Clone, Debug)]
struct Aphid {
    /// Current board coordinate.
    location: Coordinates,
    /// Remaining life. The aphid dies when this drops below one.
    life: i32,
}

/// Ladybug state.
#[derive(Clone, Debug)]
struct Ladybug {
    /// Current board coordinate.
    location: Coordinates,
    /// Remaining life. The ladybug dies when this drops below one.
    life: i32,
    /// Three directions currently favored by movement.
    preferred_directions: [Direction; 3],
}

impl Ladybug {
    /// Creates a ladybug with a random initial preferred direction set.
    fn new(location: Coordinates, life: i32, random: &mut Random) -> Self {
        let mut ladybug = Self {
            location,
            life,
            preferred_directions: [Direction::North, Direction::East, Direction::South],
        };
        ladybug.change_direction(random);
        ladybug
    }

    /// Picks one of the four cardinal direction bands used for ladybug movement.
    fn change_direction(&mut self, random: &mut Random) {
        self.preferred_directions = match random.int_inclusive(0, 3) {
            0 => [Direction::NorthWest, Direction::North, Direction::NorthEast],
            1 => [Direction::SouthWest, Direction::West, Direction::NorthWest],
            2 => [Direction::NorthEast, Direction::East, Direction::SouthEast],
            _ => [Direction::SouthEast, Direction::South, Direction::SouthWest],
        };
    }
}

/// Stored creature value.
///
/// The board stores `Option<Creature>` so IDs stay stable after deaths. A dead creature becomes
/// `None` instead of shifting every later ID.
#[derive(Clone, Debug)]
enum Creature {
    Aphid(Aphid),
    Ladybug(Ladybug),
}

impl Creature {
    /// Returns the creature kind without exposing variant internals.
    fn kind(&self) -> CreatureKind {
        match self {
            Creature::Aphid(_) => CreatureKind::Aphid,
            Creature::Ladybug(_) => CreatureKind::Ladybug,
        }
    }

    /// Returns the current board coordinate.
    fn location(&self) -> Coordinates {
        match self {
            Creature::Aphid(aphid) => aphid.location,
            Creature::Ladybug(ladybug) => ladybug.location,
        }
    }

    /// Updates the creature coordinate after movement.
    fn set_location(&mut self, location: Coordinates) {
        match self {
            Creature::Aphid(aphid) => aphid.location = location,
            Creature::Ladybug(ladybug) => ladybug.location = location,
        }
    }
}

/// Complete simulation state.
pub struct Board {
    /// Number of board rows.
    rows: usize,
    /// Number of board columns.
    cols: usize,
    /// Row-major board cells.
    field: Vec<Location>,
    /// Stable creature ID table. Dead creatures are stored as `None`.
    creatures: Vec<Option<Creature>>,
    /// Active aphid behavior parameters.
    aphid_params: AphidParams,
    /// Active ladybug behavior parameters.
    ladybug_params: LadybugParams,
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    /// Creates an empty board. `parse_board` or `load_standard_data` must initialize the field.
    pub fn new() -> Self {
        Self {
            rows: 0,
            cols: 0,
            field: Vec::new(),
            creatures: Vec::new(),
            aphid_params: AphidParams::default(),
            ladybug_params: LadybugParams::default(),
        }
    }

    /// Sets active aphid behavior parameters.
    pub fn set_aphid_params(&mut self, params: AphidParams) {
        self.aphid_params = params;
    }

    /// Sets active ladybug behavior parameters.
    pub fn set_ladybug_params(&mut self, params: LadybugParams) {
        self.ladybug_params = params;
    }

    /// Returns active aphid behavior parameters.
    pub fn aphid_params(&self) -> AphidParams {
        self.aphid_params
    }

    /// Returns active ladybug behavior parameters.
    pub fn ladybug_params(&self) -> LadybugParams {
        self.ladybug_params
    }

    /// Returns the number of board rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Returns the number of board columns.
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Returns the aphid and ladybug counts for a cell.
    pub fn cell_counts(&self, x: usize, y: usize) -> Option<(usize, usize)> {
        let coordinates = Coordinates { x, y };
        self.in_bounds(coordinates).then(|| {
            let location = &self.field[self.index(coordinates)];
            (location.aphids.len(), location.ladybugs.len())
        })
    }

    /// Returns read-only rendering data for a cell.
    pub fn cell_snapshot(&self, x: usize, y: usize) -> Option<CellSnapshot> {
        let coordinates = Coordinates { x, y };
        self.in_bounds(coordinates).then(|| {
            let location = &self.field[self.index(coordinates)];
            CellSnapshot {
                aphids: location.aphids.len(),
                ladybugs: location.ladybugs.len(),
                food: location.food,
            }
        })
    }

    /// Returns read-only data for every living creature in stable ID order.
    pub fn creature_snapshots(&self) -> Vec<CreatureSnapshot> {
        self.creatures
            .iter()
            .enumerate()
            .filter_map(|(id, creature)| {
                creature.as_ref().map(|creature| CreatureSnapshot {
                    id,
                    kind: match creature.kind() {
                        CreatureKind::Aphid => CreatureSnapshotKind::Aphid,
                        CreatureKind::Ladybug => CreatureSnapshotKind::Ladybug,
                    },
                    location: creature.location(),
                })
            })
            .collect()
    }

    /// Creates a new board field and assigns random starting food to each cell.
    fn create_field(&mut self, rows: usize, cols: usize, random: &mut Random) {
        self.rows = rows;
        self.cols = cols;
        self.field.clear();
        self.creatures.clear();

        for _ in 0..rows {
            for _ in 0..cols {
                self.field.push(Location {
                    aphids: Vec::new(),
                    ladybugs: Vec::new(),
                    food: random.int_inclusive(0, 9),
                });
            }
        }
    }

    /// Adds an aphid if the coordinate is valid and returns its stable ID.
    pub fn add_aphid(&mut self, x: usize, y: usize, life: i32) -> Option<usize> {
        let location = Coordinates { x, y };
        if !self.in_bounds(location) {
            eprintln!("Aphid position {x} {y} is outside the board; skipping.");
            return None;
        }

        let id = self.creatures.len();
        let index = self.index(location);
        self.field[index].aphids.push(id);
        self.creatures
            .push(Some(Creature::Aphid(Aphid { location, life })));
        Some(id)
    }

    /// Adds a ladybug if the coordinate is valid and returns its stable ID.
    pub fn add_ladybug(
        &mut self,
        x: usize,
        y: usize,
        life: i32,
        random: &mut Random,
    ) -> Option<usize> {
        let location = Coordinates { x, y };
        if !self.in_bounds(location) {
            eprintln!("Ladybug position {x} {y} is outside the board; skipping.");
            return None;
        }

        let id = self.creatures.len();
        let index = self.index(location);
        self.field[index].ladybugs.push(id);
        self.creatures.push(Some(Creature::Ladybug(Ladybug::new(
            location, life, random,
        ))));
        Some(id)
    }

    /// Advances the board by one phased turn.
    ///
    /// The turn snapshot prevents newly born creatures from acting immediately. Combat deaths are
    /// applied before procreation and starvation so killed creatures do not continue acting later
    /// in the same turn.
    pub fn refresh(&mut self, random: &mut Random) -> TurnStats {
        let turn_creatures: Vec<usize> = self
            .creatures
            .iter()
            .enumerate()
            .filter_map(|(id, creature)| creature.as_ref().map(|_| id))
            .collect();
        let mut stats = TurnStats::default();

        // Phase 1: all original creatures move before any combat or births happen.
        for id in turn_creatures.iter().copied() {
            self.creature_movement(id, random);
        }

        let mut dead_creatures = Vec::new();

        // Phase 2: combat is resolved for survivors from the original turn snapshot.
        for id in turn_creatures.iter().copied() {
            if dead_creatures.contains(&id)
                || self.creatures.get(id).and_then(Option::as_ref).is_none()
            {
                continue;
            }

            if let Some(killed) = self.creature_combat(id, random) {
                push_unique(&mut dead_creatures, killed);
            }
        }

        // Phase 3: combat deaths are applied before procreation and starvation.
        stats.deaths += dead_creatures.len();
        for id in dead_creatures.drain(..) {
            self.remove_creature(id);
        }

        // Phase 4: survivors from the original turn snapshot may procreate.
        for id in turn_creatures.iter().copied() {
            if self.creatures.get(id).and_then(Option::as_ref).is_none() {
                continue;
            }

            stats.births += self.creature_procreation(id, random);
        }

        // Phase 5: survivors from the original turn snapshot consume food and may starve.
        for id in turn_creatures.iter().copied() {
            if self.creatures.get(id).and_then(Option::as_ref).is_none() {
                continue;
            }

            if let Some(starved) = self.creature_starvation(id) {
                push_unique(&mut dead_creatures, starved);
            }
        }

        // Phase 6: starvation deaths are applied after every survivor had a starvation turn.
        stats.deaths += dead_creatures.len();
        for id in dead_creatures {
            self.remove_creature(id);
        }

        stats.summary = self.summary();
        stats
    }

    /// Runs one creature's movement action for the movement phase.
    fn creature_movement(&mut self, id: usize, random: &mut Random) {
        let aphid_params = self.aphid_params;
        let ladybug_params = self.ladybug_params;

        let Some(creature) = self.creatures.get_mut(id).and_then(Option::as_mut) else {
            return;
        };

        let kind = creature.kind();
        let old_location = creature.location();
        let direction = match creature {
            Creature::Aphid(_) => {
                if random.probability() < aphid_params.prob_move {
                    match random.int_inclusive(0, 7) {
                        0 => Direction::NorthWest,
                        1 => Direction::North,
                        2 => Direction::NorthEast,
                        3 => Direction::West,
                        4 => Direction::East,
                        5 => Direction::SouthWest,
                        6 => Direction::South,
                        _ => Direction::SouthEast,
                    }
                } else {
                    Direction::Same
                }
            }
            Creature::Ladybug(ladybug) => {
                if random.probability() < ladybug_params.prob_direction {
                    ladybug.change_direction(random);
                }

                if random.probability() < ladybug_params.prob_move {
                    ladybug.preferred_directions[random.int_inclusive(0, 2) as usize]
                } else {
                    Direction::Same
                }
            }
        };

        if direction == Direction::Same {
            return;
        }

        let new_location = self.direction_to_location(direction, old_location);
        self.remove_from_location(kind, old_location, id);
        self.add_to_location(kind, new_location, id);

        if let Some(creature) = self.creatures.get_mut(id).and_then(Option::as_mut) {
            creature.set_location(new_location);
        }
    }

    /// Runs one creature's combat action and returns the killed creature ID, if any.
    fn creature_combat(&mut self, id: usize, random: &mut Random) -> Option<usize> {
        let creature = self.creatures.get(id).and_then(Option::as_ref)?;
        let location = creature.location();
        let index = self.index(location);

        match creature.kind() {
            CreatureKind::Aphid => {
                if self.field[index].ladybugs.is_empty() {
                    return None;
                }

                let accomplices = self.field[index].aphids.len().saturating_sub(1) as f64;
                let prob_kill = (self.aphid_params.prob_kill
                    + accomplices * self.aphid_params.prob_accomplice)
                    .min(1.0);

                if random.probability() < prob_kill {
                    self.field[index].ladybugs.pop()
                } else {
                    None
                }
            }
            CreatureKind::Ladybug => {
                if self.field[index].aphids.is_empty() {
                    return None;
                }

                if random.probability() < self.ladybug_params.prob_kill {
                    self.field[index].aphids.pop()
                } else {
                    None
                }
            }
        }
    }

    /// Runs one creature's procreation action and returns the number of births.
    fn creature_procreation(&mut self, id: usize, random: &mut Random) -> usize {
        let Some(creature) = self.creatures.get(id).and_then(Option::as_ref) else {
            return 0;
        };

        let location = creature.location();
        let index = self.index(location);

        match creature.kind() {
            CreatureKind::Aphid => {
                if self.field[index].aphids.len() > 1
                    && random.probability() < self.aphid_params.prob_procreate
                {
                    usize::from(self.add_aphid(location.x, location.y, 2).is_some())
                } else {
                    0
                }
            }
            CreatureKind::Ladybug => {
                if self.field[index].ladybugs.len() > 1
                    && random.probability() < self.ladybug_params.prob_procreate
                {
                    usize::from(
                        self.add_ladybug(location.x, location.y, 5, random)
                            .is_some(),
                    )
                } else {
                    0
                }
            }
        }
    }

    /// Runs one creature's starvation action and returns its ID if it dies.
    fn creature_starvation(&mut self, id: usize) -> Option<usize> {
        let creature = self.creatures.get(id).and_then(Option::as_ref)?;
        let kind = creature.kind();
        let location = creature.location();
        let index = self.index(location);

        self.field[index].food -= 1;
        let food = self.field[index].food;

        let creature = self.creatures.get_mut(id).and_then(Option::as_mut)?;
        let starved = match creature {
            Creature::Aphid(aphid) => {
                if food > 0 {
                    aphid.life += 1;
                }
                aphid.life -= 1;
                aphid.life < 1
            }
            Creature::Ladybug(ladybug) => {
                if food > 0 {
                    ladybug.life += 1;
                }
                ladybug.life -= 1;
                ladybug.life < 1
            }
        };

        if starved {
            self.remove_from_location(kind, location, id);
            Some(id)
        } else {
            None
        }
    }

    /// Removes a creature from its current location and marks its stable ID as dead.
    fn remove_creature(&mut self, id: usize) {
        let Some(creature) = self.creatures.get(id).and_then(Option::as_ref) else {
            return;
        };

        self.remove_from_location(creature.kind(), creature.location(), id);
        self.creatures[id] = None;
    }

    /// Adds an existing creature ID to a location list for its kind.
    fn add_to_location(&mut self, kind: CreatureKind, location: Coordinates, id: usize) {
        let index = self.index(location);
        match kind {
            CreatureKind::Aphid => self.field[index].aphids.push(id),
            CreatureKind::Ladybug => self.field[index].ladybugs.push(id),
        }
    }

    /// Removes an existing creature ID from a location list for its kind.
    fn remove_from_location(&mut self, kind: CreatureKind, location: Coordinates, id: usize) {
        let index = self.index(location);
        match kind {
            CreatureKind::Aphid => self.field[index].aphids.retain(|creature| *creature != id),
            CreatureKind::Ladybug => self.field[index]
                .ladybugs
                .retain(|creature| *creature != id),
        }
    }

    /// Converts a movement direction into a valid destination coordinate.
    fn direction_to_location(&self, direction: Direction, coordinates: Coordinates) -> Coordinates {
        let mut x = coordinates.x as isize;
        let mut y = coordinates.y as isize;

        match direction {
            Direction::NorthWest => {
                x -= 1;
                y -= 1;
            }
            Direction::North => x -= 1,
            Direction::NorthEast => {
                x -= 1;
                y += 1;
            }
            Direction::West => y -= 1,
            Direction::East => y += 1,
            Direction::SouthWest => {
                x += 1;
                y -= 1;
            }
            Direction::South => x += 1,
            Direction::SouthEast => {
                x += 1;
                y += 1;
            }
            Direction::Same => {}
        }

        self.in_boundaries(&mut x, &mut y);
        Coordinates {
            x: x as usize,
            y: y as usize,
        }
    }

    /// Reflects out-of-bounds coordinates back onto the board and clamps edge cases.
    fn in_boundaries(&self, x: &mut isize, y: &mut isize) {
        let max_x = self.rows.saturating_sub(1) as isize;
        let max_y = self.cols.saturating_sub(1) as isize;

        if *x < 0 {
            *x += 2;
        } else if *x > max_x {
            *x -= 2;
        }

        if *y < 0 {
            *y += 2;
        } else if *y > max_y {
            *y -= 2;
        }

        *x = (*x).clamp(0, max_x);
        *y = (*y).clamp(0, max_y);
    }

    /// Returns current living creature counts and total board food.
    pub fn summary(&self) -> BoardSummary {
        let mut summary = BoardSummary {
            food: self.field.iter().map(|location| location.food).sum(),
            ..BoardSummary::default()
        };

        for creature in self.creatures.iter().flatten() {
            match creature {
                Creature::Aphid(_) => summary.aphids += 1,
                Creature::Ladybug(_) => summary.ladybugs += 1,
            }
        }

        summary
    }

    /// Converts a two-dimensional coordinate into the row-major field index.
    fn index(&self, coordinates: Coordinates) -> usize {
        coordinates.x * self.cols + coordinates.y
    }

    /// Returns whether a coordinate is inside the board.
    fn in_bounds(&self, coordinates: Coordinates) -> bool {
        coordinates.x < self.rows && coordinates.y < self.cols
    }
}

/// Random number generator wrapper used by the simulation.
///
/// `StdRng` gives deterministic seeded runs while `from_os_rng` provides non-deterministic default
/// runs from the operating system.
pub struct Random {
    /// Internal generator from the `rand` crate.
    rng: StdRng,
}

impl Default for Random {
    fn default() -> Self {
        Self::new()
    }
}

impl Random {
    /// Creates a generator seeded from operating system randomness.
    pub fn new() -> Self {
        Self {
            rng: StdRng::from_os_rng(),
        }
    }

    /// Creates a generator from an explicit seed.
    pub fn with_seed(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Returns a random floating-point value in `[0.0, 1.0)`.
    fn probability(&mut self) -> f64 {
        self.rng.random()
    }

    /// Returns a random integer in the inclusive range `min..=max`.
    fn int_inclusive(&mut self, min: i32, max: i32) -> i32 {
        self.rng.random_range(min..=max)
    }
}

/// Parses legacy whitespace board dimensions and starting creature positions.
pub fn parse_board(contents: &str, board: &mut Board, random: &mut Random) -> Result<(), String> {
    let mut tokens = contents.split_whitespace();
    let rows: usize = parse_next(&mut tokens, "row count")?;
    let cols: usize = parse_next(&mut tokens, "column count")?;

    if rows == 0 || cols == 0 {
        return Err("board dimensions must be greater than zero".to_string());
    }

    let aphid_count: usize = parse_next(&mut tokens, "aphid count")?;
    let mut aphids = Vec::with_capacity(aphid_count);
    for _ in 0..aphid_count {
        let x = parse_next(&mut tokens, "aphid x coordinate")?;
        let y = parse_next(&mut tokens, "aphid y coordinate")?;
        aphids.push((x, y));
    }

    let ladybug_count: usize = parse_next(&mut tokens, "ladybug count")?;
    let mut ladybugs = Vec::with_capacity(ladybug_count);
    for _ in 0..ladybug_count {
        let x = parse_next(&mut tokens, "ladybug x coordinate")?;
        let y = parse_next(&mut tokens, "ladybug y coordinate")?;
        ladybugs.push((x, y));
    }

    reject_trailing_tokens(&mut tokens, "board data")?;

    board.create_field(rows, cols, random);

    for (x, y) in aphids {
        board.add_aphid(x, y, 10);
    }

    for (x, y) in ladybugs {
        board.add_ladybug(x, y, 15, random);
    }

    Ok(())
}

/// Parses legacy whitespace aphid probabilities.
pub fn parse_aphid_params(contents: &str) -> Result<AphidParams, String> {
    let mut tokens = contents.split_whitespace();
    let params = AphidParams {
        prob_move: parse_probability(&mut tokens, "aphid move probability")?,
        prob_kill: parse_probability(&mut tokens, "aphid kill probability")?,
        prob_accomplice: parse_probability(&mut tokens, "aphid accomplice probability")?,
        prob_procreate: parse_probability(&mut tokens, "aphid procreation probability")?,
    };
    reject_trailing_tokens(&mut tokens, "aphid data")?;
    Ok(params)
}

/// Parses legacy whitespace ladybug probabilities.
pub fn parse_ladybug_params(contents: &str) -> Result<LadybugParams, String> {
    let mut tokens = contents.split_whitespace();
    let params = LadybugParams {
        prob_move: parse_probability(&mut tokens, "ladybug move probability")?,
        prob_kill: parse_probability(&mut tokens, "ladybug kill probability")?,
        prob_direction: parse_probability(&mut tokens, "ladybug direction probability")?,
        prob_procreate: parse_probability(&mut tokens, "ladybug procreation probability")?,
    };
    reject_trailing_tokens(&mut tokens, "ladybug data")?;
    Ok(params)
}

/// Parses TOML runtime configuration and initializes a board from it.
pub fn parse_simulation_config(
    contents: &str,
    board: &mut Board,
    random: &mut Random,
) -> Result<(), String> {
    let config: SimulationConfig = toml::from_str(contents).map_err(|error| error.to_string())?;
    let aphid_params = config
        .aphid
        .map(AphidConfig::params)
        .transpose()?
        .unwrap_or_default();
    let ladybug_params = config
        .ladybug
        .map(LadybugConfig::params)
        .transpose()?
        .unwrap_or_default();

    if config.board.rows == 0 || config.board.columns == 0 {
        return Err("board dimensions must be greater than zero".to_string());
    }

    board.create_field(config.board.rows, config.board.columns, random);

    for coordinates in config.board.aphids {
        board.add_aphid(coordinates.x, coordinates.y, 10);
    }

    for coordinates in config.board.ladybugs {
        board.add_ladybug(coordinates.x, coordinates.y, 15, random);
    }

    board.set_aphid_params(aphid_params);
    board.set_ladybug_params(ladybug_params);

    Ok(())
}

impl AphidConfig {
    /// Converts TOML values into validated aphid parameters.
    fn params(self) -> Result<AphidParams, String> {
        Ok(AphidParams {
            prob_move: validate_probability(self.move_probability, "aphid move probability")?,
            prob_kill: validate_probability(self.kill_probability, "aphid kill probability")?,
            prob_accomplice: validate_probability(
                self.accomplice_probability,
                "aphid accomplice probability",
            )?,
            prob_procreate: validate_probability(
                self.procreation_probability,
                "aphid procreation probability",
            )?,
        })
    }
}

impl LadybugConfig {
    /// Converts TOML values into validated ladybug parameters.
    fn params(self) -> Result<LadybugParams, String> {
        Ok(LadybugParams {
            prob_move: validate_probability(self.move_probability, "ladybug move probability")?,
            prob_kill: validate_probability(self.kill_probability, "ladybug kill probability")?,
            prob_direction: validate_probability(
                self.direction_change_probability,
                "ladybug direction probability",
            )?,
            prob_procreate: validate_probability(
                self.procreation_probability,
                "ladybug procreation probability",
            )?,
        })
    }
}

/// Loads the runtime TOML configuration file from the current working directory.
///
/// Invalid or missing `simulation.toml` falls back to built-in standard data and default behavior
/// parameters.
pub fn load_configured_board(random: &mut Random) -> Board {
    let mut board = Board::new();

    read_simulation_config(&mut board, random);

    board
}

/// Reads `simulation.toml`, falling back to built-in standard data on errors.
fn read_simulation_config(board: &mut Board, random: &mut Random) {
    let Ok(contents) = fs::read_to_string("simulation.toml") else {
        eprintln!("File \"simulation.toml\" not found, using standard data.");
        load_standard_data(board, random);
        return;
    };

    if let Err(error) = parse_simulation_config(&contents, board, random) {
        eprintln!("Invalid simulation.toml ({error}), using standard data.");
        load_standard_data(board, random);
    }
}

/// Loads the same default board data represented by the checked-in `simulation.toml`.
pub fn load_standard_data(board: &mut Board, random: &mut Random) {
    board.create_field(10, 10, random);

    board.add_aphid(3, 5, 10);
    board.add_aphid(4, 8, 10);
    board.add_aphid(2, 9, 10);
    board.add_aphid(1, 6, 10);
    board.add_aphid(1, 9, 10);

    board.add_ladybug(5, 9, 15, random);
    board.add_ladybug(1, 1, 15, random);
    board.add_ladybug(3, 8, 15, random);
    board.add_ladybug(9, 2, 15, random);
}

/// Parses the next whitespace-delimited token with an error label for diagnostics.
fn parse_next<T>(tokens: &mut SplitWhitespace<'_>, label: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    tokens
        .next()
        .ok_or_else(|| format!("missing {label}"))?
        .parse()
        .map_err(|_| format!("invalid {label}"))
}

/// Parses the next token as a probability and validates it is in `0.0..=1.0`.
fn parse_probability(tokens: &mut SplitWhitespace<'_>, label: &str) -> Result<f64, String> {
    let value = parse_next(tokens, label)?;
    validate_probability(value, label)
}

/// Validates a probability value is in `0.0..=1.0`.
fn validate_probability(value: f64, label: &str) -> Result<f64, String> {
    if (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(format!("{label} must be between 0 and 1"))
    }
}

/// Rejects extra config tokens after all expected values have been parsed.
fn reject_trailing_tokens(tokens: &mut SplitWhitespace<'_>, source: &str) -> Result<(), String> {
    if let Some(token) = tokens.next() {
        Err(format!("unexpected trailing token in {source}: {token}"))
    } else {
        Ok(())
    }
}

/// Appends a value only if it is not already present.
fn push_unique(values: &mut Vec<usize>, value: usize) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board_with_field(rows: usize, cols: usize) -> Board {
        let mut random = Random::with_seed(1);
        let mut board = Board::new();
        board.create_field(rows, cols, &mut random);
        board
    }

    #[test]
    fn seeded_random_is_reproducible() {
        let mut first = Random::with_seed(42);
        let mut second = Random::with_seed(42);

        for _ in 0..10 {
            assert_eq!(first.probability(), second.probability());
        }
    }

    #[test]
    fn parse_board_loads_creatures() {
        let mut random = Random::with_seed(1);
        let mut board = Board::new();

        parse_board("2 3 1 0 1 1 1 2", &mut board, &mut random).unwrap();

        assert_eq!(board.rows, 2);
        assert_eq!(board.cols, 3);
        assert_eq!(board.creatures.len(), 2);
        assert_eq!(
            board.field[board.index(Coordinates { x: 0, y: 1 })]
                .aphids
                .len(),
            1
        );
        assert_eq!(
            board.field[board.index(Coordinates { x: 1, y: 2 })]
                .ladybugs
                .len(),
            1
        );
    }

    #[test]
    fn parse_simulation_config_loads_board_and_params() {
        let mut random = Random::with_seed(1);
        let mut board = Board::new();

        parse_simulation_config(
            r#"
[board]
rows = 2
columns = 3
aphids = [{ x = 0, y = 1 }]
ladybugs = [{ x = 1, y = 2 }]

[aphid]
move_probability = 0.6
kill_probability = 0.3
accomplice_probability = 0.2
procreation_probability = 0.5

[ladybug]
move_probability = 0.4
kill_probability = 0.8
direction_change_probability = 0.7
procreation_probability = 0.1
"#,
            &mut board,
            &mut random,
        )
        .unwrap();

        assert_eq!(board.rows, 2);
        assert_eq!(board.cols, 3);
        assert_eq!(board.creatures.len(), 2);
        assert_eq!(board.aphid_params.prob_move, 0.6);
        assert_eq!(board.aphid_params.prob_accomplice, 0.2);
        assert_eq!(board.ladybug_params.prob_kill, 0.8);
        assert_eq!(board.ladybug_params.prob_direction, 0.7);
        assert_eq!(
            board.field[board.index(Coordinates { x: 0, y: 1 })]
                .aphids
                .len(),
            1
        );
        assert_eq!(
            board.field[board.index(Coordinates { x: 1, y: 2 })]
                .ladybugs
                .len(),
            1
        );
    }

    #[test]
    fn parse_probability_configs_reject_out_of_range_values() {
        assert!(parse_aphid_params("0.7 1.2 0.1 0.4").is_err());
        assert!(parse_ladybug_params("0.7 0.2 -0.1 0.2").is_err());

        let mut random = Random::with_seed(1);
        let mut board = Board::new();
        assert!(
            parse_simulation_config(
                r#"
[board]
rows = 2
columns = 2
aphids = []
ladybugs = []

[aphid]
move_probability = 1.2
kill_probability = 0.2
accomplice_probability = 0.1
procreation_probability = 0.4
"#,
                &mut board,
                &mut random,
            )
            .is_err()
        );
    }

    #[test]
    fn config_parsers_reject_trailing_tokens() {
        let mut random = Random::with_seed(1);
        let mut board = Board::new();

        assert!(parse_board("2 2 0 0 extra", &mut board, &mut random).is_err());
        assert_eq!(board.rows, 0);
        assert_eq!(board.cols, 0);
        assert!(parse_aphid_params("0.7 0.2 0.1 0.4 extra").is_err());
        assert!(parse_ladybug_params("0.7 0.2 0.4 0.2 extra").is_err());
    }

    #[test]
    fn edge_movement_bounces_back_into_board() {
        let board = board_with_field(3, 3);

        assert_eq!(
            board.direction_to_location(Direction::NorthWest, Coordinates { x: 0, y: 0 }),
            Coordinates { x: 1, y: 1 }
        );
        assert_eq!(
            board.direction_to_location(Direction::SouthEast, Coordinates { x: 2, y: 2 }),
            Coordinates { x: 1, y: 1 }
        );
    }

    #[test]
    fn ladybug_combat_kills_aphid_when_probability_succeeds() {
        let mut board = board_with_field(1, 1);
        let aphid = board.add_aphid(0, 0, 10).unwrap();
        let mut random = Random::with_seed(2);
        let ladybug = board.add_ladybug(0, 0, 15, &mut random).unwrap();
        board.ladybug_params.prob_kill = 1.0;

        assert_eq!(board.creature_combat(ladybug, &mut random), Some(aphid));
        assert!(
            board.field[board.index(Coordinates { x: 0, y: 0 })]
                .aphids
                .is_empty()
        );
    }

    #[test]
    fn lone_aphid_does_not_get_accomplice_bonus() {
        let mut board = board_with_field(1, 1);
        let aphid = board.add_aphid(0, 0, 10).unwrap();
        let mut random = Random::with_seed(2);
        let ladybug = board.add_ladybug(0, 0, 15, &mut random).unwrap();
        board.aphid_params.prob_kill = 0.0;
        board.aphid_params.prob_accomplice = 1.0;

        assert_eq!(board.creature_combat(aphid, &mut random), None);
        assert_eq!(
            board.field[board.index(Coordinates { x: 0, y: 0 })].ladybugs,
            vec![ladybug]
        );
    }

    #[test]
    fn refresh_resolves_combat_before_procreation() {
        let mut board = board_with_field(1, 1);
        let location = Coordinates { x: 0, y: 0 };
        let index = board.index(location);
        board.field[index].food = 100;

        board.add_aphid(0, 0, 10).unwrap();
        board.add_aphid(0, 0, 10).unwrap();
        let mut random = Random::with_seed(4);
        board.add_ladybug(0, 0, 15, &mut random).unwrap();

        board.aphid_params = AphidParams {
            prob_move: 0.0,
            prob_kill: 0.0,
            prob_accomplice: 0.0,
            prob_procreate: 1.0,
        };
        board.ladybug_params = LadybugParams {
            prob_move: 0.0,
            prob_kill: 1.0,
            prob_direction: 0.0,
            prob_procreate: 0.0,
        };

        let stats = board.refresh(&mut random);

        assert_eq!(board.field[index].aphids.len(), 1);
        assert_eq!(board.field[index].ladybugs.len(), 1);
        assert_eq!(board.creatures.iter().flatten().count(), 2);
        assert_eq!(
            stats,
            TurnStats {
                births: 0,
                deaths: 1,
                summary: BoardSummary {
                    aphids: 1,
                    ladybugs: 1,
                    food: 98
                }
            }
        );
    }

    #[test]
    fn aphid_procreation_adds_child_when_probability_succeeds() {
        let mut board = board_with_field(1, 1);
        let mut random = Random::with_seed(3);
        let aphid = board.add_aphid(0, 0, 10).unwrap();
        board.add_aphid(0, 0, 10).unwrap();
        board.aphid_params.prob_procreate = 1.0;

        let births = board.creature_procreation(aphid, &mut random);

        assert_eq!(births, 1);
        assert_eq!(board.creatures.len(), 3);
        assert_eq!(
            board.field[board.index(Coordinates { x: 0, y: 0 })]
                .aphids
                .len(),
            3
        );
    }

    #[test]
    fn starvation_removes_creature_from_location() {
        let mut board = board_with_field(1, 1);
        let location = Coordinates { x: 0, y: 0 };
        let index = board.index(location);
        board.field[index].food = 0;
        let aphid = board.add_aphid(0, 0, 1).unwrap();

        assert_eq!(board.creature_starvation(aphid), Some(aphid));
        assert!(board.field[index].aphids.is_empty());
    }
}
