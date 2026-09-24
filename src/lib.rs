//! Simulation library for the aphid and ladybug life-cycle model.
//!
//! The library owns the board state, creature rules, configuration loading/parsing, and random
//! number generator wrapper. The binaries own command-line parsing, rendering, and sleeping.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha12Rng;
use serde::Deserialize;
use std::env;
use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

const CONFIG_DIR_NAME: &str = "creature_life_cycle";
const CONFIG_FILE_NAME: &str = "simulation.toml";
const MAX_CELL_FOOD: i32 = 9;
const DEFAULT_FOOD_REGEN_PROBABILITY: f64 = 0.1;

/// Zero-based board coordinate: `x` is the row and `y` is the column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    /// Optional food behavior parameters. Defaults are used when omitted.
    food: Option<FoodConfig>,
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

/// TOML food behavior configuration.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FoodConfig {
    /// Chance that a cell below the food cap regains one food after each turn.
    regeneration_probability: f64,
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
    /// Food available in the cell.
    food: i32,
}

/// Tunable aphid probabilities loaded from the runtime config.
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

/// Tunable ladybug probabilities loaded from the runtime config.
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

/// Tunable food probabilities loaded from the runtime config.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoodParams {
    /// Chance that a cell below the food cap regains one food after each turn.
    pub prob_regenerate: f64,
}

