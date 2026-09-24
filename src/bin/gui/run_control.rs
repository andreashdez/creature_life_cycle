//! Playback, reset, seed, and save: the `RunAction` and `Reseed` messages and
//! the systems that carry them out, plus the status line that reports on them.

use crate::board::CELL;
use crate::editing::EditMode;
use crate::layout::BoardCamera;
use crate::params::{Params, SavedParams, apply_parameter_values};
use crate::simulation::{BoardRes, Seed, Simulation, StartingSetup, StepTimer};
use bevy::ecs::system::SystemParam;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::{EditableText, EditableTextFilter};
use creature_life_cycle::{Random, save_configured_board};

/// How long a message stays on the toolbar, matching `SaveStatus::ttl` in the
/// macroquad GUI.
pub const STATUS_SECONDS: f32 = 4.0;
/// Digits in `u64::MAX`, so the seed field cannot hold a number that could never
/// parse.
pub const SEED_DIGITS: usize = 20;

/// What the toolbar reports about the last thing the reader asked for, a save or
/// a seed, cleared after a few seconds as the previous macroquad GUI's footer status is.
#[derive(Resource, Default)]
pub struct StatusLine {
    pub message: String,
    pub success: bool,
    pub remaining: f32,
}

impl StatusLine {
    pub fn set(&mut self, message: impl Into<String>, success: bool) {
        self.message = message.into();
        self.success = success;
        self.remaining = STATUS_SECONDS;
    }

    /// Shows a message that stays until another one replaces it, for news the
    /// reader must not miss by looking away for a few seconds.
    pub fn pin(&mut self, message: impl Into<String>, success: bool) {
        self.message = message.into();
        self.success = success;
        self.remaining = f32::INFINITY;
    }
}

#[derive(Message, Clone, Copy)]
pub enum RunAction {
    TogglePlay,
    Step,
    Reset,
    Slower,
    Faster,
    Fit,
    Save,
    Undo,
    FinishEditing,
}

/// Asks for the seed in the panel's field to be applied and the run restarted.
#[derive(Message, Clone, Copy)]
pub struct Reseed;

/// The panel's seed field. Its text is a draft: it reaches the run only when the
/// reader applies it, so `Seed` and this field can disagree until then.
#[derive(Component, Clone, Default, FromTemplate)]
pub struct SeedField;

/// What a run is started from: the board, the rules, and the seed.
#[derive(SystemParam)]
pub struct RunSetup<'w> {
    pub start: Res<'w, StartingSetup>,
    pub params: Res<'w, Params>,
    pub seed: Res<'w, Seed>,
}

pub fn apply_run_actions(
    mut actions: MessageReader<RunAction>,
    mut sim: Simulation,
    mut timer: ResMut<StepTimer>,
    setup: RunSetup,
    mut edit: ResMut<EditMode>,
    mut camera: Query<(&mut Transform, &mut Projection), With<BoardCamera>>,
) {
    for action in actions.read() {
        match action {
            RunAction::TogglePlay => {
                if !sim.stats.extinct {
                    sim.playing.0 = !sim.playing.0;
                }
                timer.0.reset();
            }
            RunAction::Step => {
                sim.playing.0 = false;
                timer.0.reset();
                if !sim.stats.extinct {
                    sim.step();
                }
            }
            RunAction::FinishEditing => {
                edit.enabled = false;
                sim.playing.0 = false;
            }
            RunAction::Undo => {
                if sim.undo() {
                    // Parameter controls keep their current values after undo.
                    let turn = sim.stats.turn;
                    apply_parameter_values(&setup.params, &mut sim.board.0, &mut sim.history, turn);
                    timer.0.reset();
                    edit.enabled = true;
                }
            }
            RunAction::Reset => {
                sim.edits.0.clear();
                let mut random = Random::with_seed(setup.seed.0);
                let mut board =
                    creature_life_cycle::parse_simulation_config(&setup.start.0, &mut random)
                        .expect("the starting setup is generated from a valid board")
                        .board;
                board.set_aphid_params(setup.params.aphid);
                board.set_ladybug_params(setup.params.ladybug);
                board.set_food_params(setup.params.food);
                sim.board.0 = board;
                sim.rng.0 = random;
                sim.restart_from_edit();
                timer.0.reset();
                edit.enabled = false;
            }
            RunAction::Slower | RunAction::Faster => {
                let factor = if matches!(action, RunAction::Faster) {
                    0.5
                } else {
                    2.0
                };
                let seconds = (timer.0.duration().as_secs_f32() * factor).clamp(0.05, 4.0);
                timer
                    .0
                    .set_duration(std::time::Duration::from_secs_f32(seconds));
                timer.0.reset();
            }
            // `save_setup` reads the same stream and owns this one, which keeps
            // the file writing out of the playback handler.
            RunAction::Save => {}
            RunAction::Fit => {
                if let Ok((mut transform, mut projection)) = camera.single_mut() {
                    transform.translation.x = (sim.board.0.cols() as f32 - 1.0) * CELL * 0.5;
                    transform.translation.y = -(sim.board.0.rows() as f32 - 1.0) * CELL * 0.5;
                    if let Projection::Orthographic(ref mut ortho) = *projection {
                        ortho.scale = 1.0;
                    }
                }
            }
        }
    }
}

