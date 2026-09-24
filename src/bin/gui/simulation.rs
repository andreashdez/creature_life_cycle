//! The simulation state the GUI holds, and the one path that advances or
//! edits it. Every turn still comes from `Board::refresh`.

use crate::editing::{
    CellEdit, EDIT_APHID_LIFE, EDIT_LADYBUG_LIFE, EDIT_UNDO_LIMIT, EditAction, EditHistory,
    EditSnapshot, EditTool,
};
use crate::history::History;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use creature_life_cycle::{Board, Random, TurnStats};

pub const DEFAULT_SEED: u64 = 42;
pub const STEP_SECONDS: f32 = 0.45;

#[derive(Resource)]
pub struct BoardRes(pub Board);

#[derive(Resource)]
pub struct Rng(pub Random);

#[derive(Resource)]
pub struct StepTimer(pub Timer);

#[derive(Resource)]
pub struct Playing(pub bool);

/// Board changes that the renderers must react to. The macroquad GUI uses a
/// `board_revision` counter for the same reason; relying on `ResMut` change
/// detection instead would repaint on every parameter tweak, because editing a
/// parameter also touches the board.
#[derive(Resource)]
pub struct Revision(pub u64);

#[derive(Resource, Default, Clone)]
pub struct Stats {
    pub turn: usize,
    pub aphids: usize,
    pub ladybugs: usize,
    pub food: i32,
    pub births: usize,
    pub deaths: usize,
    pub extinct: bool,
    pub deltas: [i64; 3],
}

/// The board Reset returns to. Captured at launch and replaced by a save, so
/// Reset follows the config file the way the previous macroquad GUI's reset does by
/// reloading it from disk.
#[derive(Resource)]
pub struct StartingSetup(pub String);

/// Seed the current run was started from. The board loaded at launch uses
/// `DEFAULT_SEED`; the panel's seed field replaces it and restarts the run, as
/// `set_seed` does in the previous macroquad GUI.
#[derive(Resource, Clone, Copy)]
pub struct Seed(pub u64);

pub fn step_simulation(time: Res<Time>, mut timer: ResMut<StepTimer>, mut sim: Simulation) {
    if !sim.playing.0 || sim.stats.extinct || !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    sim.step();
}

/// Everything one turn touches. Shared by the timer and the panel's "step"
/// button so the two can never drift apart.
#[derive(SystemParam)]
pub struct Simulation<'w> {
    pub board: ResMut<'w, BoardRes>,
    pub rng: ResMut<'w, Rng>,
    pub stats: ResMut<'w, Stats>,
    pub revision: ResMut<'w, Revision>,
    pub history: ResMut<'w, History>,
    pub playing: ResMut<'w, Playing>,
    pub edits: ResMut<'w, EditHistory>,
}

impl Simulation<'_> {
    /// Applies one cell edit and returns whether the board changed. Adding
    /// always succeeds in bounds; removing does nothing on a cell without that
    /// kind of creature, and then the run carries on untouched.
    pub fn edit(&mut self, edit: CellEdit) -> bool {
        let Some(cell) = self.board.0.cell_snapshot(edit.row, edit.col) else {
            return false;
        };
        let can_change = match (edit.tool, edit.action) {
            (EditTool::Erase, _) => cell.aphids + cell.ladybugs > 0,
            (EditTool::Aphid, EditAction::Remove) => cell.aphids > 0,
            (EditTool::Ladybug, EditAction::Remove) => cell.ladybugs > 0,
            (_, EditAction::Add) => true,
        };
        if !can_change {
            return false;
        }
        let snapshot = EditSnapshot {
            board: self.board.0.clone(),
            random: self.rng.0.clone(),
            stats: self.stats.clone(),
            history: self.history.clone(),
        };
        let board = &mut self.board.0;
        let changed = match (edit.tool, edit.action) {
            (EditTool::Aphid, EditAction::Add) => board
                .add_aphid(edit.row, edit.col, EDIT_APHID_LIFE)
                .is_some(),
            (EditTool::Ladybug, EditAction::Add) => board
                .add_ladybug(edit.row, edit.col, EDIT_LADYBUG_LIFE, &mut self.rng.0)
                .is_some(),
            (EditTool::Aphid, EditAction::Remove) => board.remove_aphid_at(edit.row, edit.col),
            (EditTool::Ladybug, EditAction::Remove) => board.remove_ladybug_at(edit.row, edit.col),
            (EditTool::Erase, _) => {
                while board.remove_aphid_at(edit.row, edit.col) {}
                while board.remove_ladybug_at(edit.row, edit.col) {}
                true
            }
        };
        if changed {
            if self.edits.0.len() == EDIT_UNDO_LIMIT {
                self.edits.0.pop_front();
            }
            self.edits.0.push_back(snapshot);
            self.restart_from_edit();
        }
        changed
    }

    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.edits.0.pop_back() else {
            return false;
        };
        self.board.0 = snapshot.board;
        self.rng.0 = snapshot.random;
        *self.stats = snapshot.stats;
        *self.history = snapshot.history;
        self.playing.0 = false;
        self.revision.0 += 1;
        true
    }

    /// An edited board is a new starting setup, as `after_board_edit` treats it
    /// in the previous macroquad GUI: the run pauses, and the turn count and population
    /// history start over from what is on the board now.
    pub fn restart_from_edit(&mut self) {
        let summary = self.board.0.summary();
        *self.stats = Stats {
            turn: 0,
            aphids: summary.aphids,
            ladybugs: summary.ladybugs,
            food: summary.food,
            births: 0,
            deaths: 0,
            extinct: summary.is_extinct(),
            deltas: [0; 3],
        };
        self.playing.0 = false;
        self.history.0.clear();
        self.history.1.clear();
        self.history.record(0, summary.aphids, summary.ladybugs);
        self.revision.0 += 1;
    }

    /// Advances one turn, records it, and marks the board dirty for the
    /// renderers.
    pub fn step(&mut self) {
        self.edits.0.clear();
        let TurnStats {
            births,
            deaths,
            summary,
        } = self.board.0.refresh(&mut self.rng.0);

        let extinctions = [
            self.stats.aphids > 0 && summary.aphids == 0,
            self.stats.ladybugs > 0 && summary.ladybugs == 0,
        ];
        let stats = &mut self.stats;
        stats.deltas = [
            summary.aphids as i64 - stats.aphids as i64,
            summary.ladybugs as i64 - stats.ladybugs as i64,
            i64::from(summary.food) - i64::from(stats.food),
        ];
        stats.turn += 1;
        stats.births = births;
        stats.deaths = deaths;
        stats.aphids = summary.aphids;
        stats.ladybugs = summary.ladybugs;
        stats.food = summary.food;
        stats.extinct = summary.is_extinct();
        self.history
            .record(stats.turn, summary.aphids, summary.ladybugs);
        if extinctions.iter().any(|extinct| *extinct) {
            self.history.event(stats.turn).extinctions = extinctions;
        }
        self.revision.0 += 1;
    }
}