impl Default for FoodParams {
    fn default() -> Self {
        Self {
            prob_regenerate: DEFAULT_FOOD_REGEN_PROBABILITY,
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
    /// Index inside the creature's same-kind occupant list for its current cell.
    pub cell_slot: usize,
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
#[derive(Clone)]
pub struct Board {
    /// Number of board rows.
    rows: usize,
    /// Number of board columns.
    cols: usize,
    /// Row-major board cells.
    field: Vec<Location>,
    /// Stable creature ID table. Dead creatures are stored as `None`.
    creatures: Vec<Option<Creature>>,
    /// Per-ID index inside the creature's current cell occupant list.
    cell_slots: Vec<usize>,
    /// IDs for currently living creatures, in creation order.
    active_creatures: Vec<usize>,
    /// Reusable snapshot of active creature IDs for the current refresh.
    turn_creatures: Vec<usize>,
    /// Reusable list of creature IDs scheduled for phase-end removal.
    dead_creatures: Vec<usize>,
    /// Per-ID death marker epochs used while resolving one refresh.
    death_marks: Vec<u32>,
    /// Current marker epoch for scheduled deaths.
    death_mark_epoch: u32,
    /// Cached living creature counts and total board food.
    summary: BoardSummary,
    /// Active aphid behavior parameters.
    aphid_params: AphidParams,
    /// Active ladybug behavior parameters.
    ladybug_params: LadybugParams,
    /// Active food behavior parameters.
    food_params: FoodParams,
}

impl Board {
    /// Creates a board of the given size with random starting food and no creatures.
    ///
    /// This is the only way a `Board` comes into being, so every board has its cells from the
    /// start. The public constructors, `Board::standard` and `parse_simulation_config`, build on
    /// it and then add creatures.
    fn with_field(rows: usize, cols: usize, random: &mut Random) -> Self {
        let mut board = Self {
            rows,
            cols,
            field: Vec::with_capacity(rows * cols),
            creatures: Vec::new(),
            cell_slots: Vec::new(),
            active_creatures: Vec::new(),
            turn_creatures: Vec::new(),
            dead_creatures: Vec::new(),
            death_marks: Vec::new(),
            death_mark_epoch: 0,
            summary: BoardSummary::default(),
            aphid_params: AphidParams::default(),
            ladybug_params: LadybugParams::default(),
            food_params: FoodParams::default(),
        };

        for _ in 0..rows * cols {
            let food = random.int_inclusive(0, MAX_CELL_FOOD);
            board.field.push(Location {
                aphids: Vec::new(),
                ladybugs: Vec::new(),
                food,
            });
            board.summary.food += food;
        }

        board
    }

    /// Creates the built-in standard board, the same one `simulation.example.toml` describes.
    pub fn standard(random: &mut Random) -> Self {
        let mut board = Self::with_field(10, 10, random);

        board.add_aphid(3, 5, 10);
        board.add_aphid(4, 8, 10);
        board.add_aphid(2, 9, 10);
        board.add_aphid(1, 6, 10);
        board.add_aphid(1, 9, 10);

        board.add_ladybug(5, 9, 15, random);
        board.add_ladybug(1, 1, 15, random);
        board.add_ladybug(3, 8, 15, random);
        board.add_ladybug(9, 2, 15, random);

        board
    }

    /// Sets active aphid behavior parameters.
    pub fn set_aphid_params(&mut self, params: AphidParams) {
        self.aphid_params = params;
    }

    /// Sets active ladybug behavior parameters.
    pub fn set_ladybug_params(&mut self, params: LadybugParams) {
        self.ladybug_params = params;
    }

    /// Sets active food behavior parameters.
    pub fn set_food_params(&mut self, params: FoodParams) {
        self.food_params = params;
    }

    /// Returns active aphid behavior parameters.
    pub fn aphid_params(&self) -> AphidParams {
        self.aphid_params
    }

    /// Returns active ladybug behavior parameters.
    pub fn ladybug_params(&self) -> LadybugParams {
        self.ladybug_params
    }

    /// Returns active food behavior parameters.
    pub fn food_params(&self) -> FoodParams {
        self.food_params
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
        let mut snapshots = Vec::with_capacity(self.active_creatures.len());
        self.write_creature_snapshots(&mut snapshots);
        snapshots
    }

    /// Writes read-only data for every living creature into a reusable buffer.
    pub fn write_creature_snapshots(&self, snapshots: &mut Vec<CreatureSnapshot>) {
        snapshots.clear();
        snapshots.reserve(self.active_creatures.len());

        for id in self.active_creatures.iter().copied() {
            if let Some(creature) = self.creatures[id].as_ref() {
                snapshots.push(CreatureSnapshot {
                    id,
                    kind: match creature.kind() {
                        CreatureKind::Aphid => CreatureSnapshotKind::Aphid,
                        CreatureKind::Ladybug => CreatureSnapshotKind::Ladybug,
                    },
                    location: creature.location(),
                    cell_slot: self.cell_slots[id],
                });
            }
        }
    }

    /// Sets one cell's food while preserving the cached food total.
    #[cfg(test)]
    fn set_cell_food(&mut self, coordinates: Coordinates, food: i32) {
        let index = self.index(coordinates);
        self.summary.food += food - self.field[index].food;
        self.field[index].food = food;
    }

    /// Adds an aphid if the coordinate is valid and returns its stable ID, or `None` when the
    /// coordinate is outside the board.
    pub fn add_aphid(&mut self, x: usize, y: usize, life: i32) -> Option<usize> {
        let location = Coordinates { x, y };
        if !self.in_bounds(location) {
            return None;
        }

        let id = self.creatures.len();
        let index = self.index(location);
        let cell_slot = self.field[index].aphids.len();
        self.field[index].aphids.push(id);
        self.cell_slots.push(cell_slot);
        self.active_creatures.push(id);
        self.death_marks.push(0);
        self.summary.aphids += 1;
        self.creatures
            .push(Some(Creature::Aphid(Aphid { location, life })));
        Some(id)
    }

    /// Adds a ladybug if the coordinate is valid and returns its stable ID, or `None` when the
    /// coordinate is outside the board.
    pub fn add_ladybug(
        &mut self,
        x: usize,
        y: usize,
        life: i32,
        random: &mut Random,
    ) -> Option<usize> {
        let location = Coordinates { x, y };
        if !self.in_bounds(location) {
            return None;
        }

        let id = self.creatures.len();
        let index = self.index(location);
        let cell_slot = self.field[index].ladybugs.len();
        self.field[index].ladybugs.push(id);
        self.cell_slots.push(cell_slot);
        self.active_creatures.push(id);
        self.death_marks.push(0);
        self.summary.ladybugs += 1;
        self.creatures.push(Some(Creature::Ladybug(Ladybug::new(
            location, life, random,
        ))));
        Some(id)
    }

    /// Removes one aphid from a cell and returns whether a creature was removed.
    pub fn remove_aphid_at(&mut self, x: usize, y: usize) -> bool {
        let location = Coordinates { x, y };
        if !self.in_bounds(location) {
            return false;
        }

        let Some(id) = self.field[self.index(location)].aphids.last().copied() else {
            return false;
        };
        self.remove_creature(id);
        true
    }

    /// Removes one ladybug from a cell and returns whether a creature was removed.
    pub fn remove_ladybug_at(&mut self, x: usize, y: usize) -> bool {
        let location = Coordinates { x, y };
        if !self.in_bounds(location) {
            return false;
        }

        let Some(id) = self.field[self.index(location)].ladybugs.last().copied() else {
            return false;
        };
        self.remove_creature(id);
        true
    }

    /// Advances the board by one phased turn.
    ///
    /// The turn snapshot prevents newly born creatures from acting immediately. Combat deaths are
    /// applied before procreation and starvation so killed creatures do not continue acting later
    /// in the same turn.
    pub fn refresh(&mut self, random: &mut Random) -> TurnStats {
        let mut turn_creatures = std::mem::take(&mut self.turn_creatures);
        turn_creatures.clear();
        turn_creatures.extend_from_slice(&self.active_creatures);
        let mut stats = TurnStats::default();

        // Phase 1: all original creatures move before any combat or births happen.
        for id in turn_creatures.iter().copied() {
            self.creature_movement(id, random);
        }

        let mut dead_creatures = std::mem::take(&mut self.dead_creatures);
        dead_creatures.clear();
        self.start_death_marking();

        // Phase 2: combat is resolved for survivors from the original turn snapshot.
        for id in turn_creatures.iter().copied() {
            if self.is_marked_dead(id) || self.creatures.get(id).and_then(Option::as_ref).is_none()
            {
                continue;
            }

            if let Some(killed) = self.creature_combat(id, random) {
                self.mark_dead(&mut dead_creatures, killed);
            }
        }

        // Phase 3: combat deaths are applied before procreation and starvation.
        stats.deaths += dead_creatures.len();
        let mut removed_creatures = self.remove_dead_creatures(&dead_creatures);
        dead_creatures.clear();

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
                self.mark_dead(&mut dead_creatures, starved);
            }
        }

        // Phase 6: starvation deaths are applied after every survivor had a starvation turn.
        stats.deaths += dead_creatures.len();
        removed_creatures |= self.remove_dead_creatures(&dead_creatures);
        dead_creatures.clear();
        if removed_creatures {
            self.compact_active_creatures();
        }

        self.turn_creatures = turn_creatures;
        self.dead_creatures = dead_creatures;

        // Phase 7: cells randomly regain food after creatures finish acting.
        self.regenerate_food(random);

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
        let old_cell_slot = self.cell_slots[id];
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
        self.remove_from_location(kind, old_location, id, old_cell_slot);
        let new_cell_slot = self.add_to_location(kind, new_location, id);
        self.cell_slots[id] = new_cell_slot;

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
        let cell_slot = self.cell_slots[id];
        let index = self.index(location);

        let ate = if self.field[index].food > 0 {
            self.field[index].food -= 1;
            self.summary.food -= 1;
            true
        } else {
            false
        };

        let creature = self.creatures.get_mut(id).and_then(Option::as_mut)?;
        let starved = match creature {
            Creature::Aphid(aphid) => {
                if !ate {
                    aphid.life -= 1;
                }
                aphid.life < 1
            }
            Creature::Ladybug(ladybug) => {
                if !ate {
                    ladybug.life -= 1;
                }
                ladybug.life < 1
            }
        };

        if starved {
            self.remove_from_location(kind, location, id, cell_slot);
            Some(id)
        } else {
            None
        }
    }

    /// Randomly restores one food to cells that are below the food cap.
    fn regenerate_food(&mut self, random: &mut Random) -> usize {
        let mut regenerated = 0;

        for location in &mut self.field {
            if location.food < MAX_CELL_FOOD
                && random.probability() < self.food_params.prob_regenerate
            {
                location.food += 1;
                self.summary.food += 1;
                regenerated += 1;
            }
        }

        regenerated
    }

    /// Removes a creature from its current location and marks its stable ID as dead.
    fn remove_creature(&mut self, id: usize) {
        let Some((kind, location)) = self.creature_kind_location(id) else {
            return;
        };
        let cell_slot = self.cell_slots[id];

        self.remove_from_location(kind, location, id, cell_slot);
        self.mark_creature_removed(id, kind);
        self.active_creatures.retain(|active_id| *active_id != id);
    }

    /// Removes scheduled creatures without compacting active IDs.
    fn remove_dead_creatures(&mut self, dead_creatures: &[usize]) -> bool {
        let mut removed = false;

        for id in dead_creatures.iter().copied() {
            let Some(kind) = self.creature_kind(id) else {
                continue;
            };

            self.mark_creature_removed(id, kind);
            removed = true;
        }

        removed
    }

    /// Removes dead IDs from the active list after all refresh phases finish.
    fn compact_active_creatures(&mut self) {
        let creatures = &self.creatures;
        self.active_creatures
            .retain(|active_id| creatures[*active_id].is_some());
    }

    /// Returns a live creature's kind and location by stable ID.
    fn creature_kind_location(&self, id: usize) -> Option<(CreatureKind, Coordinates)> {
        self.creatures
            .get(id)
            .and_then(Option::as_ref)
            .map(|creature| (creature.kind(), creature.location()))
    }

    /// Returns a live creature's kind by stable ID.
    fn creature_kind(&self, id: usize) -> Option<CreatureKind> {
        self.creatures
            .get(id)
            .and_then(Option::as_ref)
            .map(Creature::kind)
    }

    /// Marks a creature as removed in the stable ID table and cached totals.
    fn mark_creature_removed(&mut self, id: usize, kind: CreatureKind) {
        self.creatures[id] = None;
        match kind {
            CreatureKind::Aphid => self.summary.aphids -= 1,
            CreatureKind::Ladybug => self.summary.ladybugs -= 1,
        }
    }

    /// Starts a new scheduled-death marking pass without clearing the full marker table.
    fn start_death_marking(&mut self) {
        self.death_mark_epoch = self.death_mark_epoch.wrapping_add(1);
        if self.death_mark_epoch == 0 {
            self.death_marks.fill(0);
            self.death_mark_epoch = 1;
        }
    }

    /// Returns whether a creature is already scheduled to die in this refresh.
    fn is_marked_dead(&self, id: usize) -> bool {
        self.death_marks.get(id).copied() == Some(self.death_mark_epoch)
    }

    /// Marks one creature ID as dead and records it once for phase-end removal.
    fn mark_dead(&mut self, dead_creatures: &mut Vec<usize>, id: usize) {
        if id >= self.death_marks.len() {
            self.death_marks.resize(id + 1, 0);
        }

        if self.death_marks[id] != self.death_mark_epoch {
            self.death_marks[id] = self.death_mark_epoch;
            dead_creatures.push(id);
        }
    }

    /// Adds an existing creature ID to a location list for its kind and returns its cell slot.
    fn add_to_location(&mut self, kind: CreatureKind, location: Coordinates, id: usize) -> usize {
        let index = self.index(location);
        match kind {
            CreatureKind::Aphid => {
                let cell_slot = self.field[index].aphids.len();
                self.field[index].aphids.push(id);
                cell_slot
            }
            CreatureKind::Ladybug => {
                let cell_slot = self.field[index].ladybugs.len();
                self.field[index].ladybugs.push(id);
                cell_slot
            }
        }
    }

    /// Removes an existing creature ID from a location list for its kind.
    fn remove_from_location(
        &mut self,
        kind: CreatureKind,
        location: Coordinates,
        id: usize,
        cell_slot: usize,
    ) {
        let index = self.index(location);
        let moved = match kind {
            CreatureKind::Aphid => {
                swap_remove_occupant(&mut self.field[index].aphids, id, cell_slot)
            }
            CreatureKind::Ladybug => {
                swap_remove_occupant(&mut self.field[index].ladybugs, id, cell_slot)
            }
        };

        if let Some((moved_id, moved_slot)) = moved {
            self.cell_slots[moved_id] = moved_slot;
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
        self.summary
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
/// `ChaCha12Rng` gives deterministic seeded runs while `from_os_rng` provides non-deterministic
/// default runs from the operating system. It is named directly rather than through
/// `rand::rngs::StdRng`, whose algorithm `rand` may change between releases, so a given seed keeps
/// producing the same run across dependency upgrades.
#[derive(Clone)]
pub struct Random {
    /// Internal generator from the `rand` crate.
    rng: ChaCha12Rng,
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
            rng: ChaCha12Rng::from_os_rng(),
        }
    }

    /// Creates a generator from an explicit seed.
    pub fn with_seed(seed: u64) -> Self {
        Self {
            rng: ChaCha12Rng::seed_from_u64(seed),
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

/// Why config file contents do not describe a board.
///
/// Messages are complete on their own, so front ends print them as they are.
#[derive(Debug)]
pub enum InvalidConfig {
    /// The contents are not TOML, or do not match the config schema.
    Toml(toml::de::Error),
    /// `rows` or `columns` is zero.
    EmptyBoard,
    /// A probability lies outside `0.0..=1.0`.
    Probability {
        /// Which probability, named as a reader would name it.
        label: &'static str,
        /// The value found in the file.
        value: f64,
    },
}

impl fmt::Display for InvalidConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(error) => write!(f, "{error}"),
            Self::EmptyBoard => f.write_str("board dimensions must be greater than zero"),
            Self::Probability { label, value } => {
                write!(f, "{label} must be between 0 and 1, not {value}")
            }
        }
    }
}

// The message already includes the TOML error, so it is not repeated as a
// `source`, which would print it twice for callers that walk the chain.
impl Error for InvalidConfig {}

/// Why a config file could not be found, read, parsed, or written.
///
/// Each message names the file involved and already includes the underlying error, so front ends
/// print it as it is. The underlying error stays in the variant for callers that need to tell the
/// cases apart.
#[derive(Debug)]
pub enum ConfigError {
    /// Neither `XDG_CONFIG_HOME` nor `HOME` is set, so there is no default config location.
    NoConfigHome,
    /// A config file named by the caller does not exist.
    NotFound { path: PathBuf },
    /// A config file exists but could not be read.
    Read { path: PathBuf, error: io::Error },
    /// A config file was read but does not describe a board.
    Invalid { path: PathBuf, error: InvalidConfig },
    /// The directory for a config file could not be created.
    CreateDir { path: PathBuf, error: io::Error },
    /// A config file could not be written.
    Write { path: PathBuf, error: io::Error },
    /// An invalid config file could not be moved aside before being saved over.
    Backup {
        path: PathBuf,
        backup: PathBuf,
        error: io::Error,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigHome => f.write_str("XDG_CONFIG_HOME and HOME are not set"),
            Self::NotFound { path } => write!(f, "{} does not exist", path.display()),
            Self::Read { path, error } => write!(f, "could not read {}: {error}", path.display()),
            Self::Invalid { path, error } => write!(f, "invalid {}: {error}", path.display()),
            Self::CreateDir { path, error } => write!(
                f,
                "failed to create config directory {}: {error}",
                path.display()
            ),
            Self::Write { path, error } => write!(f, "failed to write {}: {error}", path.display()),
            Self::Backup {
                path,
                backup,
                error,
            } => write!(
                f,
                "failed to back up invalid {} to {}: {error}",
                path.display(),
                backup.display()
            ),
        }
    }
}

// See `InvalidConfig`: the message already carries the inner error.
impl Error for ConfigError {}

/// Something a successful load did, or left out, that the user should hear about.
///
/// The library never prints. It hands these to the front end, which decides where they go: the
/// CLI and the GUI both write them to stderr.
#[derive(Debug)]
pub enum ConfigNotice {
    /// There is no config location, so the standard board runs without being saved.
    NoConfigHome,
    /// The config file was missing and has been created from the standard board.
    Created { path: PathBuf },
    /// The config file was missing and could not be created, so the standard board runs without
    /// being saved.
    NotCreated { error: ConfigError },
    /// A starting creature lies outside the board and was left out.
    OutsideBoard {
        kind: CreatureSnapshotKind,
        x: usize,
        y: usize,
    },
}

impl fmt::Display for ConfigNotice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigHome => write!(
                f,
                "Config path unavailable ({}), using standard data.",
                ConfigError::NoConfigHome
            ),
            Self::Created { path } => write!(f, "Created default config at {}.", path.display()),
            Self::NotCreated { error } => write!(
                f,
                "Config file not found, using standard data. Could not create default config: {error}"
            ),
            Self::OutsideBoard { kind, x, y } => {
                let kind = match kind {
                    CreatureSnapshotKind::Aphid => "Aphid",
                    CreatureSnapshotKind::Ladybug => "Ladybug",
                };
                write!(f, "{kind} position {x} {y} is outside the board; skipping.")
            }
        }
    }
}

