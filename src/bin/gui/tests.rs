//! GUI tests. They drive systems on a bare `World` rather than a running app,
//! so they need no window or GPU.

use crate::board::*;
use crate::chart::*;
use crate::creatures::*;
use crate::editing::*;
use crate::history::*;
use crate::hover::*;
use crate::input::*;
use crate::layout::*;
use crate::panel::*;
use crate::params::*;
use crate::run_control::*;
use crate::simulation::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;
use creature_life_cycle::{Coordinates, CreatureSnapshot, CreatureSnapshotKind, Random};

#[test]
fn percentages_validate_bounds_and_format_readably() {
    assert_eq!(parse_percentage("12.5"), Some(0.125));
    assert_eq!(parse_percentage(" 100 "), Some(1.0));
    assert_eq!(parse_percentage("0"), Some(0.0));
    for invalid in ["", ".", "-1", "100.1", "NaN", "inf", "70%"] {
        assert_eq!(parse_percentage(invalid), None, "{invalid}");
    }
    assert_eq!(percentage_text(0.7), "70");
    assert_eq!(percentage_text(0.125), "12.5");
    assert_eq!(percentage_text(0.0), "0");
}

#[test]
fn percentage_fields_commit_on_enter_or_blur_and_reject_invalid_drafts() {
    let mut world = world_mid_run();
    world.init_resource::<InputFocus>();
    world.init_resource::<ButtonInput<KeyCode>>();
    world.init_resource::<Messages<ParameterReset>>();
    let field = world.spawn((ProbField(0), EditableText::default())).id();
    let system = world.register_system(commit_parameter_fields);
    world
        .resource_mut::<InputFocus>()
        .set(field, bevy::input_focus::FocusCause::Navigated);
    world.run_system(system).unwrap();
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("12.5");
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    world.run_system(system).unwrap();
    assert_eq!(world.resource::<Params>().get(0), 0.125);
    world.resource_mut::<ButtonInput<KeyCode>>().clear();
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("101");
    world.resource_mut::<InputFocus>().clear();
    world.run_system(system).unwrap();
    assert_eq!(world.resource::<Params>().get(0), 0.125);
    assert_eq!(
        world
            .get::<EditableText>(field)
            .unwrap()
            .value()
            .to_string(),
        "12.5"
    );
    assert!(!world.resource::<StatusLine>().success);

    world
        .resource_mut::<InputFocus>()
        .set(field, bevy::input_focus::FocusCause::Navigated);
    world.run_system(system).unwrap();
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("34");
    world.resource_mut::<InputFocus>().clear();
    world.run_system(system).unwrap();
    assert_eq!(world.resource::<Params>().get(0), 0.34);
}

#[test]
fn restoring_a_parameter_group_wins_over_its_pending_draft() {
    let mut world = world_mid_run();
    world.init_resource::<InputFocus>();
    world.init_resource::<ButtonInput<KeyCode>>();
    world.init_resource::<Messages<ParameterReset>>();
    let field = world.spawn((ProbField(0), EditableText::default())).id();
    let system = world.register_system(commit_parameter_fields);
    world.resource_mut::<Params>().set(4, 0.33);
    world
        .resource_mut::<InputFocus>()
        .set(field, bevy::input_focus::FocusCause::Navigated);
    world.run_system(system).unwrap();
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("25");
    world.resource_mut::<InputFocus>().clear();
    world.write_message(ParameterReset(0, 4));
    world.run_system(system).unwrap();
    for index in 0..4 {
        assert_eq!(
            world.resource::<Params>().get(index),
            Params::default().get(index)
        );
    }
    assert_eq!(world.resource::<Params>().get(4), 0.33);
}

#[test]
fn chart_events_coalesce_rule_changes_and_disappear_when_reverted() {
    let mut world = world_mid_run();
    let original = world.resource::<Params>().get(0);
    world.resource_mut::<Params>().set(0, 0.25);
    world.run_system_once(apply_params).unwrap();
    world.resource_mut::<Params>().set(0, 0.35);
    world.run_system_once(apply_params).unwrap();
    let events = &world.resource::<History>().1;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].turn, 12);
    assert_eq!(events[0].changes[0], Some((original, 0.35)));
    world.resource_mut::<Params>().set(0, original);
    world.run_system_once(apply_params).unwrap();
    assert!(world.resource::<History>().1.is_empty());
}

#[test]
fn chart_records_each_species_extinction_once() {
    let mut world = world_mid_run();
    edit(&mut world, 0, 0, EditTool::Aphid, EditAction::Add);
    edit(&mut world, 3, 3, EditTool::Ladybug, EditAction::Add);
    for index in 0..9 {
        world.resource_mut::<Params>().set(index, 0.0);
    }
    world.run_system_once(apply_params).unwrap();
    for _ in 0..40 {
        world
            .run_system_once(|mut sim: Simulation| sim.step())
            .unwrap();
    }
    let history = world.resource::<History>();
    for species in 0..2 {
        let events: Vec<_> = history
            .1
            .iter()
            .filter(|event| event.extinctions[species])
            .collect();
        assert_eq!(events.len(), 1);
        let point = history
            .0
            .iter()
            .find(|point| point.turn == events[0].turn)
            .unwrap();
        assert_eq!(point.series(species), 0);
    }
}

#[test]
fn chart_events_follow_rolling_history_and_edit_undo() {
    let mut history = History::default();
    for turn in 0..HISTORY_LIMIT + 5 {
        history.record(turn, 1, 1);
        if turn == 0 || turn == HISTORY_LIMIT {
            history.event(turn).extinctions[0] = true;
        }
    }
    assert_eq!(history.1.len(), 1);
    assert_eq!(history.1[0].turn, HISTORY_LIMIT);
    let mut world = world_mid_run();
    world.resource_mut::<History>().event(11).changes[0] = Some((0.7, 0.8));
    edit(&mut world, 0, 0, EditTool::Aphid, EditAction::Add);
    assert!(world.resource::<History>().1.is_empty());
    run_action(&mut world, RunAction::Undo);
    assert_eq!(world.resource::<History>().1[0].turn, 11);
    assert_eq!(
        world.resource::<History>().1[0].changes[0],
        Some((0.7, 0.8))
    );
}

