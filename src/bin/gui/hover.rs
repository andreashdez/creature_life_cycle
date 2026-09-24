//! The hovered cell: finding it under the pointer, outlining it, and the
//! inspector popup that follows the pointer.

use crate::board::{CELL, CELL_CORNER, GAP, MAX_FOOD, cell_to_world};
use crate::chart::{ChartSize, GRIDLINE, INK_PRIMARY, INK_SECONDARY, TOOLTIP_BG};
use crate::editing::{BoardEditGizmos, EDIT_APHID, EDIT_LADYBUG, EditMode, EditTool};
use crate::layout::{BoardCamera, board_rect};
use crate::panel::PropertiesPanel;
use crate::probe::ProbeCell;
use crate::simulation::BoardRes;
use crate::widgets::set_text;
use bevy::prelude::*;

/// Offset of the cell popup from the pointer, and the margin it keeps from the
/// window edges, both as `draw_hover_tooltip` in the previous macroquad GUI.
const POPUP_OFFSET: f32 = 18.0;
const POPUP_MARGIN: f32 = 12.0;

const HOVER: Color = Color::srgb(0.85, 0.87, 0.70);

#[derive(Resource, Default)]
pub struct Hovered(pub Option<(usize, usize)>);

/// Root of the cell inspector popup, which floats beside the pointer.
#[derive(Component)]
pub struct CellPopup;

/// The popup's two lines, so one system can fill both.
#[derive(Component, Clone, Copy)]
pub enum PopupLine {
    Heading,
    Counts,
}

/// Cell inspector popup. It lives outside the panel and follows the pointer, as
/// `draw_hover_tooltip` in the previous macroquad GUI does, so hovering a cell never
/// moves the controls. `update_cell_popup` places it and fills it in.
pub fn spawn_cell_popup(commands: &mut Commands, ui_camera: Entity, font: Handle<Font>) {
    let line = |size: f32, colour: Color| {
        (
            Text::new(""),
            TextFont {
                font: font.clone().into(),
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(colour),
        )
    };

    commands
        .spawn((
            CellPopup,
            UiTargetCamera(ui_camera),
            // Above the toolbar and the chart overlay, whatever the spawn order.
            GlobalZIndex(10),
            Visibility::Hidden,
            // The popup sits under the pointer; it must not eat its own hover.
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                padding: UiRect::axes(px(12), px(10)),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(TOOLTIP_BG),
            BorderColor::all(GRIDLINE),
        ))
        .with_children(|popup| {
            popup.spawn((PopupLine::Heading, line(14.0, INK_PRIMARY)));
            popup.spawn((PopupLine::Counts, line(13.0, INK_SECONDARY)));
        });
}

/// Replaces the inverse-layout arithmetic the previous macroquad GUI does to find the
/// hovered cell: project the cursor back through the camera instead.
pub fn hovered_cell(
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform), With<BoardCamera>>,
    board: Res<BoardRes>,
    probe: Option<Res<ProbeCell>>,
    mut hovered: ResMut<Hovered>,
    (chart_size, properties): (Res<ChartSize>, Res<PropertiesPanel>),
) {
    let chart_height = chart_size.1;
    if let Some(probe) = probe {
        hovered.0 = Some(probe.0);
        return;
    }
    let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), camera.single()) else {
        return;
    };

    hovered.0 = window
        .cursor_position()
        .filter(|&cursor| board_rect(window, chart_height, &properties).contains(cursor))
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
        .and_then(|world| {
            let col = (world.x / CELL + 0.5).floor();
            let row = (-world.y / CELL + 0.5).floor();
            (col >= 0.0 && row >= 0.0).then_some((row as usize, col as usize))
        })
        .filter(|&(row, col)| row < board.0.rows() && col < board.0.cols());
}