/// A board loaded from a config file, with anything the user should be told about the load.
pub struct LoadedBoard {
    /// The board, ready to run.
    pub board: Board,
    /// What the load did or left out, in the order it happened.
    pub notices: Vec<ConfigNotice>,
}

/// Parses TOML runtime configuration and builds the board it describes.
///
/// Starting creatures outside the board are left out rather than failing the load, and are
/// returned as `ConfigNotice::OutsideBoard` so the caller can report them. Everything is validated
/// before the board is built, so an invalid config draws nothing from `random`.
pub fn parse_simulation_config(
    contents: &str,
    random: &mut Random,
) -> Result<LoadedBoard, InvalidConfig> {
    let config: SimulationConfig = toml::from_str(contents).map_err(InvalidConfig::Toml)?;
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
    let food_params = config
        .food
        .map(FoodConfig::params)
        .transpose()?
        .unwrap_or_default();

    if config.board.rows == 0 || config.board.columns == 0 {
        return Err(InvalidConfig::EmptyBoard);
    }

    let mut board = Board::with_field(config.board.rows, config.board.columns, random);

    let mut notices = Vec::new();
    for ConfigCoordinates { x, y } in config.board.aphids {
        if board.add_aphid(x, y, 10).is_none() {
            notices.push(ConfigNotice::OutsideBoard {
                kind: CreatureSnapshotKind::Aphid,
                x,
                y,
            });
        }
    }

    for ConfigCoordinates { x, y } in config.board.ladybugs {
        if board.add_ladybug(x, y, 15, random).is_none() {
            notices.push(ConfigNotice::OutsideBoard {
                kind: CreatureSnapshotKind::Ladybug,
                x,
                y,
            });
        }
    }

    board.set_aphid_params(aphid_params);
    board.set_ladybug_params(ladybug_params);
    board.set_food_params(food_params);

    Ok(LoadedBoard { board, notices })
}

