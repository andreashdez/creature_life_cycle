//! The graphical simulation: the board, a `bevy_feathers` settings sidebar, a
//! population history chart, the food overlay and cell editing.
//!
//! Run with: `cargo run --bin gui`
//!
//! The simulation itself is untouched: `Board` lives in a resource and every
//! turn still comes from `Board::refresh`. Much of this is a port of the
//! earlier macroquad GUI, removed in favour of this one;
//! `notes/bevy-board-rendering.md` records the design and the trade-offs, and
//! comments referring to it explain where a constant or layout came from.

mod board;
mod chart;
mod creatures;
mod editing;
mod history;
mod hover;
mod input;
mod layout;
mod panel;
mod params;
mod probe;
mod run_control;
mod simulation;
mod startup;
mod summary;
#[cfg(test)]
mod tests;
mod widgets;

use crate::board::{
    BoardDetail, BoardView, CellMaterial, FoodOverlay, board_render_assets, rebuild_food_overlay,
    recolor_cells, update_board_detail,
};
use crate::chart::{
    ChartGridGizmos, ChartHover, ChartPanel, ChartSeriesGizmos, ChartSize, chart_hover,
    chart_resize_input, draw_chart_marks, sync_chart_panel, update_chart_labels,
};
use crate::creatures::{
    BadgeIndex, BadgeRaster, CreatureIndex, TrailGizmos, TrailGlowGizmos, advance_tweens,
    scale_count_badges, scale_trail_gizmos, sync_count_badges, sync_creatures,
};
use crate::editing::{
    BoardEditGizmos, CellEdit, EditHistory, EditMode, apply_cell_edits, board_edit_input,
    exclusive_play_and_edit, sync_edit_controls, update_edit_palette, update_edit_preview,
};
use crate::hover::{Hovered, draw_hover, hovered_cell, update_cell_popup};
use crate::input::{BoardPointer, camera_controls, keyboard_controls};
use crate::layout::layout_viewports;
use crate::panel::{
    ActiveTab, PanelSections, PropertiesPanel, scroll_panel, sync_board_view, sync_food_checkbox,
    sync_panel_sections, sync_properties_panel, sync_scrollbar_visibility, sync_sidebar,
};
use crate::params::{
    ParameterReset, apply_params, commit_parameter_fields, sync_parameter_widgets,
};
use crate::probe::{ScreenshotProbe, review_window_size, screenshot_probe};
use crate::run_control::{
    Reseed, RunAction, StatusLine, apply_run_actions, apply_seed, expire_status, init_seed_field,
    save_setup, seed_field_keyboard,
};
use crate::simulation::{
    DEFAULT_SEED, Playing, Revision, STEP_SECONDS, Seed, Stats, StepTimer, step_simulation,
};
use crate::startup::setup;
use crate::summary::{update_hud, update_status_text};
use bevy::camera::visibility::RenderLayers;
use bevy::feathers::FeathersPlugins;
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::theme::UiTheme;
use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Aphids and Ladybugs — Bevy board prototype".into(),
                // The cameras fit to the actual viewport, including HiDPI scaling.
                resolution: review_window_size().into(),
                // Keep tiling window managers from enlarging review captures.
                resizable: std::env::var_os("CLC_SCREENSHOT").is_none(),
                resize_constraints: WindowResizeConstraints {
                    min_width: 900.0,
                    min_height: 640.0,
                    ..default()
                },
                ..default()
            }),
            ..default()
        }))
        .add_plugins(board_render_assets)
        .add_plugins(Material2dPlugin::<CellMaterial>::default())
        .add_plugins(FeathersPlugins)
        .insert_resource(UiTheme(create_dark_theme()))
        // Gridlines and series need different widths, and gizmo width is set per
        // group, so they are two groups. Both live on the chart's render layer.
        .insert_gizmo_config(
            ChartGridGizmos,
            GizmoConfig {
                render_layers: RenderLayers::layer(2),
                line: GizmoLineConfig {
                    width: 1.0,
                    ..default()
                },
                ..default()
            },
        )
        .insert_gizmo_config(
            ChartSeriesGizmos,
            GizmoConfig {
                render_layers: RenderLayers::layer(2),
                line: GizmoLineConfig {
                    width: 2.0,
                    joints: GizmoLineJoint::Round(8),
                    ..default()
                },
                ..default()
            },
        )
        .init_resource::<ChartSize>()
        .init_resource::<ChartHover>()
        .init_resource::<ChartPanel>()
        .insert_resource(ClearColor(Color::srgb(0.086, 0.106, 0.102)))
        .init_resource::<Hovered>()
        .init_resource::<FoodOverlay>()
        .init_resource::<BoardView>()
        .init_resource::<BoardDetail>()
        .init_resource::<EditMode>()
        .init_resource::<EditHistory>()
        .init_resource::<PanelSections>()
        .init_resource::<BoardPointer>()
        .add_message::<CellEdit>()
        .add_message::<RunAction>()
        .add_message::<Reseed>()
        .add_message::<ParameterReset>()
        .init_resource::<ActiveTab>()
        .init_resource::<PropertiesPanel>()
        .init_resource::<StatusLine>()
        .insert_resource(Seed(DEFAULT_SEED))
        .insert_gizmo_config(
            BoardEditGizmos,
            GizmoConfig {
                line: GizmoLineConfig {
                    width: 2.0,
                    ..default()
                },
                ..default()
            },
        )
        // The trail groups keep the gizmo defaults: their widths follow the
        // camera's zoom and are set every frame by `scale_trail_gizmos`.
        .init_gizmo_group::<TrailGizmos>()
        .init_gizmo_group::<TrailGlowGizmos>()
        .init_resource::<CreatureIndex>()
        .init_resource::<BadgeIndex>()
        .init_resource::<BadgeRaster>()
        .insert_resource(Playing(true))
        .insert_resource(Stats::default())
        .insert_resource(StepTimer(Timer::from_seconds(
            STEP_SECONDS,
            TimerMode::Repeating,
        )))
        .insert_resource(Revision(0))
        .add_systems(Startup, setup)
        .add_systems(Update, screenshot_probe)
        .add_systems(
            Update,
            (
                // Input and simulation.
                (
                    keyboard_controls.run_if(not(resource_exists::<ScreenshotProbe>)),
                    seed_field_keyboard.run_if(not(resource_exists::<ScreenshotProbe>)),
                    // Before the playback handler, so a seed applied by key or
                    // button restarts the run in the same frame it is asked for.
                    apply_seed,
                    apply_run_actions,
                    (sync_sidebar, sync_properties_panel).chain(),
                    chart_resize_input.run_if(not(resource_exists::<ScreenshotProbe>)),
                    layout_viewports,
                    scroll_panel.run_if(not(resource_exists::<ScreenshotProbe>)),
                    camera_controls.run_if(not(resource_exists::<ScreenshotProbe>)),
                    hovered_cell,
                    board_edit_input.run_if(not(resource_exists::<ScreenshotProbe>)),
                    apply_cell_edits,
                    exclusive_play_and_edit,
                    sync_edit_controls,
                    chart_hover,
                    commit_parameter_fields,
                    apply_params,
                    // After the edits and parameters have landed, so a save
                    // writes the board as it is on screen, and before the next
                    // turn, so it is that board rather than the one after it.
                    save_setup,
                    expire_status,
                    step_simulation,
                )
                    .chain(),
                // Rendering, from the state settled above. Split in two because a
                // system tuple holds at most 20 entries.
                (
                    recolor_cells,
                    rebuild_food_overlay,
                    sync_food_checkbox,
                    init_seed_field,
                    sync_creatures,
                    sync_count_badges,
                    scale_count_badges,
                    advance_tweens,
                    draw_hover,
                    draw_chart_marks,
                    update_chart_labels,
                    update_hud,
                    update_status_text,
                    update_cell_popup,
                    sync_scrollbar_visibility,
                    sync_panel_sections,
                    update_edit_palette,
                    update_edit_preview,
                )
                    .chain(),
                (
                    sync_parameter_widgets,
                    sync_board_view,
                    update_board_detail,
                    scale_trail_gizmos,
                    sync_chart_panel,
                )
                    .chain(),
            )
                .chain(),
        )
        .run();
}
