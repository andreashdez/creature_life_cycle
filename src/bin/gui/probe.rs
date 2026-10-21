//! The screenshot probe: with `CLC_SCREENSHOT` set, the app drives its own
//! controls through the real input paths, saves a series of captures, and
//! exits, so the GUI can be checked without a human watching it.

use crate::board::{BoardView, CELL, FoodOverlay, GAP, cell_to_world};
use crate::chart::{ChartAction, ChartControlButton, ChartHover};
use crate::creatures::{BADGE_SLOTS, badge_text_raster, badges_legible, creature_world};
use crate::editing::{CellEdit, EditAction, EditMode, EditTool, EditToolButton};
use crate::history::{
    ClearHistoryPin, EventList, EventListBody, EventRow, History, HistorySelection,
};
use crate::layout::BoardCamera;
use crate::panel::{
    FoodOverlayToggle, PanelRoot, PanelSection, PanelTab, PropertiesToggle, SectionToggle,
    TabButton, ViewButton, scrolled_offset,
};
use crate::params::{Params, ProbField, ProbStep};
use crate::run_control::RunAction;
use crate::simulation::{BoardRes, Playing, Stats, StepTimer};
use crate::summary::{SpeedMenuButton, SpeedMenuPopup, SpeedPreset};
use bevy::feathers::controls::ButtonVariant;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseScrollUnit;
use bevy::input_focus::{FocusCause, FocusedInput, InputFocus};
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use bevy::text::EditableText;
use bevy::ui::Checked;
use bevy::ui_widgets::{Activate, SetSelected, ToggleChecked, ValueChange};
use creature_life_cycle::Coordinates;

/// The screenshot probe can also verify the smallest supported window.
pub fn review_window_size() -> (u32, u32) {
    if std::env::var_os("CLC_SCREENSHOT").is_some() && std::env::var_os("CLC_COMPACT").is_some() {
        (900, 640)
    } else {
        (1200, 860)
    }
}

/// Set `CLC_SCREENSHOT=some/path.png` to let the app run a few turns, save a
/// frame, and quit. Lets the prototype be checked without a human watching it.
#[derive(Resource)]
pub struct ScreenshotProbe {
    pub path: String,
    pub timer: Timer,
    pub stage: u8,
}

/// Board cell the probe edits and hovers.
const PROBE_CELL: (usize, usize) = (2, 3);