/// Formats the current board and active behavior parameters as `simulation.toml` content.
pub fn format_simulation_config(board: &Board) -> String {
    let mut aphids = Vec::new();
    let mut ladybugs = Vec::new();

    for creature in board.creature_snapshots() {
        match creature.kind {
            CreatureSnapshotKind::Aphid => aphids.push(creature.location),
            CreatureSnapshotKind::Ladybug => ladybugs.push(creature.location),
        }
    }

    let aphid_params = board.aphid_params();
    let ladybug_params = board.ladybug_params();
    let food_params = board.food_params();
    let mut contents = String::new();

    writeln!(&mut contents, "[board]").expect("write to string");
    writeln!(&mut contents, "rows = {}", board.rows()).expect("write to string");
    writeln!(&mut contents, "columns = {}", board.cols()).expect("write to string");
    writeln!(&mut contents).expect("write to string");
    write_coordinate_array(&mut contents, "aphids", &aphids);
    writeln!(&mut contents).expect("write to string");
    write_coordinate_array(&mut contents, "ladybugs", &ladybugs);
    writeln!(&mut contents).expect("write to string");
    writeln!(&mut contents, "[aphid]").expect("write to string");
    writeln!(
        &mut contents,
        "move_probability = {}",
        aphid_params.prob_move
    )
    .expect("write to string");
    writeln!(
        &mut contents,
        "kill_probability = {}",
        aphid_params.prob_kill
    )
    .expect("write to string");
    writeln!(
        &mut contents,
        "accomplice_probability = {}",
        aphid_params.prob_accomplice
    )
    .expect("write to string");
    writeln!(
        &mut contents,
        "procreation_probability = {}",
        aphid_params.prob_procreate
    )
    .expect("write to string");
    writeln!(&mut contents).expect("write to string");
    writeln!(&mut contents, "[ladybug]").expect("write to string");
    writeln!(
        &mut contents,
        "move_probability = {}",
        ladybug_params.prob_move
    )
    .expect("write to string");
    writeln!(
        &mut contents,
        "kill_probability = {}",
        ladybug_params.prob_kill
    )
    .expect("write to string");
    writeln!(
        &mut contents,
        "direction_change_probability = {}",
        ladybug_params.prob_direction
    )
    .expect("write to string");
    writeln!(
        &mut contents,
        "procreation_probability = {}",
        ladybug_params.prob_procreate
    )
    .expect("write to string");
    writeln!(&mut contents).expect("write to string");
    writeln!(&mut contents, "[food]").expect("write to string");
    writeln!(
        &mut contents,
        "regeneration_probability = {}",
        food_params.prob_regenerate
    )
    .expect("write to string");

    contents
}

