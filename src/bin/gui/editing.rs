//! Hand-editing the board: tool selection, `CellEdit` messages, undo, and the
//! edit tab's controls.

use crate::board::cell_to_world;
use crate::creatures::CreatureAssets;
use crate::history::History;
use crate::hover::Hovered;
use crate::input::BoardPointer;
use crate::simulation::{Playing, Simulation, Stats};
use crate::widgets::ui_text;
use bevy::feathers::controls::{ButtonVariant, FeathersButton};
use bevy::feathers::display::label_small;
use bevy::feathers::theme::ThemedText;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Activate;
use creature_life_cycle::{Board, Random};
use std::collections::VecDeque;

/// Starting life for hand-placed creatures: the same values `src/lib.rs` gives
/// creatures loaded from the config, and that the macroquad editor uses.
pub const EDIT_APHID_LIFE: i32 = 10;
pub const EDIT_LADYBUG_LIFE: i32 = 15;

// Edit preview outline per tool, from `draw_edit_overlay` in the previous macroquad GUI.
pub const EDIT_APHID: Color = Color::srgba_u8(111, 219, 91, 175);
pub const EDIT_LADYBUG: Color = Color::srgba_u8(231, 78, 61, 175);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditTool {
    #[default]
    Aphid,
    Ladybug,
    Erase,
}

/// Board editing state, as `editing_enabled` / `edit_tool` in the previous macroquad GUI.
/// Keys and panel controls write it; the panel controls are synced from it.
#[derive(Resource, Default)]
pub struct EditMode {
    pub enabled: bool,
    pub tool: EditTool,
}

impl EditMode {
    /// Picking a tool also starts editing, as `A` / `L` do in the previous macroquad GUI.
    pub fn select(&mut self, tool: EditTool) {
        self.tool = tool;
        self.enabled = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    Add,
    Remove,
}

/// One requested change to one cell. Pointer input sends these and
/// `apply_cell_edits` applies them, so anything else that sends one (the
/// screenshot probe) goes through exactly the same path.
#[derive(Message, Clone, Copy, Debug)]
pub struct CellEdit {
    pub row: usize,
    pub col: usize,
    pub tool: EditTool,
    pub action: EditAction,
}

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct BoardEditGizmos;

#[derive(Component, Clone, Default, FromTemplate)]
pub struct EditStatus;

#[derive(Component, Clone, Copy, Default, FromTemplate)]
pub struct EditToolButton(pub EditTool);

#[derive(Component, Clone, Default, FromTemplate)]
pub struct UndoButton;

#[derive(Component)]
pub struct EditPreview;

/// Exact snapshots keep undo faithful to life, food, creature IDs and RNG state.
pub struct EditSnapshot {
    pub board: Board,
    pub random: Random,
    pub stats: Stats,
    pub history: History,
}

#[derive(Resource, Default)]
pub struct EditHistory(pub VecDeque<EditSnapshot>);

pub const EDIT_UNDO_LIMIT: usize = 20;

pub fn edit_tool_button(
    caption: &'static str,
    shortcut: &'static str,
    tool: EditTool,
    artwork: Option<Handle<Image>>,
) -> impl Scene {
    let eraser = artwork.is_none();
    bsn! {
        @FeathersButton
        Node {
            flex_grow: 1.0, flex_basis: px(0), min_width: px(0), height: px(98),
            padding: UiRect::all(px(6)), flex_direction: FlexDirection::Column, row_gap: px(4),
        }
        EditToolButton({tool})
        on(move |_: On<Activate>, mut edit: ResMut<EditMode>| { edit.select(tool); })
        Children [
            {artwork.map(|image| bsn! {
                Node { width: px(36), height: px(36), flex_shrink: 0.0 }
                ImageNode { image: {image} }
            })},
            {eraser.then(|| bsn! {
                Node { height: px(36), align_items: AlignItems::Center }
                Children [ui_text("×", 30.0)]
            })},
            (Text(caption) ThemedText),
            label_small(shortcut),
        ]
    }
}

/// Turns clicks on the hovered cell into edits while editing: a left click adds
/// the selected creature, a right click removes one.
pub fn board_edit_input(
    mouse: Res<ButtonInput<MouseButton>>,
    edit: Res<EditMode>,
    hovered: Res<Hovered>,
    pointer: Res<BoardPointer>,
    mut edits: MessageWriter<CellEdit>,
) {
    if !edit.enabled {
        return;
    }
    let Some((row, col)) = hovered.0 else {
        return;
    };

    // Add on release rather than press, so a left drag can still pan.
    let action = if mouse.just_released(MouseButton::Left) && pointer.is_click() {
        EditAction::Add
    } else if mouse.just_pressed(MouseButton::Right) {
        EditAction::Remove
    } else {
        return;
    };
    edits.write(CellEdit {
        row,
        col,
        tool: edit.tool,
        action,
    });
}

pub fn apply_cell_edits(mut edits: MessageReader<CellEdit>, mut sim: Simulation) {
    for edit in edits.read() {
        sim.edit(*edit);
    }
}

/// Editing pauses the run, and resuming the run ends editing, whichever control
/// did it: a key, the panel, or an edit.
pub fn exclusive_play_and_edit(mut playing: ResMut<Playing>, mut edit: ResMut<EditMode>) {
    if edit.is_changed() && edit.enabled && playing.0 {
        playing.0 = false;
    } else if playing.is_changed() && playing.0 && edit.enabled {
        edit.enabled = false;
    }
}

/// Tool selection is shared by pointer controls, shortcuts and playback.
pub fn sync_edit_controls(
    edit: Res<EditMode>,
    mut buttons: Query<(&EditToolButton, &mut ButtonVariant)>,
) {
    if !edit.is_changed() {
        return;
    }
    for (tool, mut variant) in &mut buttons {
        *variant = if edit.enabled && tool.0 == edit.tool {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Normal
        };
    }
}

pub fn update_edit_palette(
    mut commands: Commands,
    edit: Res<EditMode>,
    history: Res<EditHistory>,
    mut status: Query<&mut Text, With<EditStatus>>,
    undo: Query<(Entity, Has<InteractionDisabled>), With<UndoButton>>,
) {
    if edit.is_changed() {
        for mut text in &mut status {
            text.0 = if edit.enabled {
                "Editing · simulation paused"
            } else {
                "Choose a tool to start editing"
            }
            .into();
        }
    }
    if history.is_changed() {
        for (entity, disabled) in &undo {
            if history.0.is_empty() && !disabled {
                commands.entity(entity).insert(InteractionDisabled);
            } else if !history.0.is_empty() && disabled {
                commands.entity(entity).remove::<InteractionDisabled>();
            }
        }
    }
}

pub fn update_edit_preview(
    hovered: Res<Hovered>,
    edit: Res<EditMode>,
    assets: Res<CreatureAssets>,
    mut preview: Query<(&mut Sprite, &mut Transform, &mut Visibility), With<EditPreview>>,
) {
    let Ok((mut sprite, mut transform, mut visibility)) = preview.single_mut() else {
        return;
    };
    if let Some((row, col)) = hovered.0
        && edit.enabled
        && edit.tool != EditTool::Erase
    {
        sprite.image = if edit.tool == EditTool::Aphid {
            assets.aphid.clone()
        } else {
            assets.ladybug.clone()
        };
        transform.translation = cell_to_world(row, col).extend(5.0);
        *visibility = Visibility::Inherited;
    } else {
        *visibility = Visibility::Hidden;
    }
}