#[test]
fn chart_resizing_preserves_board_space_and_remembers_height() {
    let mut panel = ChartPanel::default();
    panel.apply(ChartAction::Larger, 860.0);
    assert_eq!(panel.effective_height(860.0), 260.0);
    panel.apply(ChartAction::Toggle, 860.0);
    assert_eq!(panel.effective_height(860.0), CHART_COLLAPSED_HEIGHT);
    panel.apply(ChartAction::Toggle, 860.0);
    assert_eq!(panel.effective_height(860.0), 260.0);
    panel.height = 1000.0;
    assert_eq!(panel.effective_height(640.0), 420.0);
    assert!(640.0 - panel.effective_height(640.0) >= 180.0);
    let plot = plot_rect(600.0, panel.effective_height(640.0));
    assert!(plot.height() > 0.0);
    panel.height = 0.0;
    assert_eq!(panel.effective_height(640.0), CHART_MIN_HEIGHT);
}

#[test]
fn distant_markers_aggregate_crowded_cells_and_keep_both_species() {
    let mut world = world_mid_run();
    edit(&mut world, 1, 1, EditTool::Aphid, EditAction::Add);
    let one = population_mesh(&world.resource::<BoardRes>().0).count_vertices();
    edit(&mut world, 1, 1, EditTool::Aphid, EditAction::Add);
    let crowded = population_mesh(&world.resource::<BoardRes>().0).count_vertices();
    assert_eq!(
        one, crowded,
        "markers represent occupied cells, not individual creatures"
    );
    edit(&mut world, 1, 1, EditTool::Ladybug, EditAction::Add);
    let mixed = population_mesh(&world.resource::<BoardRes>().0).count_vertices();
    assert!(
        mixed > crowded,
        "mixed cells retain a marker for each species"
    );
}

#[test]
fn axis_uses_clean_steps_and_at_most_four_intervals() {
    for max in [0, 1, 4, 6, 9, 45, 100, 101, 999, 12_345] {
        let (top, step) = nice_axis(max);
        assert!(top >= max, "max {max}: top {top}");
        assert_eq!(top % step, 0, "max {max}: top {top} step {step}");
        assert!(top / step <= 4, "max {max}: top {top} step {step}");
        let magnitude = 10usize.pow(step.ilog10());
        assert!([1, 2, 5].contains(&(step / magnitude)), "step {step}");
    }
    assert_eq!(nice_axis(45), (60, 20));
    assert_eq!(nice_axis(0), (4, 1));
}

#[test]
fn nearest_index_snaps_to_turns_and_clamps() {
    let plot = Rect::new(0.0, 0.0, 100.0, 50.0);
    assert_eq!(nearest_index(-20.0, 11, plot), 0);
    assert_eq!(nearest_index(49.0, 11, plot), 5);
    assert_eq!(nearest_index(500.0, 11, plot), 10);
    assert_eq!(nearest_index(30.0, 1, plot), 0);
}

#[test]
fn history_drops_the_oldest_turn_past_the_limit() {
    let mut history = History::default();
    for turn in 0..HISTORY_LIMIT + 5 {
        history.record(turn, turn, 0);
    }
    assert_eq!(history.0.len(), HISTORY_LIMIT);
    assert_eq!(history.0.front().map(|point| point.turn), Some(5));
    assert_eq!(
        history.0.back().map(|point| point.turn),
        Some(HISTORY_LIMIT + 4)
    );
}

#[test]
fn dashes_stay_dashed_when_segments_are_shorter_than_a_dash() {
    // A full history across a narrow plot: 240 turns at 3px per turn, far
    // shorter than the 7px dash. Restarting per segment would draw this
    // solid; carrying the phase keeps the ink at DASH / DASH_PERIOD.
    let points: Vec<Vec2> = (0..HISTORY_LIMIT)
        .map(|turn| Vec2::new(turn as f32 * 3.0, 0.0))
        .collect();
    let total = 3.0 * (HISTORY_LIMIT - 1) as f32;
    let inked: f32 = dash_segments(&points)
        .iter()
        .map(|(start, end)| start.distance(*end))
        .sum();
    let expected = total * DASH / DASH_PERIOD;
    assert!(
        (inked - expected).abs() < DASH,
        "inked {inked}, expected about {expected} of {total}"
    );
}

#[test]
fn dashes_follow_the_line_through_its_corners() {
    let points = [Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0)];
    // 20px of line: dash at 0..7, gap to 12, dash at 12..19. The second dash
    // starts 2px up the vertical leg; restarting the pattern at the corner
    // would have started it at (10, 0) instead.
    assert_eq!(
        dash_segments(&points),
        vec![
            (Vec2::new(0.0, 0.0), Vec2::new(7.0, 0.0)),
            (Vec2::new(10.0, 2.0), Vec2::new(10.0, 9.0)),
        ]
    );
}

#[test]
fn one_speckle_per_unit_of_food_inside_the_cell_and_above_the_bar() {
    for food in 0..=MAX_FOOD {
        for (row, col) in [(0, 0), (29, 61), (99, 99)] {
            let offsets: Vec<Vec2> = speckle_offsets(row, col, food).collect();
            assert_eq!(
                offsets.len(),
                food as usize,
                "food {food} at ({row}, {col})"
            );
            for offset in offsets {
                assert!((0.0..1.0).contains(&offset.x), "{offset:?}");
                assert!(
                    offset.y + 0.02 < 0.82,
                    "speckle reaches the bar: {offset:?}"
                );
            }
        }
    }
    assert_eq!(speckle_offsets(0, 0, -3).count(), 0);
    assert_eq!(speckle_offsets(0, 0, 40).count(), MAX_FOOD as usize);
}