/// Returns the XDG runtime config path.
///
/// `XDG_CONFIG_HOME` is used when set; otherwise this falls back to
/// `$HOME/.config/creature_life_cycle/simulation.toml`.
pub fn simulation_config_path() -> Result<PathBuf, ConfigError> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .ok_or(ConfigError::NoConfigHome)?;

    Ok(config_home.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
}

/// Where `save_configured_board` wrote the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedConfig {
    /// The config file that now holds the board.
    pub path: PathBuf,
    /// Where the previous file was moved because it did not parse, if it was.
    pub backup: Option<PathBuf>,
}

/// Saves the current board and active behavior parameters to the XDG config path.
///
/// An existing file that does not parse is moved aside to `simulation.toml.bak` first. Such a file
/// is one the board was never loaded from, most likely a hand edit with a mistake in it, and
/// overwriting it would throw that work away.
pub fn save_configured_board(board: &Board) -> Result<SavedConfig, ConfigError> {
    let path = simulation_config_path()?;
    let backup = back_up_invalid_config(&path)?;
    save_simulation_config(board, &path)?;
    Ok(SavedConfig { path, backup })
}

/// Moves a config file that does not parse to `<path>.bak`, returning the backup path.
///
/// A valid file, or no file at all, is left where it is.
fn back_up_invalid_config(path: &Path) -> Result<Option<PathBuf>, ConfigError> {
    let Some(contents) = read_config_file(path)? else {
        return Ok(None);
    };
    // The parse only checks validity, so it draws from a throwaway generator
    // and leaves the caller's random sequence untouched.
    if parse_simulation_config(&contents, &mut Random::with_seed(0)).is_ok() {
        return Ok(None);
    }

    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    let backup = PathBuf::from(backup);
    match fs::rename(path, &backup) {
        Ok(()) => Ok(Some(backup)),
        Err(error) => Err(ConfigError::Backup {
            path: path.to_path_buf(),
            backup,
            error,
        }),
    }
}