/// Feathers spawns the field empty, so fill it with the seed the run started
/// from and hold it to digits, as the previous macroquad GUI's field does.
pub fn init_seed_field(
    mut commands: Commands,
    seed: Res<Seed>,
    mut fields: Query<(Entity, &mut EditableText), Added<SeedField>>,
) {
    for (entity, mut field) in &mut fields {
        field.editor.set_text(&seed.0.to_string());
        commands
            .entity(entity)
            .insert(EditableTextFilter::new(|character| {
                character.is_ascii_digit()
            }));
    }
}

/// Applies the seed in the panel's field and restarts the run from it, as
/// `set_seed` does in the previous macroquad GUI. The restart itself is left to
/// `RunAction::Reset`, so there is one restart path rather than two.
/// Enter applies the seed while the field has focus, as Enter commits the
/// macroquad GUI's field.
pub fn seed_field_keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    fields: Query<(), With<SeedField>>,
    mut requests: MessageWriter<Reseed>,
) {
    if keys.just_pressed(KeyCode::Enter)
        && focus.get().is_some_and(|entity| fields.contains(entity))
    {
        requests.write(Reseed);
    }
}

pub fn apply_seed(
    mut requests: MessageReader<Reseed>,
    mut actions: MessageWriter<RunAction>,
    mut seed: ResMut<Seed>,
    mut status: ResMut<StatusLine>,
    mut fields: Query<&mut EditableText, With<SeedField>>,
) {
    if requests.read().next().is_none() {
        return;
    }
    let Ok(mut field) = fields.single_mut() else {
        return;
    };

    match field.value().to_string().trim().parse::<u64>() {
        Ok(value) => {
            // Echo back what was parsed, so leading zeros and stray spaces are
            // replaced by the seed the run actually uses.
            field.editor.set_text(&value.to_string());
            seed.0 = value;
            actions.write(RunAction::Reset);
            status.set(format!("Restarted from seed {value}."), true);
        }
        // An unreadable seed leaves the run alone, as a failed `commit` does in
        // the previous macroquad GUI.
        Err(_) => status.set(format!("Enter a whole seed from 0 to {}.", u64::MAX), false),
    }
}

/// Writes the current board and probabilities to the XDG config file, as
/// `save_config` does in the previous macroquad GUI.
pub fn save_setup(
    mut actions: MessageReader<RunAction>,
    mut board: ResMut<BoardRes>,
    mut start: ResMut<StartingSetup>,
    mut status: ResMut<StatusLine>,
    params: Res<Params>,
    mut saved: ResMut<SavedParams>,
) {
    if !actions
        .read()
        .any(|action| matches!(action, RunAction::Save))
    {
        return;
    }

    // The file records the parameters the board holds, so push the sliders'
    // current values in rather than depending on where this system is ordered
    // against `apply_params`.
    board.0.set_aphid_params(params.aphid);
    board.0.set_ladybug_params(params.ladybug);
    board.0.set_food_params(params.food);

    match save_configured_board(&board.0) {
        Ok(saved_config) => {
            // What was saved is the starting setup from now on, so Reset returns
            // to it instead of to the board loaded at launch.
            start.0 = creature_life_cycle::format_simulation_config(&board.0);
            saved.0 = *params;
            match saved_config.backup {
                Some(backup) => {
                    let name = backup.file_name().unwrap_or(backup.as_os_str());
                    status.pin(
                        format!("Saved. The invalid file is now {}.", name.display()),
                        true,
                    );
                }
                None => status.set("Starting setup saved.", true),
            }
        }
        Err(error) => status.set(format!("Save failed: {error}"), false),
    }
}

/// Clears the save message once it has had its time. The countdown bypasses
/// change detection so a fading message does not reflag the resource every
/// frame; only the clearing itself has to reach the label.
pub fn expire_status(time: Res<Time>, mut status: ResMut<StatusLine>) {
    if status.remaining <= 0.0 {
        return;
    }
    let remaining = status.remaining - time.delta_secs();
    if remaining > 0.0 {
        status.bypass_change_detection().remaining = remaining;
    } else {
        *status = StatusLine::default();
    }
}