/// A 4x4 board with no creatures, mid-run: turn 12, playing, 12 turns of
/// history.
fn world_mid_run() -> World {
    let config = "[board]\nrows = 4\ncolumns = 4\naphids = []\nladybugs = []\n";
    let mut random = Random::with_seed(7);
    let board = creature_life_cycle::parse_simulation_config(config, &mut random)
        .unwrap()
        .board;

    let mut history = History::default();
    for turn in 0..12 {
        history.record(turn, 5, 5);
    }
    let mut world = World::new();
    world.insert_resource(StartingSetup(
        creature_life_cycle::format_simulation_config(&board),
    ));
    world.insert_resource(Params {
        aphid: board.aphid_params(),
        ladybug: board.ladybug_params(),
        food: board.food_params(),
    });
    world.insert_resource(SavedParams(*world.resource::<Params>()));
    world.insert_resource(StepTimer(Timer::from_seconds(
        STEP_SECONDS,
        TimerMode::Repeating,
    )));
    world.init_resource::<EditMode>();
    world.init_resource::<EditHistory>();
    world.init_resource::<StatusLine>();
    world.insert_resource(Seed(DEFAULT_SEED));
    world.init_resource::<Messages<RunAction>>();
    world.init_resource::<Messages<Reseed>>();
    world.insert_resource(BoardRes(board));
    world.insert_resource(Rng(random));
    world.insert_resource(Stats {
        turn: 12,
        births: 3,
        deaths: 2,
        ..default()
    });
    world.insert_resource(Revision(0));
    world.insert_resource(Playing(true));
    world.insert_resource(history);
    world
}

fn edit(world: &mut World, row: usize, col: usize, tool: EditTool, action: EditAction) -> bool {
    world
        .run_system_once(move |mut sim: Simulation| {
            sim.edit(CellEdit {
                row,
                col,
                tool,
                action,
            })
        })
        .unwrap()
}

fn run_action(world: &mut World, action: RunAction) {
    world.resource_mut::<Messages<RunAction>>().clear();
    world.write_message(action);
    world.run_system_once(apply_run_actions).unwrap();
}

fn reseed(world: &mut World, typed: &str) {
    world.resource_mut::<Messages<Reseed>>().clear();
    world.resource_mut::<Messages<RunAction>>().clear();
    let field = world.spawn((SeedField, EditableText::new(typed))).id();
    world.write_message(Reseed);
    world.run_system_once(apply_seed).unwrap();
    world.despawn(field);
}

/// Every cell's food, which the seed decides when a run is built.
fn food_layout(world: &World) -> Vec<i32> {
    let board = &world.resource::<BoardRes>().0;
    (0..board.rows())
        .flat_map(|row| (0..board.cols()).map(move |col| (row, col)))
        .filter_map(|(row, col)| board.cell_snapshot(row, col).map(|cell| cell.food))
        .collect()
}

#[test]
fn a_typed_seed_is_normalised_and_restarts_the_run_from_itself() {
    let mut world = world_mid_run();
    reseed(&mut world, " 007 ");

    assert_eq!(world.resource::<Seed>().0, 7);
    let status = world.resource::<StatusLine>();
    assert!(status.success, "{}", status.message);
    assert_eq!(status.message, "Restarted from seed 7.");

    // The restart itself is `RunAction::Reset`, so there is one restart path.
    assert_eq!(world.resource::<Messages<RunAction>>().len(), 1);
    world.run_system_once(apply_run_actions).unwrap();
    let from_seven = food_layout(&world);
    assert_eq!(world.resource::<Stats>().turn, 0);

    // The same seed rebuilds the same field; a different one does not.
    run_action(&mut world, RunAction::Reset);
    assert_eq!(food_layout(&world), from_seven);
    reseed(&mut world, "12345");
    world.run_system_once(apply_run_actions).unwrap();
    assert_ne!(food_layout(&world), from_seven);
}

#[test]
fn an_unreadable_seed_is_reported_and_leaves_the_run_alone() {
    let mut world = world_mid_run();
    reseed(&mut world, "");

    assert_eq!(world.resource::<Seed>().0, DEFAULT_SEED);
    assert!(
        world.resource::<Messages<RunAction>>().is_empty(),
        "a seed that cannot be read must not restart the run"
    );
    let status = world.resource::<StatusLine>();
    assert!(!status.success);
    assert_eq!(
        status.message,
        "Enter a whole seed from 0 to 18446744073709551615."
    );
}

#[test]
fn a_focused_seed_field_keeps_the_keyboard_to_itself() {
    let mut world = world_mid_run();
    world.init_resource::<InputFocus>();
    world.init_resource::<FoodOverlay>();
    world.init_resource::<BoardView>();
    world.init_resource::<ActiveTab>();
    world.init_resource::<PropertiesPanel>();
    let field = world.spawn((SeedField, EditableText::new("7"))).id();
    world
        .resource_mut::<InputFocus>()
        .set(field, bevy::input_focus::FocusCause::Navigated);

    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(KeyCode::KeyN);
    keys.press(KeyCode::Enter);
    world.insert_resource(keys);

    world.run_system_once(keyboard_controls).unwrap();
    assert!(
        world.resource::<Messages<RunAction>>().is_empty(),
        "typing a seed must not step the run"
    );
    world.run_system_once(seed_field_keyboard).unwrap();
    assert_eq!(world.resource::<Messages<Reseed>>().len(), 1);
}

fn save_action(world: &mut World) {
    world.resource_mut::<Messages<RunAction>>().clear();
    world.write_message(RunAction::Save);
    world.run_system_once(save_setup).unwrap();
}

