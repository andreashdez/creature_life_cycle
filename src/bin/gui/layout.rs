//! Where the board, the chart, and the sidebars sit in the window, and the
//! cameras that draw them.

use crate::chart::{ChartGridGizmos, ChartPanel, ChartSeriesGizmos, ChartSize};
use crate::editing::BoardEditGizmos;
use crate::panel::PropertiesPanel;
use bevy::camera::{ScalingMode, Viewport};
use bevy::prelude::*;

/// Sidebar widths in logical pixels.
pub const SUMMARY_WIDTH: f32 = 260.0;
pub const PANEL_WIDTH: f32 = 300.0;
pub const PANEL_HEADER: f32 = 104.0;

// Cameras are told apart by marker, never by `.single()` over `Camera2d`:
// with the board, chart, and UI cameras all being `Camera2d`, such a query
// matches three entities and quietly returns an error.
#[derive(Component)]
pub struct BoardCamera;

#[derive(Component)]
pub struct ChartCamera;

/// Fits the board and chart between the summary and optional properties panel.
/// Runs when the window or either panel changes. Setting camera viewports also keeps
/// hover picking right for free, since `viewport_to_world_2d` is
/// viewport-aware.
pub fn layout_viewports(
    windows: Query<Ref<Window>>,
    panel_state: Res<ChartPanel>,
    properties: Res<PropertiesPanel>,
    mut board_camera: Query<&mut Camera, (With<BoardCamera>, Without<ChartCamera>)>,
    mut chart_camera: Query<(&mut Camera, &mut Projection), With<ChartCamera>>,
    mut gizmo_config: ResMut<GizmoConfigStore>,
    mut chart_size: ResMut<ChartSize>,
) {
    let (Ok(window), Ok(mut board), Ok((mut chart, mut projection))) = (
        windows.single(),
        board_camera.single_mut(),
        chart_camera.single_mut(),
    ) else {
        return;
    };

    if !(window.is_changed() || panel_state.is_changed() || properties.is_changed()) {
        return;
    }
    let height = panel_state.effective_height(window.height());
    chart.is_active = !panel_state.collapsed;
    let scale = window.resolution.scale_factor();
    let physical = window.resolution.physical_size();
    let summary = (SUMMARY_WIDTH * scale).round() as u32;
    let properties = (properties.width() * scale).round() as u32;
    let strip = (height * scale).round() as u32;
    if physical.x <= summary + properties || physical.y <= strip {
        return;
    }
    let width = physical.x - summary - properties;

    board.viewport = Some(Viewport {
        physical_position: UVec2::new(summary, 0),
        physical_size: UVec2::new(width, physical.y - strip),
        ..default()
    });
    chart.viewport = Some(Viewport {
        physical_position: UVec2::new(summary, physical.y - strip),
        physical_size: UVec2::new(width, strip),
        ..default()
    });

    // One world unit per logical pixel, so the chart can be laid out in the
    // same units as its text overlay.
    let logical_width = width as f32 / scale;
    if let Projection::Orthographic(ref mut ortho) = *projection {
        ortho.scaling_mode = ScalingMode::Fixed {
            width: logical_width,
            height,
        };
    }
    chart_size.set_if_neq(ChartSize(logical_width, height));

    // Gizmo line width is in physical pixels (the line shader measures against
    // `view.viewport`), so a 1px hairline and a 2px series line have to be
    // scaled up on HiDPI displays or they render at half weight.
    gizmo_config.config_mut::<ChartGridGizmos>().0.line.width = scale;
    gizmo_config.config_mut::<ChartSeriesGizmos>().0.line.width = 2.0 * scale;
    gizmo_config.config_mut::<BoardEditGizmos>().0.line.width = 2.0 * scale;
}

/// The board's area in logical window coordinates: between the sidebars and
/// above the chart strip.
pub fn board_rect(window: &Window, chart_height: f32, properties: &PropertiesPanel) -> Rect {
    Rect::new(
        SUMMARY_WIDTH,
        0.0,
        window.width() - properties.width(),
        (window.height() - chart_height).max(0.0),
    )
}