/// Saves the current board and active behavior parameters to a TOML config file.
pub fn save_simulation_config(board: &Board, path: impl AsRef<Path>) -> Result<(), ConfigError> {
    let path = path.as_ref();

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| ConfigError::CreateDir {
            path: parent.to_path_buf(),
            error,
        })?;
    }

    fs::write(path, format_simulation_config(board)).map_err(|error| ConfigError::Write {
        path: path.to_path_buf(),
        error,
    })
}

/// Writes a TOML inline-table coordinate array.
fn write_coordinate_array(contents: &mut String, label: &str, coordinates: &[Coordinates]) {
    if coordinates.is_empty() {
        writeln!(contents, "{label} = []").expect("write to string");
        return;
    }

    writeln!(contents, "{label} = [").expect("write to string");
    for coordinate in coordinates {
        writeln!(
            contents,
            "  {{ x = {}, y = {} }},",
            coordinate.x, coordinate.y
        )
        .expect("write to string");
    }
    writeln!(contents, "]").expect("write to string");
}

impl AphidConfig {
    /// Converts TOML values into validated aphid parameters.
    fn params(self) -> Result<AphidParams, InvalidConfig> {
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
    fn params(self) -> Result<LadybugParams, InvalidConfig> {
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

impl FoodConfig {
    /// Converts TOML values into validated food parameters.
    fn params(self) -> Result<FoodParams, InvalidConfig> {
        Ok(FoodParams {
            prob_regenerate: validate_probability(
                self.regeneration_probability,
                "food regeneration probability",
            )?,
        })
    }
}

/// Loads the runtime TOML configuration file from the XDG config directory.
///
/// A missing file is created from the built-in standard data, which is then used, and the standard
/// data is also used when there is no config directory at all. A file that exists but cannot be
/// read or parsed is an error: running the defaults instead would quietly simulate a board nobody
/// asked for.
pub fn load_configured_board(random: &mut Random) -> Result<LoadedBoard, ConfigError> {
    let Ok(path) = simulation_config_path() else {
        return Ok(LoadedBoard {
            board: Board::standard(random),
            notices: vec![ConfigNotice::NoConfigHome],
        });
    };

    let Some(contents) = read_config_file(&path)? else {
        let board = Board::standard(random);
        let notice = match save_simulation_config(&board, &path) {
            Ok(()) => ConfigNotice::Created { path },
            Err(error) => ConfigNotice::NotCreated { error },
        };
        return Ok(LoadedBoard {
            board,
            notices: vec![notice],
        });
    };

    board_from_config(&contents, &path, random)
}

/// Loads a board from a named TOML configuration file.
///
/// Unlike `load_configured_board`, a missing file is an error rather than a cue to create one: the
/// caller asked for this file, so a typo in its path should not run something else.
pub fn load_board_from(
    path: impl AsRef<Path>,
    random: &mut Random,
) -> Result<LoadedBoard, ConfigError> {
    let path = path.as_ref();
    let contents = read_config_file(path)?.ok_or_else(|| ConfigError::NotFound {
        path: path.to_path_buf(),
    })?;
    board_from_config(&contents, path, random)
}

/// Reads a config file, returning `None` when it does not exist.
fn read_config_file(path: &Path) -> Result<Option<String>, ConfigError> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ConfigError::Read {
            path: path.to_path_buf(),
            error,
        }),
    }
}

/// Builds a board from config file contents, naming the file in any error.
fn board_from_config(
    contents: &str,
    path: &Path,
    random: &mut Random,
) -> Result<LoadedBoard, ConfigError> {
    parse_simulation_config(contents, random).map_err(|error| ConfigError::Invalid {
        path: path.to_path_buf(),
        error,
    })
}

/// Validates a probability value is in `0.0..=1.0`.
fn validate_probability(value: f64, label: &'static str) -> Result<f64, InvalidConfig> {
    if (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(InvalidConfig::Probability { label, value })
    }
}