/// The save, the failure, and the backup cases live in one test: they set
/// `XDG_CONFIG_HOME`, which is process-wide, so they cannot run in parallel.
#[test]
fn saving_writes_the_edited_board_and_makes_it_what_reset_restores() {
    let output = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-output");
    let config_home = output.join("bevy_save_setup");
    let _ = std::fs::remove_dir_all(&config_home);
    std::fs::create_dir_all(&output).unwrap();
    // `save_configured_board` resolves its own path from the environment, and
    // this is the only test in this binary that reads it.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &config_home) };

    let mut world = world_mid_run();
    world.resource_mut::<Params>().aphid.prob_move = 0.31;
    assert!(edit(&mut world, 1, 2, EditTool::Aphid, EditAction::Add));
    save_action(&mut world);

    let status = world.resource::<StatusLine>();
    assert!(status.success, "{}", status.message);
    assert_eq!(status.remaining, STATUS_SECONDS);

    // The slider values reach the file even though `apply_params` never ran.
    let saved = std::fs::read_to_string(
        config_home
            .join("creature_life_cycle")
            .join("simulation.toml"),
    )
    .unwrap();
    let mut random = Random::with_seed(1);
    let reloaded = creature_life_cycle::parse_simulation_config(&saved, &mut random)
        .unwrap()
        .board;
    assert_eq!(reloaded.cell_counts(1, 2), Some((1, 0)));
    assert_eq!(reloaded.aphid_params().prob_move, 0.31);

    // Reset follows the save, as the previous macroquad GUI's reset does by reloading
    // the config file.
    let saved_setup = world.resource::<StartingSetup>().0.clone();
    run_action(&mut world, RunAction::Reset);
    let board = &world.resource::<BoardRes>().0;
    assert_eq!(board.cell_counts(1, 2), Some((1, 0)));
    assert_eq!(board.summary().aphids, 1);

    // A config home that is a file, not a directory, so the write fails. A
    // failed save is reported and changes nothing.
    let blocked = output.join("bevy_save_blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &blocked) };
    save_action(&mut world);

    let status = world.resource::<StatusLine>();
    assert!(!status.success);
    assert!(
        status.message.starts_with("Save failed:"),
        "{}",
        status.message
    );
    assert_eq!(world.resource::<StartingSetup>().0, saved_setup);

    // Saving over a file that does not parse moves it aside first, so a
    // broken hand edit survives, and the message stays until replaced.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &config_home) };
    let config_dir = config_home.join("creature_life_cycle");
    let broken = "[board]\nrows = 3\ncolums = 4\n";
    std::fs::write(config_dir.join("simulation.toml"), broken).unwrap();
    save_action(&mut world);

    let status = world.resource::<StatusLine>();
    assert!(status.success, "{}", status.message);
    assert_eq!(
        status.message,
        "Saved. The invalid file is now simulation.toml.bak."
    );
    assert_eq!(status.remaining, f32::INFINITY);
    assert_eq!(
        std::fs::read_to_string(config_dir.join("simulation.toml.bak")).unwrap(),
        broken
    );
    let saved = std::fs::read_to_string(config_dir.join("simulation.toml")).unwrap();
    creature_life_cycle::parse_simulation_config(&saved, &mut random).unwrap();
}

#[test]
fn a_save_message_clears_itself_once_it_has_had_its_time() {
    let mut world = world_mid_run();
    world.resource_mut::<StatusLine>().set("Saved.", true);
    let mut time = Time::<()>::default();
    time.advance_by(std::time::Duration::from_secs_f32(STATUS_SECONDS / 2.0));
    world.insert_resource(time);

    world.run_system_once(expire_status).unwrap();
    assert_eq!(world.resource::<StatusLine>().message, "Saved.");

    world.run_system_once(expire_status).unwrap();
    assert!(world.resource::<StatusLine>().message.is_empty());
    assert_eq!(world.resource::<StatusLine>().remaining, 0.0);
}

#[test]
fn switching_food_details_off_hands_the_board_view_back() {
    let mut world = world_mid_run();
    world.init_resource::<InputFocus>();
    world.init_resource::<FoodOverlay>();
    world.init_resource::<BoardView>();
    world.init_resource::<ActiveTab>();
    world.init_resource::<PropertiesPanel>();
    let press_f = |world: &mut World| {
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyF);
        world.insert_resource(keys);
        world.run_system_once(keyboard_controls).unwrap();
    };

    // The details select the food view, and give it back on the way out,
    // so the board is the neutral shading it started with.
    press_f(&mut world);
    assert!(world.resource::<FoodOverlay>().on);
    assert_eq!(*world.resource::<BoardView>(), BoardView::Food);
    press_f(&mut world);
    assert!(!world.resource::<FoodOverlay>().on);
    assert_eq!(*world.resource::<BoardView>(), BoardView::Population);

    // A view picked on its own keeps the food shading when the details go.
    *world.resource_mut::<BoardView>() = BoardView::Food;
    press_f(&mut world);
    press_f(&mut world);
    assert!(!world.resource::<FoodOverlay>().on);
    assert_eq!(*world.resource::<BoardView>(), BoardView::Food);
}

#[test]
fn focused_widgets_do_not_also_trigger_global_space_shortcut() {
    let mut world = world_mid_run();
    world.init_resource::<InputFocus>();
    world.init_resource::<FoodOverlay>();
    world.init_resource::<BoardView>();
    world.init_resource::<ActiveTab>();
    world.init_resource::<PropertiesPanel>();
    let focused_button = world.spawn_empty().id();
    world
        .resource_mut::<InputFocus>()
        .set(focused_button, bevy::input_focus::FocusCause::Navigated);
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(KeyCode::Space);
    world.insert_resource(keys);
    world.run_system_once(keyboard_controls).unwrap();
    assert!(world.resource::<Messages<RunAction>>().is_empty());
    world.resource_mut::<InputFocus>().clear();
    world.run_system_once(keyboard_controls).unwrap();
    assert_eq!(world.resource::<Messages<RunAction>>().len(), 1);
}

#[test]
fn single_step_pauses_and_clears_the_pending_timer() {
    let mut world = world_mid_run();
    world
        .resource_mut::<StepTimer>()
        .0
        .tick(std::time::Duration::from_secs_f32(0.4));
    run_action(&mut world, RunAction::Step);
    assert_eq!(world.resource::<Stats>().turn, 13);
    assert!(!world.resource::<Playing>().0);
    assert_eq!(world.resource::<StepTimer>().0.elapsed_secs(), 0.0);
    // An extinct run cannot keep adding empty history points via Step.
    run_action(&mut world, RunAction::Step);
    assert_eq!(world.resource::<Stats>().turn, 13);
}