pub type ProbeWidgets<'w, 's> = (
    Query<'w, 's, (Entity, Has<Checked>), With<FoodOverlayToggle>>,
    Query<'w, 's, (Entity, &'static EditToolButton, &'static ButtonVariant)>,
    Query<'w, 's, (Entity, &'static SectionToggle)>,
    Query<'w, 's, (Entity, &'static ViewButton)>,
    Query<'w, 's, (Entity, &'static ChartControlButton)>,
    Query<'w, 's, Entity, With<PropertiesToggle>>,
    Query<'w, 's, Entity, With<SpeedMenuButton>>,
    Query<'w, 's, &'static Visibility, With<SpeedMenuPopup>>,
    Query<'w, 's, (Entity, &'static SpeedPreset)>,
);

pub type ProbeState<'w> = (
    Res<'w, Params>,
    Res<'w, BoardRes>,
    Res<'w, History>,
    Res<'w, FoodOverlay>,
    Res<'w, EditMode>,
    Res<'w, Stats>,
    Res<'w, Playing>,
    Res<'w, StepTimer>,
);

/// Extra event-list state used to verify pinning and nested wheel routing.
pub type ProbeHistory<'w, 's> = (
    Query<'w, 's, (Entity, &'static EventRow)>,
    Query<'w, 's, Entity, With<EventList>>,
    Query<'w, 's, Entity, With<ClearHistoryPin>>,
    Query<'w, 's, (Entity, &'static ScrollPosition), (With<EventListBody>, Without<PanelRoot>)>,
    Res<'w, HistorySelection>,
    Res<'w, ChartHover>,
);

/// Send the same bubbling wheel event that picking dispatches, so the inner
/// list must consume it before the outer properties scroll area sees it.
fn probe_list_scroll(entity: Entity) -> bevy::picking::events::PointerScroll {
    use bevy::picking::{
        backend::HitData,
        events::Pointer,
        pointer::{Location, PointerId},
    };
    bevy::picking::events::PointerScroll {
        entity,
        pointer: Pointer::new(
            PointerId::Mouse,
            Location {
                target: bevy::camera::NormalizedRenderTarget::Window(
                    bevy::window::WindowRef::Entity(Entity::PLACEHOLDER)
                        .normalize(None)
                        .unwrap(),
                ),
                position: Vec2::ZERO,
            },
        ),
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: -100.0,
        hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
        phase: bevy::input::touch::TouchPhase::Moved,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "debug-only probe, not a hot path"
)]
pub fn screenshot_probe(
    mut commands: Commands,
    (time, probe): (Res<Time>, Option<ResMut<ScreenshotProbe>>),
    mut board_cameras: Query<
        (&Camera, &GlobalTransform, &mut Projection, &mut Transform),
        With<BoardCamera>,
    >,
    (
        food_toggles,
        tool_buttons,
        sections,
        views,
        chart_controls,
        properties_toggles,
        speed_buttons,
        speed_popups,
        speed_items,
    ): ProbeWidgets,
    (params, board, history, overlay, edit, stats, playing, step_timer): ProbeState,
    mut panel: Query<(&mut ScrollPosition, &ComputedNode), With<PanelRoot>>,
    tabs: Query<(Entity, &TabButton)>,
    mut edits: MessageWriter<CellEdit>,
    mut exit: MessageWriter<AppExit>,
    mut windows: Query<&mut Window>,
    mut actions: MessageWriter<RunAction>,
    mut focus: ResMut<InputFocus>,
    fields: Query<(Entity, &ProbField, &Children)>,
    mut texts: Query<&mut EditableText>,
    steps: Query<(Entity, &ProbStep)>,
    (event_rows, event_lists, clear_pins, event_bodies, selection, chart_hover): ProbeHistory,
) {
    let Some(mut probe) = probe else {
        return;
    };
    if !probe.timer.tick(time.delta()).just_finished() {
        return;
    }

    let (row, col) = PROBE_CELL;
    let cell = |board: &BoardRes| {
        board
            .0
            .cell_snapshot(row, col)
            .map(|cell| (cell.aphids, cell.ladybugs))
    };
    let tools = || {
        tool_buttons
            .iter()
            .map(|(_, tool, variant)| format!("{:?}={variant:?}", tool.0))
            .collect::<Vec<_>>()
            .join(" ")
    };

    match probe.stage {
        // Every change below goes through the path real input takes: widget
        // events into their observers, and `CellEdit` messages into
        // `apply_cell_edits`. Nothing is written into resources directly.
        0 => {
            if let Ok(mut window) = windows.single_mut() {
                let (width, height) = review_window_size();
                window.resolution.set(width as f32, height as f32);
            }
            if let Some((entity, _, _)) = fields.iter().find(|(_, field, _)| field.0 == 0) {
                info!(
                    "before ValueChange: params[0]={:.3} board aphid prob_move={:.3}",
                    params.get(0),
                    board.0.aphid_params().prob_move,
                );
                commands.trigger(ValueChange {
                    source: entity,
                    value: 88.0f64,
                    is_final: true,
                });
            }
            if let Ok((toggle, checked)) = food_toggles.single() {
                info!(
                    "before toggle: overlay={} checkbox checked={checked}",
                    overlay.on
                );
                commands.trigger(ValueChange {
                    source: toggle,
                    value: true,
                    is_final: true,
                });
            }

            info!(
                "before edit: cell {:?}={:?} turn={} aphids={} ladybugs={} history={} playing={} editing={} tool={:?} [{}]",
                PROBE_CELL,
                cell(&board),
                stats.turn,
                stats.aphids,
                stats.ladybugs,
                history.0.len(),
                playing.0,
                edit.enabled,
                edit.tool,
                tools(),
            );
            if let Some((entity, _, _)) = tool_buttons
                .iter()
                .find(|(_, tool, _)| tool.0 == EditTool::Ladybug)
            {
                commands.trigger(Activate { entity });
            }
            // Net effect on the cell: three aphids and two ladybugs, so both
            // kinds end up crowded enough to earn a count badge.
            for (tool, action) in [
                (EditTool::Aphid, EditAction::Add),
                (EditTool::Aphid, EditAction::Add),
                (EditTool::Aphid, EditAction::Add),
                (EditTool::Aphid, EditAction::Add),
                (EditTool::Aphid, EditAction::Remove),
                (EditTool::Ladybug, EditAction::Add),
                (EditTool::Ladybug, EditAction::Add),
                (EditTool::Ladybug, EditAction::Add),
                (EditTool::Ladybug, EditAction::Remove),
            ] {
                edits.write(CellEdit {
                    row,
                    col,
                    tool,
                    action,
                });
            }
            commands.insert_resource(ProbeCell(PROBE_CELL));
            // Pin the chart crosshair too, so the capture shows its tooltip.
            commands.insert_resource(ProbeHover(0));
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        // Capture once the observers, edits and sync systems have all run.
        1 => {
            info!(
                "after ValueChange: params[0]={:.3} board aphid prob_move={:.3}",
                params.get(0),
                board.0.aphid_params().prob_move,
            );
            if let Ok((_, checked)) = food_toggles.single() {
                info!(
                    "after toggle: overlay={} checkbox checked={checked}",
                    overlay.on
                );
            }
            info!(
                "after edit: cell {:?}={:?} turn={} aphids={} ladybugs={} history={} playing={} editing={} tool={:?} [{}]",
                PROBE_CELL,
                cell(&board),
                stats.turn,
                stats.aphids,
                stats.ladybugs,
                history.0.len(),
                playing.0,
                edit.enabled,
                edit.tool,
                tools(),
            );
            // Zoom, pan and cell hover each need exactly one board camera.
            info!("board cameras matched: {}", board_cameras.iter().count());
            if let Ok((camera, camera_transform, Projection::Orthographic(ortho), _)) =
                board_cameras.single()
                && let Some(viewport) = camera.logical_viewport_size()
            {
                let pixels_per_unit = viewport.y / ortho.area.height();
                info!(
                    "cell on screen: {:.1}px  badges legible: {}  badge raster: {}px",
                    (CELL - GAP) * pixels_per_unit,
                    badges_legible(viewport.y, ortho.area.height()),
                    badge_text_raster(pixels_per_unit).0,
                );
                // Where the aphid badge lands, so a capture can be cropped to it
                // whatever size the window manager hands us.
                let badge = creature_world(Coordinates { x: row, y: col }, BADGE_SLOTS[0]);
                if let Ok(screen) = camera.world_to_viewport(camera_transform, badge.extend(0.0)) {
                    // `world_to_viewport` already returns window coordinates,
                    // so the panel offset must not be added again.
                    info!(
                        "aphid badge at physical ({:.0}, {:.0})",
                        screen.x * 2.0,
                        screen.y * 2.0,
                    );
                }
            }
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.clone()));
            probe.timer = Timer::from_seconds(1.5, TimerMode::Once);
        }
        2 => {
            if let Some((entity, _)) = tabs.iter().find(|(_, tab)| tab.0 == PanelTab::Edit) {
                focus.set(entity, FocusCause::Navigated);
                commands.trigger(FocusedInput::new(
                    entity,
                    KeyboardInput {
                        key_code: KeyCode::ArrowLeft,
                        logical_key: Key::ArrowLeft,
                        state: ButtonState::Pressed,
                        text: None,
                        repeat: false,
                        window: Entity::PLACEHOLDER,
                    },
                    Entity::PLACEHOLDER,
                ));
            }
            if let Some((_, _, children)) = fields.iter().find(|(_, field, _)| field.0 == 0) {
                for child in children.iter() {
                    if let Ok(mut text) = texts.get_mut(child) {
                        text.editor.set_text("12.3456789");
                        commands.trigger(FocusedInput::new(
                            child,
                            KeyboardInput {
                                key_code: KeyCode::Enter,
                                logical_key: Key::Enter,
                                state: ButtonState::Pressed,
                                text: None,
                                repeat: false,
                                window: Entity::PLACEHOLDER,
                            },
                            Entity::PLACEHOLDER,
                        ));
                    }
                }
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        3 => {
            assert!((params.get(0) - 0.123456789).abs() < 1e-12);
            assert!((board.0.aphid_params().prob_move - 0.123456789).abs() < 1e-12);
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.replace(".png", "-parameters.png")));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        4 => {
            if let Some((entity, _)) = steps.iter().find(|(_, step)| step.0 == 0 && step.1 > 0.0) {
                commands.trigger(Activate { entity });
            }
            if let Ok((mut position, computed)) = panel.single_mut() {
                let content = computed.content_size() * computed.inverse_scale_factor();
                let visible = computed.size() * computed.inverse_scale_factor();
                position.y = scrolled_offset(
                    position.y,
                    -1000.0,
                    MouseScrollUnit::Line,
                    content.y - visible.y,
                );
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        5 => {
            assert!((params.get(0) - 0.124).abs() < 1e-12);
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.replace(".png", "-scrolled.png")));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        6 => {
            if let Some((entity, _)) = tabs.iter().find(|(_, tab)| tab.0 == PanelTab::Edit) {
                focus.set(entity, FocusCause::Navigated);
                commands.trigger(FocusedInput::new(
                    entity,
                    KeyboardInput {
                        key_code: KeyCode::Enter,
                        logical_key: Key::Enter,
                        state: ButtonState::Pressed,
                        text: None,
                        repeat: false,
                        window: Entity::PLACEHOLDER,
                    },
                    Entity::PLACEHOLDER,
                ));
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        7 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.replace(".png", "-edit.png")));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        // Exercise the shader at actual camera zoom levels, rather than
        // scaling a screenshot, to catch border-width and antialiasing faults.
        8 | 10 => {
            if let Ok((_, _, mut projection, mut transform)) = board_cameras.single_mut()
                && let Projection::Orthographic(ortho) = &mut *projection
            {
                ortho.scale = if probe.stage == 8 { 0.35 } else { 2.0 };
                // Keep the insects in the close-up, rather than zooming into
                // empty cells at the centre of the board.
                let centre = if probe.stage == 8 {
                    cell_to_world(row, col)
                } else {
                    Vec2::new(
                        (board.0.cols() as f32 - 1.0) * CELL * 0.5,
                        -(board.0.rows() as f32 - 1.0) * CELL * 0.5,
                    )
                };
                transform.translation.x = centre.x;
                transform.translation.y = centre.y;
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        9 | 11 => {
            let suffix = if probe.stage == 9 {
                "-zoomed.png"
            } else {
                "-distant.png"
            };
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.replace(".png", suffix)));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        12 => {
            if let Some((entity, _)) = tabs.iter().find(|(_, tab)| tab.0 == PanelTab::Overview) {
                focus.set(entity, FocusCause::Navigated);
                commands.trigger(FocusedInput::new(
                    entity,
                    KeyboardInput {
                        key_code: KeyCode::Home,
                        logical_key: Key::Home,
                        state: ButtonState::Pressed,
                        text: None,
                        repeat: false,
                        window: Entity::PLACEHOLDER,
                    },
                    Entity::PLACEHOLDER,
                ));
            }
            for (entity, section) in &sections {
                if matches!(section.0, PanelSection::Setup) {
                    commands.trigger(ToggleChecked { entity });
                }
            }
            if let Ok((_, _, mut projection, _)) = board_cameras.single_mut()
                && let Projection::Orthographic(ortho) = &mut *projection
            {
                ortho.scale = 1.0;
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        13 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.replace(".png", "-setup.png")));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        14 => {
            if let Ok((mut position, computed)) = panel.single_mut() {
                position.y = ((computed.content_size().y - computed.size().y)
                    * computed.inverse_scale_factor())
                .max(0.0);
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        15 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(
                    probe.path.replace(".png", "-setup-scrolled.png"),
                ));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        16 => {
            for (entity, view) in &views {
                if view.0 == BoardView::Population {
                    commands.trigger(Activate { entity });
                }
            }
            for (entity, section) in &sections {
                if matches!(section.0, PanelSection::Setup) {
                    commands.trigger(ToggleChecked { entity });
                }
            }
            if let Ok((mut position, _)) = panel.single_mut() {
                position.y = 0.0;
            }
            if let Ok((_, _, mut projection, _)) = board_cameras.single_mut()
                && let Projection::Orthographic(ortho) = &mut *projection
            {
                ortho.scale = 2.0;
            }
            actions.write(RunAction::FinishEditing);
            commands.remove_resource::<ProbeCell>();
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        17 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(
                    probe.path.replace(".png", "-population-distant.png"),
                ));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        18 | 20 => {
            for (entity, control) in &chart_controls {
                if control.0 == ChartAction::Toggle {
                    commands.trigger(Activate { entity });
                }
            }
            if probe.stage == 20 {
                for (entity, control) in &chart_controls {
                    if control.0 == ChartAction::Larger {
                        commands.trigger(Activate { entity });
                    }
                }
                actions.write(RunAction::Fit);
                for _ in 0..12 {
                    actions.write(RunAction::Step);
                }
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        19 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.replace(".png", "-collapsed.png")));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        21 => {
            if let Some((entity, _, _)) = fields.iter().find(|(_, field, _)| field.0 == 0) {
                commands.trigger(ValueChange {
                    source: entity,
                    value: 25.0f64,
                    is_final: true,
                });
            }
            commands.insert_resource(ProbeHover(stats.turn));
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        22 => {
            for _ in 0..45 {
                actions.write(RunAction::Step);
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        23 => {
            info!(
                "chart events: {}",
                history
                    .1
                    .iter()
                    .map(|event| format!("turn {}: {}", event.turn, event.caption()))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.replace(".png", "-events.png")));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        24 | 26 => {
            for entity in &properties_toggles {
                focus.set(entity, FocusCause::Navigated);
                commands.trigger(FocusedInput::new(
                    entity,
                    KeyboardInput {
                        key_code: KeyCode::Enter,
                        logical_key: Key::Enter,
                        state: ButtonState::Pressed,
                        text: None,
                        repeat: false,
                        window: Entity::PLACEHOLDER,
                    },
                    Entity::PLACEHOLDER,
                ));
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        25 | 27 => {
            let suffix = if probe.stage == 25 {
                "-properties-hidden.png"
            } else {
                "-properties-restored.png"
            };
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.replace(".png", suffix)));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        28 | 29 => {
            // Enough rows to exercise scrolling in both window sizes.
            actions.write(RunAction::Step);
            if let Some((source, _, _)) = fields.iter().find(|(_, field, _)| field.0 == 0) {
                commands.trigger(ValueChange {
                    source,
                    value: if probe.stage == 28 { 30.0f64 } else { 35.0f64 },
                    is_final: true,
                });
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        30 => {
            commands.remove_resource::<ProbeHover>();
            if let (Ok(list), Some((row, _))) = (
                event_lists.single(),
                event_rows.iter().find(|(_, row)| row.0.turn == 12),
            ) {
                focus.set(list, FocusCause::Navigated);
                commands.trigger(SetSelected { entity: list, row });
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        31 => {
            assert_eq!(selection.0, Some(12));
            assert_eq!(
                chart_hover.0,
                history.0.iter().position(|point| point.turn == 12)
            );
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(
                    probe.path.replace(".png", "-event-selected.png"),
                ));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        32 => {
            if let Ok((body, _)) = event_bodies.single() {
                commands.trigger(probe_list_scroll(body));
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        33 => {
            assert!(
                event_bodies.single().unwrap().1.y > 0.0,
                "the event list must scroll"
            );
            assert_eq!(
                panel.single().unwrap().0.y,
                0.0,
                "the outer panel must not also scroll"
            );
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(
                    probe.path.replace(".png", "-event-scrolled.png"),
                ));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        34 => {
            if let Ok(entity) = clear_pins.single() {
                commands.trigger(Activate { entity });
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        35 => {
            assert_eq!(selection.0, None);
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(
                    probe.path.replace(".png", "-event-cleared.png"),
                ));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        36 => {
            commands.trigger(Activate {
                entity: speed_buttons.single().unwrap(),
            });
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        37 => {
            assert_eq!(*speed_popups.single().unwrap(), Visibility::Visible);
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.replace(".png", "-speed-menu.png")));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        38 | 39 => {
            // Navigate and commit through the same focused keyboard events as a user.
            let entity = focus.get().unwrap();
            let (key_code, logical_key) = if probe.stage == 38 {
                (KeyCode::End, Key::End)
            } else {
                assert_eq!(speed_items.get(entity).unwrap().1.0, 0.05);
                (KeyCode::Enter, Key::Enter)
            };
            commands.trigger(FocusedInput::new(
                entity,
                KeyboardInput {
                    key_code,
                    logical_key,
                    state: ButtonState::Pressed,
                    text: None,
                    repeat: false,
                    window: Entity::PLACEHOLDER,
                },
                Entity::PLACEHOLDER,
            ));
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        40 => {
            assert_eq!(step_timer.0.duration().as_secs_f32(), 0.05);
            assert!(!playing.0, "selecting a speed must keep the run paused");
            assert_eq!(*speed_popups.single().unwrap(), Visibility::Hidden);
            assert_eq!(focus.get(), Some(speed_buttons.single().unwrap()));
            actions.write(RunAction::Slower);
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        41 => {
            assert_eq!(step_timer.0.duration().as_secs_f32(), 0.1);
            commands.trigger(Activate {
                entity: speed_buttons.single().unwrap(),
            });
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        42 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(
                    probe.path.replace(".png", "-speed-selected.png"),
                ));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        _ => {
            exit.write(AppExit::Success);
            return;
        }
    }

    probe.stage += 1;
}

/// Set by the screenshot probe to pin the hovered board cell, since it has no
/// pointer.
#[derive(Resource)]
pub struct ProbeCell(pub (usize, usize));

/// Set by the screenshot probe to pin the crosshair, since it has no pointer.
#[derive(Resource)]
pub struct ProbeHover(pub usize);