pub fn draw_hover(
    hovered: Res<Hovered>,
    edit: Res<EditMode>,
    mut gizmos: Gizmos,
    mut edit_gizmos: Gizmos<BoardEditGizmos>,
) {
    let Some((row, col)) = hovered.0 else {
        return;
    };
    let centre = cell_to_world(row, col);
    gizmos
        .rounded_rect_2d(centre, Vec2::splat(CELL - GAP), HOVER)
        .corner_radius((CELL - GAP) * CELL_CORNER);

    // While editing, preview what a click would place: an outline just outside
    // the cell and a creature-sized ring, in the selected tool's colour.
    if edit.enabled {
        let colour = match edit.tool {
            EditTool::Aphid => EDIT_APHID,
            EditTool::Ladybug => EDIT_LADYBUG,
            EditTool::Erase => HOVER,
        };
        edit_gizmos
            .rounded_rect_2d(centre, Vec2::splat(CELL - GAP + 6.0), colour)
            .corner_radius((CELL - GAP) * CELL_CORNER + 3.0);
        if edit.tool == EditTool::Erase {
            let d = Vec2::splat((CELL - GAP) * 0.22);
            edit_gizmos.line_2d(centre - d, centre + d, colour);
            edit_gizmos.line_2d(
                centre + Vec2::new(-d.x, d.y),
                centre + Vec2::new(d.x, -d.y),
                colour,
            );
        } else {
            edit_gizmos.circle_2d(centre, (CELL - GAP) * 0.22, colour);
        }
    }
}

/// Fills the cell inspector popup and parks it beside the pointer, hiding it
/// whenever no cell is hovered. This is the Bevy counterpart to
/// `draw_hover_tooltip` in the previous macroquad GUI, down to the 18px offset and the
/// clamp that keeps the popup on screen near the window edges.
pub fn update_cell_popup(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<BoardCamera>>,
    hovered: Res<Hovered>,
    board: Res<BoardRes>,
    mut popup: Query<(&mut Node, &ComputedNode, &mut Visibility), With<CellPopup>>,
    mut lines: Query<(&PopupLine, &mut Text)>,
    (chart_size, properties): (Res<ChartSize>, Res<PropertiesPanel>),
) {
    let chart_height = chart_size.1;
    let (Ok(window), Ok((mut node, computed, mut visibility))) =
        (windows.single(), popup.single_mut())
    else {
        return;
    };

    let cell = hovered
        .0
        .and_then(|(row, col)| board.0.cell_snapshot(row, col).map(|cell| (row, col, cell)));
    let Some((row, col, snapshot)) = cell else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let Some(anchor) = popup_anchor(
        window,
        cameras.single().ok(),
        row,
        col,
        chart_height,
        &properties,
    ) else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };

    // Hidden rather than `Display::None`, so the popup is still laid out while
    // it is away and its measured size can place it the frame it comes back.
    let size = computed.size() * computed.inverse_scale_factor();
    let left = px(clamp_to_window(anchor.x, size.x, window.width()));
    let top = px(clamp_to_window(anchor.y, size.y, window.height()));
    if node.left != left || node.top != top {
        node.left = left;
        node.top = top;
    }
    visibility.set_if_neq(Visibility::Inherited);

    for (line, text) in &mut lines {
        let next = match line {
            PopupLine::Heading => format!("Row {row} · Column {col}"),
            PopupLine::Counts => format!(
                "Aphids {}    Ladybugs {}    Food {} / {MAX_FOOD}",
                snapshot.aphids, snapshot.ladybugs, snapshot.food
            ),
        };
        set_text(Some(text), next);
    }
}

/// What the popup points at: the pointer while it is over the board, and
/// otherwise the hovered cell itself, which is what the screenshot probe pins
/// since it has no pointer.
fn popup_anchor(
    window: &Window,
    camera: Option<(&Camera, &GlobalTransform)>,
    row: usize,
    col: usize,
    chart_height: f32,
    properties: &PropertiesPanel,
) -> Option<Vec2> {
    window
        .cursor_position()
        .filter(|&cursor| board_rect(window, chart_height, properties).contains(cursor))
        .or_else(|| {
            let (camera, transform) = camera?;
            camera
                .world_to_viewport(transform, cell_to_world(row, col).extend(0.0))
                .ok()
        })
}

/// One axis of the popup's placement: `POPUP_OFFSET` past the anchor, pulled
/// back so the whole popup stays inside the window.
pub fn clamp_to_window(anchor: f32, size: f32, window: f32) -> f32 {
    (anchor + POPUP_OFFSET)
        .min(window - size - POPUP_MARGIN)
        .max(POPUP_MARGIN)
}