#[test]
fn reset_restores_starting_board_and_keeps_rules_with_repeatable_rng() {
    let mut world = world_mid_run();
    world.resource_mut::<StartingSetup>().0 =
        "[board]\nrows=4\ncolumns=4\naphids=[{x=1,y=2}]\nladybugs=[{x=2,y=1}]\n".into();
    world.resource_mut::<Params>().aphid.prob_move = 0.88;
    edit(&mut world, 3, 3, EditTool::Ladybug, EditAction::Add);
    world.resource_mut::<EditMode>().enabled = true;
    run_action(&mut world, RunAction::Reset);
    let first = world.resource::<BoardRes>().0.summary();
    assert_eq!((first.aphids, first.ladybugs), (1, 1));
    assert_eq!(world.resource::<Stats>().turn, 0);
    assert_eq!(world.resource::<History>().0.len(), 1);
    assert_eq!(
        world.resource::<BoardRes>().0.aphid_params().prob_move,
        0.88
    );
    assert!(!world.resource::<Playing>().0);
    assert!(!world.resource::<EditMode>().enabled);
    run_action(&mut world, RunAction::Step);
    let after_step = world.resource::<BoardRes>().0.creature_snapshots();
    run_action(&mut world, RunAction::Reset);
    assert_eq!(world.resource::<BoardRes>().0.summary(), first);
    run_action(&mut world, RunAction::Step);
    assert_eq!(
        world.resource::<BoardRes>().0.creature_snapshots(),
        after_step
    );
}

#[test]
fn fit_restores_camera_and_sidebars_are_outside_board_hit_area() {
    let mut world = world_mid_run();
    let camera = world
        .spawn((
            BoardCamera,
            Transform::from_xyz(99.0, 99.0, 999.0),
            Projection::Orthographic(OrthographicProjection {
                scale: 3.0,
                ..OrthographicProjection::default_2d()
            }),
        ))
        .id();
    run_action(&mut world, RunAction::Fit);
    assert_eq!(
        world.get::<Transform>(camera).unwrap().translation,
        Vec3::new(48.0, -48.0, 999.0)
    );
    let Projection::Orthographic(projection) = world.get::<Projection>(camera).unwrap() else {
        panic!()
    };
    assert_eq!(projection.scale, 1.0);
    let window = Window {
        resolution: (900u32, 640u32).into(),
        ..default()
    };
    let mut properties = PropertiesPanel::default();
    assert!(!board_rect(&window, CHART_HEIGHT, &properties).contains(Vec2::new(40.0, 40.0)));
    assert!(board_rect(&window, CHART_HEIGHT, &properties).contains(Vec2::new(800.0, 40.0)));
    properties.open = true;
    assert!(!board_rect(&window, CHART_HEIGHT, &properties).contains(Vec2::new(800.0, 40.0)));
    assert!(board_rect(&window, CHART_HEIGHT, &properties).contains(Vec2::new(500.0, 40.0)));
    assert!(!board_rect(&window, CHART_HEIGHT, &properties).contains(Vec2::new(500.0, 600.0)));
}

#[test]
fn an_edit_restarts_the_run_from_the_edited_board() {
    let mut world = world_mid_run();
    assert!(edit(&mut world, 1, 2, EditTool::Ladybug, EditAction::Add));
    assert!(edit(&mut world, 1, 2, EditTool::Aphid, EditAction::Add));

    let cell = world.resource::<BoardRes>().0.cell_snapshot(1, 2).unwrap();
    assert_eq!((cell.aphids, cell.ladybugs), (1, 1));

    let stats = world.resource::<Stats>();
    assert_eq!((stats.turn, stats.births, stats.deaths), (0, 0, 0));
    assert_eq!((stats.aphids, stats.ladybugs), (1, 1));
    assert!(!stats.extinct);
    assert!(!world.resource::<Playing>().0, "editing pauses the run");

    let history = &world.resource::<History>().0;
    assert_eq!(history.len(), 1, "history starts over");
    assert_eq!(
        history.front().map(|p| (p.turn, p.aphids, p.ladybugs)),
        Some((0, 1, 1))
    );
    assert_eq!(world.resource::<Revision>().0, 2, "renderers see each edit");
}

#[test]
fn removing_takes_one_creature_of_the_selected_kind_only() {
    let mut world = world_mid_run();
    edit(&mut world, 0, 0, EditTool::Aphid, EditAction::Add);
    edit(&mut world, 0, 0, EditTool::Aphid, EditAction::Add);
    edit(&mut world, 0, 0, EditTool::Ladybug, EditAction::Add);

    assert!(edit(&mut world, 0, 0, EditTool::Aphid, EditAction::Remove));
    let cell = world.resource::<BoardRes>().0.cell_snapshot(0, 0).unwrap();
    assert_eq!((cell.aphids, cell.ladybugs), (1, 1));
}

#[test]
fn a_no_op_edit_leaves_the_run_alone() {
    let mut world = world_mid_run();
    assert!(!edit(
        &mut world,
        3,
        3,
        EditTool::Ladybug,
        EditAction::Remove
    ));

    assert_eq!(world.resource::<Stats>().turn, 12);
    assert!(world.resource::<Playing>().0);
    assert_eq!(world.resource::<History>().0.len(), 12);
    assert_eq!(world.resource::<Revision>().0, 0);
    assert!(world.resource::<EditHistory>().0.is_empty());
}

