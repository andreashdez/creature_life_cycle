//! Keyboard shortcuts, and the pointer on the board: pan, zoom, and telling a
//! click from a drag.

use crate::board::{BoardView, FoodOverlay, set_food_details};
use crate::chart::{ChartPanel, ChartSize};
use crate::editing::{EditMode, EditTool};
use crate::layout::{BoardCamera, board_rect};
use crate::panel::{ActiveTab, PanelTab, PropertiesPanel};
use crate::run_control::RunAction;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;

/// How far the pointer may move between press and release and still count as a
/// click (an edit) rather than a drag (a pan), in logical pixels.
pub const CLICK_SLOP: f32 = 4.0;

/// Where a board press started and how far the pointer has wandered since, so a
/// click can be told apart from a drag.
#[derive(Resource, Default)]
pub struct BoardPointer {
    pub pressed_on_board: bool,
    pub press_at: Vec2,
    pub travelled: f32,
}

impl BoardPointer {
    pub fn is_click(&self) -> bool {
        self.pressed_on_board && self.travelled <= CLICK_SLOP
    }
}

pub fn keyboard_controls(
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: ResMut<InputFocus>,
    mut actions: MessageWriter<RunAction>,
    (mut overlay, mut view): (ResMut<FoodOverlay>, ResMut<BoardView>),
    mut edit: ResMut<EditMode>,
    (mut active, mut properties): (ResMut<ActiveTab>, ResMut<PropertiesPanel>),
    text_fields: Query<(), With<EditableText>>,
) {
    // Focused Feathers widgets own their keyboard input, including Space. Do
    // not toggle playback a second time when Space activates a focused button.
    if keys.just_pressed(KeyCode::Escape) {
        focus.clear();
    }
    // A focused text field owns every key, not just Space: typing a seed must
    // not also step the run or fit the board. `seed_field_keyboard` is what
    // reads the keyboard there.
    if focus
        .get()
        .is_some_and(|entity| text_fields.contains(entity))
    {
        return;
    }
    for (key, action) in [
        (KeyCode::Space, RunAction::TogglePlay),
        (KeyCode::KeyN, RunAction::Step),
        (KeyCode::KeyR, RunAction::Reset),
        (KeyCode::BracketRight, RunAction::Faster),
        (KeyCode::BracketLeft, RunAction::Slower),
        (KeyCode::Home, RunAction::Fit),
        (KeyCode::KeyS, RunAction::Save),
        (KeyCode::KeyZ, RunAction::Undo),
    ] {
        if keys.just_pressed(key) && !(key == KeyCode::Space && focus.get().is_some()) {
            actions.write(action);
        }
    }
    if keys.just_pressed(KeyCode::KeyE) {
        edit.enabled = !edit.enabled;
        active.0 = PanelTab::Edit;
        properties.open = true;
        focus.clear();
    }
    if keys.just_pressed(KeyCode::KeyA) {
        edit.select(EditTool::Aphid);
        active.0 = PanelTab::Edit;
        properties.open = true;
        focus.clear();
    }
    if keys.just_pressed(KeyCode::KeyL) {
        edit.select(EditTool::Ladybug);
        active.0 = PanelTab::Edit;
        properties.open = true;
        focus.clear();
    }
    if keys.just_pressed(KeyCode::KeyX) {
        edit.select(EditTool::Erase);
        active.0 = PanelTab::Edit;
        properties.open = true;
        focus.clear();
    }
    if keys.just_pressed(KeyCode::KeyF) {
        set_food_details(!overlay.on, &mut overlay, &mut view);
    }
}

pub fn camera_controls(
    windows: Query<&Window>,
    mut camera: Query<(&mut Transform, &mut Projection), With<BoardCamera>>,
    (scroll, motion, mouse): (
        Res<AccumulatedMouseScroll>,
        Res<AccumulatedMouseMotion>,
        Res<ButtonInput<MouseButton>>,
    ),
    mut pointer: ResMut<BoardPointer>,
    mut focus: ResMut<InputFocus>,
    (chart_size, chart_panel, properties): (Res<ChartSize>, Res<ChartPanel>, Res<PropertiesPanel>),
) {
    let chart_height = chart_size.1;
    let (Ok(window), Ok((mut transform, mut projection))) = (windows.single(), camera.single_mut())
    else {
        return;
    };

    // Scrolling the panel or the chart must not zoom the board, and dragging a
    // slider must not pan it, so both only act when they start over the board.
    let cursor = window.cursor_position();
    let over_board = !chart_panel.dragging
        && cursor.is_some_and(|cursor| {
            board_rect(window, chart_height, &properties).contains(cursor)
                && cursor.y < window.height() - chart_height - 5.0
        });
    let buttons = [MouseButton::Left, MouseButton::Middle];
    if mouse.any_just_pressed(buttons) {
        if over_board {
            focus.clear();
        }
        *pointer = BoardPointer {
            pressed_on_board: over_board,
            press_at: cursor.unwrap_or_default(),
            travelled: 0.0,
        };
    }
    let held = mouse.any_pressed(buttons);
    if held && let Some(cursor) = cursor {
        pointer.travelled = pointer.travelled.max(cursor.distance(pointer.press_at));
    }

    if let Projection::Orthographic(ref mut ortho) = *projection {
        if over_board && scroll.delta.y != 0.0 {
            ortho.scale = (ortho.scale * (1.0 - scroll.delta.y * 0.1)).clamp(0.05, 8.0);
        }
        // Pan only once the pointer has left the click slop, so a click meant as
        // an edit never nudges the board.
        if held && pointer.pressed_on_board && !pointer.is_click() {
            transform.translation.x -= motion.delta.x * ortho.scale;
            transform.translation.y += motion.delta.y * ortho.scale;
        }
    }
}