/// Removes an occupant by its cached slot and returns the ID moved into that slot, if any.
#[inline]
fn swap_remove_occupant(
    occupants: &mut Vec<usize>,
    id: usize,
    cell_slot: usize,
) -> Option<(usize, usize)> {
    debug_assert_eq!(occupants.get(cell_slot).copied(), Some(id));

    let removed = occupants.swap_remove(cell_slot);
    debug_assert_eq!(removed, id);

    (cell_slot < occupants.len()).then(|| (occupants[cell_slot], cell_slot))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board_with_field(rows: usize, cols: usize) -> Board {
        Board::with_field(rows, cols, &mut Random::with_seed(1))
    }

    fn creature_cell_slot(board: &Board, id: usize) -> usize {
        board.cell_slots[id]
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
    fn parse_simulation_config_loads_board_and_params() {
        let mut random = Random::with_seed(1);

        let board = parse_simulation_config(
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

[food]
regeneration_probability = 0.9
"#,
            &mut random,
        )
        .unwrap()
        .board;

        assert_eq!(board.rows, 2);
        assert_eq!(board.cols, 3);
        assert_eq!(board.creatures.len(), 2);
        assert_eq!(board.aphid_params.prob_move, 0.6);
        assert_eq!(board.aphid_params.prob_accomplice, 0.2);
        assert_eq!(board.ladybug_params.prob_kill, 0.8);
        assert_eq!(board.ladybug_params.prob_direction, 0.7);
        assert_eq!(board.food_params.prob_regenerate, 0.9);
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
    fn standard_board_matches_the_example_config() {
        let standard = Board::standard(&mut Random::with_seed(1));
        let example = parse_simulation_config(
            include_str!("../simulation.example.toml"),
            &mut Random::with_seed(1),
        )
        .unwrap();

        assert!(example.notices.is_empty());
        assert_eq!(
            format_simulation_config(&standard),
            format_simulation_config(&example.board)
        );
        // Same seed, same draws: the food each cell starts with matches too.
        assert_eq!(standard.summary(), example.board.summary());
    }

    #[test]
    fn an_invalid_config_draws_nothing_from_the_generator() {
        let mut random = Random::with_seed(1);
        let invalid = "[board]\nrows = 0\ncolumns = 2\naphids = []\nladybugs = []\n";
        assert!(matches!(
            parse_simulation_config(invalid, &mut random),
            Err(InvalidConfig::EmptyBoard)
        ));

        // A failed parse followed by the fallback must match the fallback alone,
        // so the GUI's seeded run does not depend on whether the config was valid.
        let after_failure = Board::standard(&mut random);
        let mut reference = Random::with_seed(1);
        let fresh = Board::standard(&mut reference);
        assert_eq!(after_failure.summary(), fresh.summary());
        assert_eq!(random.probability(), reference.probability());
    }

    #[test]
    fn format_simulation_config_round_trips() {
        let mut random = Random::with_seed(1);
        let board = parse_simulation_config(
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

[food]
regeneration_probability = 0.9
"#,
            &mut random,
        )
        .unwrap()
        .board;

        let contents = format_simulation_config(&board);
        let mut round_trip_random = Random::with_seed(1);
        let round_trip = parse_simulation_config(&contents, &mut round_trip_random)
            .unwrap()
            .board;

        assert_eq!(round_trip.rows, 2);
        assert_eq!(round_trip.cols, 3);
        assert_eq!(round_trip.aphid_params, board.aphid_params);
        assert_eq!(round_trip.ladybug_params, board.ladybug_params);
        assert_eq!(round_trip.food_params, board.food_params);
        assert_eq!(round_trip.cell_counts(0, 1), Some((1, 0)));
        assert_eq!(round_trip.cell_counts(1, 2), Some((0, 1)));
    }

    #[test]
    fn save_simulation_config_creates_parent_directory() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-output")
            .join("save_simulation_config")
            .join("simulation.toml");
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }

        let board = board_with_field(1, 1);

        save_simulation_config(&board, &path).unwrap();

        assert!(path.exists());
    }

    fn clean_test_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-output")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_board_from_reads_a_named_file_and_rejects_missing_or_invalid_ones() {
        let dir = clean_test_dir("load_board_from");
        let mut random = Random::with_seed(1);

        let valid = dir.join("valid.toml");
        fs::write(
            &valid,
            "[board]\nrows = 2\ncolumns = 3\naphids = [{ x = 1, y = 2 }]\nladybugs = []\n",
        )
        .unwrap();
        let loaded = load_board_from(&valid, &mut random).unwrap();
        assert_eq!((loaded.board.rows(), loaded.board.cols()), (2, 3));
        assert_eq!(loaded.board.cell_counts(1, 2), Some((1, 0)));
        assert!(loaded.notices.is_empty());

        let missing = dir.join("missing.toml");
        let error = load_board_from(&missing, &mut random).err().unwrap();
        assert!(matches!(&error, ConfigError::NotFound { path } if *path == missing));
        assert!(error.to_string().ends_with("missing.toml does not exist"));
        assert!(!missing.exists(), "a named file is never created");

        let invalid = dir.join("invalid.toml");
        fs::write(&invalid, "[board]\nrows = 2\n").unwrap();
        let error = load_board_from(&invalid, &mut random).err().unwrap();
        assert!(matches!(
            &error,
            ConfigError::Invalid { path, error: InvalidConfig::Toml(_) } if *path == invalid
        ));
        assert!(error.to_string().starts_with("invalid "), "{error}");
    }

    #[test]
    fn creatures_outside_the_board_are_left_out_and_reported() {
        let mut random = Random::with_seed(1);
        let LoadedBoard { board, notices } = parse_simulation_config(
            "[board]\nrows = 2\ncolumns = 2\naphids = [{ x = 1, y = 1 }, { x = 2, y = 0 }]\nladybugs = [{ x = 0, y = 5 }]\n",
            &mut random,
        )
        .unwrap();

        assert_eq!(board.summary().aphids, 1);
        assert_eq!(board.summary().ladybugs, 0);
        let messages: Vec<String> = notices.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "Aphid position 2 0 is outside the board; skipping.",
                "Ladybug position 0 5 is outside the board; skipping.",
            ]
        );
    }

    #[test]
    fn an_invalid_config_is_backed_up_and_a_valid_one_is_left_in_place() {
        let dir = clean_test_dir("back_up_invalid_config");
        let path = dir.join("simulation.toml");
        let backup = dir.join("simulation.toml.bak");

        assert_eq!(back_up_invalid_config(&path).unwrap(), None);

        save_simulation_config(&board_with_field(1, 1), &path).unwrap();
        assert_eq!(back_up_invalid_config(&path).unwrap(), None);
        assert!(path.exists());

        let broken = "[board]\nrows = 3\ncolums = 4\n";
        fs::write(&path, broken).unwrap();
        assert_eq!(back_up_invalid_config(&path).unwrap(), Some(backup.clone()));
        assert!(!path.exists());
        assert_eq!(fs::read_to_string(&backup).unwrap(), broken);
    }

    #[test]
    fn remove_creature_at_removes_one_matching_kind() {
        let mut random = Random::with_seed(1);
        let mut board = board_with_field(1, 1);
        board.add_aphid(0, 0, 10).unwrap();
        board.add_aphid(0, 0, 10).unwrap();
        board.add_ladybug(0, 0, 15, &mut random).unwrap();

        assert!(board.remove_aphid_at(0, 0));
        assert_eq!(board.cell_counts(0, 0), Some((1, 1)));
        assert!(board.remove_ladybug_at(0, 0));
        assert_eq!(board.cell_counts(0, 0), Some((1, 0)));
        assert!(!board.remove_ladybug_at(0, 0));
        assert_eq!(board.cell_counts(0, 0), Some((1, 0)));
    }

    #[test]
    fn removed_creatures_are_not_active() {
        let mut random = Random::with_seed(1);
        let mut board = board_with_field(1, 1);
        let aphid = board.add_aphid(0, 0, 10).unwrap();
        let ladybug = board.add_ladybug(0, 0, 15, &mut random).unwrap();

        assert!(board.remove_aphid_at(0, 0));

        let snapshots = board.creature_snapshots();
        assert_eq!(board.active_creatures, vec![ladybug]);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, ladybug);
        assert_eq!(board.summary().aphids, 0);
        assert_eq!(board.summary().ladybugs, 1);
        assert!(board.creatures[aphid].is_none());
    }

    #[test]
    fn cell_slots_follow_swap_removed_occupants() {
        let mut random = Random::with_seed(1);
        let mut board = board_with_field(1, 1);
        let index = board.index(Coordinates { x: 0, y: 0 });

        let first_aphid = board.add_aphid(0, 0, 10).unwrap();
        let middle_aphid = board.add_aphid(0, 0, 10).unwrap();
        let last_aphid = board.add_aphid(0, 0, 10).unwrap();
        let first_ladybug = board.add_ladybug(0, 0, 15, &mut random).unwrap();
        let middle_ladybug = board.add_ladybug(0, 0, 15, &mut random).unwrap();
        let last_ladybug = board.add_ladybug(0, 0, 15, &mut random).unwrap();

        board.remove_creature(first_aphid);
        board.remove_creature(first_ladybug);

        assert_eq!(board.field[index].aphids, vec![last_aphid, middle_aphid]);
        assert_eq!(creature_cell_slot(&board, last_aphid), 0);
        assert_eq!(creature_cell_slot(&board, middle_aphid), 1);
        assert_eq!(
            board.field[index].ladybugs,
            vec![last_ladybug, middle_ladybug]
        );
        assert_eq!(creature_cell_slot(&board, last_ladybug), 0);
        assert_eq!(creature_cell_slot(&board, middle_ladybug), 1);

        board.remove_creature(last_aphid);
        board.remove_creature(last_ladybug);

        assert_eq!(board.field[index].aphids, vec![middle_aphid]);
        assert_eq!(creature_cell_slot(&board, middle_aphid), 0);
        assert_eq!(board.field[index].ladybugs, vec![middle_ladybug]);
        assert_eq!(creature_cell_slot(&board, middle_ladybug), 0);
    }

    #[test]
    fn parse_simulation_config_rejects_out_of_range_probabilities() {
        let mut random = Random::with_seed(1);
        assert!(matches!(
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
                &mut random,
            ),
            Err(InvalidConfig::Probability {
                label: "aphid move probability",
                value: 1.2,
            })
        ));

        assert!(
            parse_simulation_config(
                r#"
[board]
rows = 2
columns = 2
aphids = []
ladybugs = []

[food]
regeneration_probability = -0.1
"#,
                &mut random,
            )
            .is_err()
        );
    }

    #[test]
    fn parse_simulation_config_rejects_unknown_fields() {
        let mut random = Random::with_seed(1);

        assert!(
            parse_simulation_config(
                r#"
[board]
rows = 2
columns = 2
aphids = []
ladybugs = []
extra = true
"#,
                &mut random,
            )
            .is_err()
        );
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
        board.set_cell_food(location, 100);

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
        board.set_cell_food(location, 0);
        let aphid = board.add_aphid(0, 0, 1).unwrap();

        assert_eq!(board.creature_starvation(aphid), Some(aphid));
        assert!(board.field[index].aphids.is_empty());
    }

    #[test]
    fn starvation_never_reduces_food_below_zero() {
        let mut board = board_with_field(1, 1);
        let location = Coordinates { x: 0, y: 0 };
        let index = board.index(location);
        board.set_cell_food(location, 0);
        let aphid = board.add_aphid(0, 0, 2).unwrap();

        assert_eq!(board.creature_starvation(aphid), None);
        assert_eq!(board.field[index].food, 0);
    }

    #[test]
    fn starvation_consumes_last_food_without_life_loss() {
        let mut board = board_with_field(1, 1);
        let location = Coordinates { x: 0, y: 0 };
        let index = board.index(location);
        board.set_cell_food(location, 1);
        let aphid = board.add_aphid(0, 0, 1).unwrap();

        assert_eq!(board.creature_starvation(aphid), None);
        assert_eq!(board.field[index].food, 0);
    }

    #[test]
    fn food_regeneration_restores_food_without_exceeding_cap() {
        let mut board = board_with_field(10, 10);
        for x in 0..board.rows {
            for y in 0..board.cols {
                board.set_cell_food(Coordinates { x, y }, 0);
            }
        }
        board.food_params.prob_regenerate = 1.0;
        let mut random = Random::with_seed(1);

        let regenerated = board.regenerate_food(&mut random);

        assert_eq!(regenerated, 100);
        assert_eq!(board.summary().food, 100);
        assert!(
            board
                .field
                .iter()
                .all(|location| location.food == 1 && location.food <= MAX_CELL_FOOD)
        );
    }
}