#[test]
fn erase_and_undo_restore_creatures_food_and_history() {
    let mut world = world_mid_run();
    edit(&mut world, 1, 2, EditTool::Aphid, EditAction::Add);
    edit(&mut world, 1, 2, EditTool::Aphid, EditAction::Add);
    edit(&mut world, 1, 2, EditTool::Ladybug, EditAction::Add);
    let before = world.resource::<BoardRes>().0.cell_snapshot(1, 2).unwrap();
    let creatures = world.resource::<BoardRes>().0.creature_snapshots();

    assert!(edit(&mut world, 1, 2, EditTool::Erase, EditAction::Add));
    let cleared = world.resource::<BoardRes>().0.cell_snapshot(1, 2).unwrap();
    assert_eq!((cleared.aphids, cleared.ladybugs), (0, 0));
    assert_eq!(cleared.food, before.food);
    assert!(world.resource::<Stats>().extinct);

    run_action(&mut world, RunAction::Undo);
    assert_eq!(
        world.resource::<BoardRes>().0.cell_snapshot(1, 2),
        Some(before)
    );
    assert_eq!(
        world.resource::<BoardRes>().0.creature_snapshots(),
        creatures
    );
    assert!(!world.resource::<Playing>().0);
    assert!(world.resource::<EditMode>().enabled);

    for _ in 0..3 {
        run_action(&mut world, RunAction::Undo);
    }
    assert_eq!(world.resource::<Stats>().turn, 12);
    assert_eq!(world.resource::<History>().0.len(), 12);
    assert!(world.resource::<EditHistory>().0.is_empty());
    let revision = world.resource::<Revision>().0;
    run_action(&mut world, RunAction::Undo);
    assert_eq!(
        world.resource::<Revision>().0,
        revision,
        "empty undo is a no-op"
    );
}

#[test]
fn undo_restores_random_state_and_keeps_current_parameters() {
    let mut world = world_mid_run();
    edit(&mut world, 1, 1, EditTool::Ladybug, EditAction::Add);
    edit(&mut world, 1, 1, EditTool::Aphid, EditAction::Add);
    let mut expected_board = world.resource::<BoardRes>().0.clone();
    let mut expected_random = world.resource::<Rng>().0.clone();
    // Placing a ladybug consumes random numbers for its preferred directions.
    edit(&mut world, 2, 2, EditTool::Ladybug, EditAction::Add);
    world.resource_mut::<Params>().aphid.prob_move = 0.31;
    expected_board.set_aphid_params(world.resource::<Params>().aphid);
    run_action(&mut world, RunAction::Undo);
    assert_eq!(
        world.resource::<History>().1.back().unwrap().changes[0],
        Some((0.7, 0.31)),
        "undo records the current rules at the restored turn boundary"
    );
    for _ in 0..8 {
        let expected = expected_board.refresh(&mut expected_random);
        world
            .run_system_once(|mut sim: Simulation| sim.step())
            .unwrap();
        assert_eq!(world.resource::<BoardRes>().0.summary(), expected.summary);
        assert_eq!(
            world.resource::<BoardRes>().0.creature_snapshots(),
            expected_board.creature_snapshots()
        );
    }
}

#[test]
fn undo_then_replacing_a_species_updates_its_sprite_in_the_same_frame() {
    let mut world = world_mid_run();
    let mut images = Assets::<Image>::default();
    let aphid = images.add(Image::default());
    let ladybug = images.add(Image::default());
    world.insert_resource(CreatureAssets {
        aphid: aphid.clone(),
        ladybug: ladybug.clone(),
        badge: Handle::default(),
        badge_shadow_mesh: Handle::default(),
        badge_colours: [Handle::default(), Handle::default()],
        badge_shadow: Handle::default(),
        font: Handle::default(),
    });
    world.init_resource::<CreatureIndex>();
    edit(&mut world, 1, 1, EditTool::Aphid, EditAction::Add);
    world.run_system_once(sync_creatures).unwrap();
    let entity = *world.resource::<CreatureIndex>().0.values().next().unwrap();
    assert_eq!(world.get::<Sprite>(entity).unwrap().image, aphid);

    run_action(&mut world, RunAction::Undo);
    edit(&mut world, 1, 1, EditTool::Ladybug, EditAction::Add);
    world.run_system_once(sync_creatures).unwrap();
    assert_eq!(world.resource::<CreatureIndex>().0.len(), 1);
    assert_eq!(world.get::<Sprite>(entity).unwrap().image, ladybug);
}

#[test]
fn undo_is_bounded_and_cleared_by_step_and_reset() {
    let mut world = world_mid_run();
    for _ in 0..EDIT_UNDO_LIMIT + 3 {
        edit(&mut world, 1, 1, EditTool::Aphid, EditAction::Add);
    }
    assert_eq!(world.resource::<EditHistory>().0.len(), EDIT_UNDO_LIMIT);
    for _ in 0..EDIT_UNDO_LIMIT {
        run_action(&mut world, RunAction::Undo);
    }
    assert_eq!(world.resource::<BoardRes>().0.summary().aphids, 3);
    edit(&mut world, 0, 0, EditTool::Aphid, EditAction::Add);
    run_action(&mut world, RunAction::Step);
    assert!(world.resource::<EditHistory>().0.is_empty());
    edit(&mut world, 0, 0, EditTool::Aphid, EditAction::Add);
    run_action(&mut world, RunAction::Reset);
    assert!(world.resource::<EditHistory>().0.is_empty());
}

#[test]
fn population_deltas_match_the_last_turn_and_reset_after_edits() {
    let mut world = world_mid_run();
    edit(&mut world, 1, 1, EditTool::Aphid, EditAction::Add);
    let before = world.resource::<BoardRes>().0.summary();
    run_action(&mut world, RunAction::Step);
    let after = world.resource::<BoardRes>().0.summary();
    assert_eq!(
        world.resource::<Stats>().deltas,
        [
            after.aphids as i64 - before.aphids as i64,
            after.ladybugs as i64 - before.ladybugs as i64,
            i64::from(after.food) - i64::from(before.food),
        ]
    );
    edit(&mut world, 2, 2, EditTool::Ladybug, EditAction::Add);
    assert_eq!(world.resource::<Stats>().deltas, [0; 3]);
}

#[test]
fn editing_and_playing_exclude_each_other() {
    let mut world = World::new();
    world.insert_resource(Playing(true));
    world.insert_resource(EditMode::default());
    let system = world.register_system(exclusive_play_and_edit);
    world.run_system(system).unwrap();
    assert!(world.resource::<Playing>().0, "nothing to do at startup");

    // Starting to edit pauses the run...
    world.resource_mut::<EditMode>().select(EditTool::Ladybug);
    world.run_system(system).unwrap();
    assert!(!world.resource::<Playing>().0);
    assert!(world.resource::<EditMode>().enabled);

    // ...and pressing play ends editing, keeping the chosen tool.
    world.resource_mut::<Playing>().0 = true;
    world.run_system(system).unwrap();
    assert!(world.resource::<Playing>().0);
    let edit = world.resource::<EditMode>();
    assert!(!edit.enabled);
    assert_eq!(edit.tool, EditTool::Ladybug);
}

#[test]
fn a_press_is_a_click_until_it_leaves_the_slop() {
    let mut pointer = BoardPointer {
        pressed_on_board: true,
        ..default()
    };
    assert!(pointer.is_click());
    pointer.travelled = CLICK_SLOP;
    assert!(pointer.is_click());
    pointer.travelled = CLICK_SLOP + 0.5;
    assert!(!pointer.is_click(), "a drag pans instead");

    pointer.travelled = 0.0;
    pointer.pressed_on_board = false;
    assert!(
        !pointer.is_click(),
        "presses that start off the board never edit"
    );
}

#[test]
fn mixed_cells_keep_aphids_and_ladybugs_apart() {
    use CreatureSnapshotKind::{Aphid, Ladybug};

    // The case that hid a creature: the first of each kind in one cell.
    let (aphid, _) = creature_slot(Aphid, 0, 1, 1);
    let (ladybug, _) = creature_slot(Ladybug, 0, 1, 1);
    assert!(
        aphid.distance(ladybug) > 2.0 * CREATURE_RADIUS,
        "{aphid:?} and {ladybug:?} overlap"
    );

    // A cell with one kind centres its first creature.
    assert_eq!(creature_slot(Ladybug, 0, 0, 1).0, Vec2::new(0.5, 0.5));
    assert_eq!(creature_slot(Aphid, 0, 3, 0).0, Vec2::new(0.5, 0.5));

    // Crowded cells shrink creatures; extra creatures reuse the last slot.
    assert_eq!(creature_slot(Aphid, 0, 3, 2).1, 1.0);
    assert_eq!(creature_slot(Aphid, 0, 3, 3).1, 0.82);
    assert_eq!(
        creature_slot(Aphid, 40, 50, 1).0,
        creature_slot(Aphid, 4, 50, 1).0
    );
}

fn snapshot(id: usize, row: usize, col: usize, kind: CreatureSnapshotKind) -> CreatureSnapshot {
    CreatureSnapshot {
        id,
        kind,
        location: Coordinates { x: row, y: col },
        cell_slot: 0,
    }
}

#[test]
fn badges_mark_only_cells_holding_more_than_one_of_a_kind() {
    use CreatureSnapshotKind::{Aphid, Ladybug};
    let snapshots = [
        // Three aphids and two ladybugs share (1, 1): both get a badge.
        snapshot(0, 1, 1, Aphid),
        snapshot(1, 1, 1, Aphid),
        snapshot(2, 1, 1, Aphid),
        snapshot(3, 1, 1, Ladybug),
        snapshot(4, 1, 1, Ladybug),
        // One of each in (2, 2), and a lone aphid: no badges.
        snapshot(5, 2, 2, Aphid),
        snapshot(6, 2, 2, Ladybug),
        snapshot(7, 0, 3, Aphid),
    ];
    assert_eq!(
        crowded_cells(&snapshots),
        vec![
            (Coordinates { x: 1, y: 1 }, 0, 3),
            (Coordinates { x: 1, y: 1 }, 1, 2),
        ]
    );
    assert!(crowded_cells(&[]).is_empty());
}

#[test]
fn badges_appear_relabel_and_are_cleaned_up() {
    let mut world = World::new();
    let mut meshes = Assets::<Mesh>::default();
    let mut materials = Assets::<ColorMaterial>::default();
    let circle = meshes.add(Circle::new(1.0));
    let colour = materials.add(ColorMaterial::default());
    world.insert_resource(CreatureAssets {
        aphid: Handle::default(),
        ladybug: Handle::default(),
        badge: circle.clone(),
        badge_shadow_mesh: circle.clone(),
        badge_colours: [colour.clone(), colour.clone()],
        badge_shadow: colour.clone(),
        font: Handle::default(),
    });
    world.insert_resource(meshes);
    world.insert_resource(materials);
    world.init_resource::<BadgeIndex>();
    world.init_resource::<BadgeRaster>();
    world.insert_resource(Revision(0));

    let config = "[board]\nrows = 4\ncolumns = 4\naphids = []\nladybugs = []\n";
    let mut random = Random::with_seed(7);
    let mut board = creature_life_cycle::parse_simulation_config(config, &mut random)
        .unwrap()
        .board;
    for _ in 0..3 {
        board.add_aphid(1, 1, EDIT_APHID_LIFE);
    }
    board.add_aphid(2, 2, EDIT_APHID_LIFE);
    world.insert_resource(BoardRes(board));

    let count_badges = |world: &mut World| {
        world
            .query_filtered::<Entity, With<CountBadge>>()
            .iter(world)
            .count()
    };
    let label = |world: &mut World| {
        let badge = *world.resource::<BadgeIndex>().0.values().next().unwrap();
        world.get::<Text2d>(badge.text).unwrap().0.clone()
    };

    world.run_system_once(sync_count_badges).unwrap();
    assert_eq!(
        world.resource::<BadgeIndex>().0.len(),
        1,
        "only (1, 1) is crowded"
    );
    assert_eq!(label(&mut world), "3");
    // Disc, shadow and text per badge.
    assert_eq!(count_badges(&mut world), 3);

    // Down to two aphids: the badge stays but is relabelled.
    world.resource_mut::<BoardRes>().0.remove_aphid_at(1, 1);
    world.run_system_once(sync_count_badges).unwrap();
    assert_eq!(label(&mut world), "2");
    assert_eq!(
        count_badges(&mut world),
        3,
        "no second badge for the same cell"
    );

    // Down to one: the badge and its children go away.
    world.resource_mut::<BoardRes>().0.remove_aphid_at(1, 1);
    world.run_system_once(sync_count_badges).unwrap();
    assert!(world.resource::<BadgeIndex>().0.is_empty());
    assert_eq!(count_badges(&mut world), 0, "badge entities are despawned");
}

#[test]
fn badge_counts_are_rasterised_at_the_size_they_are_drawn() {
    let height = (CELL - GAP) * BADGE_FONT;
    // Whatever the zoom, the badge keeps its size on the board...
    for pixels_per_unit in [0.5, 1.0, 2.333, 4.0, 12.0] {
        let (font_size, scale) = badge_text_raster(pixels_per_unit);
        assert!(
            (font_size * scale - height).abs() < 1e-3,
            "{pixels_per_unit}: {font_size} x {scale} is not {height}"
        );
    }

    // ...while the raster follows the zoom, in steps, within its range.
    let (zoomed_out, _) = badge_text_raster(1.0);
    let (default_zoom, _) = badge_text_raster(2.333);
    let (zoomed_in, _) = badge_text_raster(8.0);
    assert!(zoomed_out < default_zoom && default_zoom < zoomed_in);
    assert!(default_zoom >= height * 2.0, "atlas must not be upscaled");
    assert_eq!(default_zoom % BADGE_RASTER_STEP, 0.0);
    assert_eq!(badge_text_raster(0.01).0, BADGE_RASTER_RANGE.0);
    assert_eq!(badge_text_raster(1000.0).0, BADGE_RASTER_RANGE.1);
}

#[test]
fn the_movement_streak_grows_then_collapses_into_the_creature() {
    let (from, to) = (Vec2::ZERO, Vec2::new(CELL, 0.0));
    let streak = |t: f32| {
        let points: Vec<_> = trail_points(from, from.lerp(to, t), t).collect();
        let (tail, head) = (points[0].0, points[points.len() - 1].0);
        (head.distance(tail), points)
    };

    // The head is always the creature, the tail always fades to nothing,
    // and the alpha only ever climbs towards the head.
    for t in [0.0, 0.2, TRAIL_LAG, 0.6, 0.9, 1.0] {
        let (_, points) = streak(t);
        assert_eq!(points.len(), TRAIL_POINTS);
        assert_eq!(points[0].1, 0.0, "{t}: the tail must be invisible");
        assert_eq!(points[points.len() - 1].1, 1.0, "{t}: the head is solid");
        assert!(points.windows(2).all(|pair| pair[0].1 <= pair[1].1));
        assert!(
            points[points.len() - 1]
                .0
                .abs_diff_eq(from.lerp(to, t), 1e-4)
        );
    }

    // Until the tail is let go the streak is the whole distance travelled,
    // then it shortens to nothing rather than popping off the board.
    assert_eq!(streak(TRAIL_LAG).0, from.lerp(to, TRAIL_LAG).distance(from));
    assert!(streak(0.6).0 > streak(0.9).0);
    assert!(streak(1.0).0 < 1e-4, "a settled creature drags nothing");
}

#[test]
fn badges_hide_once_cells_are_too_small_to_read() {
    let cell = CELL - GAP;
    // A viewport showing 10 cells' worth of world is plenty at 800px tall.
    assert!(badges_legible(800.0, cell * 10.0));
    // The same viewport zoomed out to 40 cells is not.
    assert!(!badges_legible(800.0, cell * 40.0));
    // Exactly at the threshold counts as legible.
    assert!(badges_legible(BADGE_MIN_CELL_PIXELS * 10.0, cell * 10.0));
    assert!(
        !badges_legible(800.0, 0.0),
        "a collapsed viewport never draws"
    );
}

#[test]
fn every_creature_slot_stays_inside_its_cell() {
    use CreatureSnapshotKind::{Aphid, Ladybug};
    for (aphids, ladybugs) in [(5, 0), (0, 5), (5, 5)] {
        for kind in [Aphid, Ladybug] {
            for slot in 0..5 {
                let (offset, scale) = creature_slot(kind, slot, aphids, ladybugs);
                let radius = CREATURE_RADIUS * scale;
                assert!(
                    offset.min_element() - radius >= 0.0 && offset.max_element() + radius <= 1.0,
                    "{kind:?} slot {slot} in ({aphids}, {ladybugs}) spills out: {offset:?}"
                );
            }
        }
    }
}

#[test]
fn panel_scrolling_stays_within_the_content() {
    use MouseScrollUnit::{Line, Pixel};
    // Wheel down (negative delta) moves further into the content.
    assert_eq!(scrolled_offset(0.0, -2.0, Line, 500.0), 42.0);
    assert_eq!(scrolled_offset(100.0, 30.0, Pixel, 500.0), 70.0);
    assert_eq!(scrolled_offset(490.0, -3.0, Line, 500.0), 500.0);
    assert_eq!(scrolled_offset(10.0, 5.0, Line, 500.0), 0.0);
    // Content that fits never scrolls.
    assert_eq!(scrolled_offset(0.0, -5.0, Line, -80.0), 0.0);
}

#[test]
fn the_cell_popup_stays_on_screen() {
    // Room to spare: the popup sits just past the pointer.
    assert_eq!(clamp_to_window(400.0, 200.0, 1200.0), 418.0);
    // Near the far edge it is pulled back inside, margin and all.
    assert_eq!(clamp_to_window(1100.0, 200.0, 1200.0), 988.0);
    // Never off the near edge, however tight the window is.
    assert_eq!(clamp_to_window(-40.0, 200.0, 1200.0), 12.0);
    assert_eq!(clamp_to_window(400.0, 200.0, 150.0), 12.0);
}

#[test]
fn values_map_into_the_plot_with_zero_on_the_baseline() {
    let plot = plot_rect(800.0, CHART_HEIGHT);
    assert_eq!(value_y(0, 60, plot), plot.max.y);
    assert_eq!(value_y(60, 60, plot), plot.min.y);
    assert_eq!(history_x(0, 10, plot), plot.min.x);
    assert_eq!(history_x(9, 10, plot), plot.max.x);
}
