//! The properties panel on the right: its tabs, collapsible sections,
//! scrolling, and the overview tab's board view and food controls.

use crate::board::{BoardView, FoodOverlay, food_colour, set_food_details};
use crate::creatures::CreatureAssets;
use crate::editing::{EditStatus, EditTool, UndoButton, edit_tool_button};
use crate::layout::{PANEL_HEADER, PANEL_WIDTH};
use crate::params::{Params, param_group, param_row};
use crate::run_control::{Reseed, RunAction, SEED_DIGITS, SeedField};
use crate::summary::HudText;
use crate::widgets::{run_button, ui_text};
use bevy::feathers::controls::{
    ButtonVariant, FeathersButton, FeathersCheckbox, FeathersScrollbar, FeathersTextInput,
    FeathersTextInputContainer,
};
use bevy::feathers::display::{label, label_small};
use bevy::feathers::theme::{ThemeBackgroundColor, ThemedText};
use bevy::feathers::tokens;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::ui::Checked;
use bevy::ui_widgets::{Activate, ValueChange};

#[derive(Component, Clone, Default, FromTemplate)]
pub struct ViewButton(pub BoardView);

#[derive(Component, Clone, Default, FromTemplate)]
pub struct FoodControls;

#[derive(Clone, Copy, Default)]
pub enum PanelSection {
    #[default]
    Setup,
    Help,
}

#[derive(Resource, Default)]
pub struct PanelSections {
    pub setup: bool,
    pub help: bool,
}

impl PanelSections {
    fn is_open(&self, section: PanelSection) -> bool {
        match section {
            PanelSection::Setup => self.setup,
            PanelSection::Help => self.help,
        }
    }
}

#[derive(Component, Clone, Default, FromTemplate)]
pub struct SectionBody(pub PanelSection);

#[derive(Component, Clone, Default, FromTemplate)]
pub struct SectionLabel(pub PanelSection);

#[derive(Component, Clone, Default, FromTemplate)]
pub struct SectionToggle(pub PanelSection);

/// The panel's scrolling root.
#[derive(Component, Clone, Default, FromTemplate)]
pub struct PanelRoot;

/// Tags the panel's food-details checkbox.
#[derive(Component, Clone, Default, FromTemplate)]
pub struct FoodOverlayToggle;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum PanelTab {
    #[default]
    Overview,
    Parameters,
    Edit,
}

#[derive(Resource, Default)]
pub struct ActiveTab(pub PanelTab);

/// Hidden initially so the board and history have room even in small windows.
#[derive(Resource, Default)]
pub struct PropertiesPanel {
    pub open: bool,
}

impl PropertiesPanel {
    pub fn width(&self) -> f32 {
        if self.open { PANEL_WIDTH } else { 0.0 }
    }
}

#[derive(Component, Clone, Default, FromTemplate)]
pub struct PropertiesRoot;

#[derive(Component, Clone, Default, FromTemplate)]
pub struct PropertiesToggleLabel;

#[derive(Component, Clone, Default, FromTemplate)]
pub struct PropertiesToggle;

#[derive(Component, Clone, FromTemplate)]
pub struct TabPage(pub PanelTab);

#[derive(Component, Clone, FromTemplate)]
pub struct TabButton(pub PanelTab);

#[derive(Component, Clone, Default, FromTemplate)]
pub struct PanelScrollbar;

fn stat_row(caption: &'static str, index: usize) -> impl Scene {
    bsn! {
        Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center }
        Children [
            label(caption),
            ({ui_text("0", 18.0)} HudText({index})),
        ]
    }
}

fn section_title(title: &'static str) -> impl Scene {
    bsn! {
        Node { margin: UiRect::new(px(0), px(0), px(8), px(6)) }
        Children [ui_text(title, 16.0)]
    }
}

fn tab_button(caption: &'static str, tab: PanelTab) -> impl Scene {
    bsn! {
        @FeathersButton
        Node { flex_grow: 1.0 }
        TabButton({tab})
        on(move |_: On<Activate>, mut active: ResMut<ActiveTab>, mut panel: ResMut<PropertiesPanel>| {
            active.0 = tab;
            panel.open = true;
        })
        Children [(Text(caption) ThemedText)]
    }
}

/// Names the file the Save button writes, so the panel says where the setup
/// goes rather than leaving the reader to guess at the XDG path. The home
/// directory is abbreviated, as a shell would print it, to keep the line short.
fn save_destination() -> String {
    let Ok(path) = creature_life_cycle::simulation_config_path() else {
        return "No config path is available.".into();
    };
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let shown = home
        .and_then(|home| {
            path.strip_prefix(home)
                .ok()
                .map(|rest| format!("~/{}", rest.display()))
        })
        .unwrap_or_else(|| path.display().to_string());
    format!("Writes to {shown}")
}

fn section_toggle(section: PanelSection) -> impl Scene {
    bsn! {
        @FeathersButton { @variant: ButtonVariant::Plain }
        SectionToggle({section})
        Node { margin: UiRect::top(px(16)), justify_content: JustifyContent::SpaceBetween }
        on(move |_: On<Activate>, mut sections: ResMut<PanelSections>| {
            match section {
                PanelSection::Setup => sections.setup = !sections.setup,
                PanelSection::Help => sections.help = !sections.help,
            }
        })
        Children [({ui_text("", 16.0)} SectionLabel({section}))]
    }
}

pub fn sync_panel_sections(
    sections: Res<PanelSections>,
    mut bodies: Query<(&SectionBody, &mut Node)>,
    mut labels: Query<(&SectionLabel, &mut Text)>,
) {
    if !sections.is_changed() {
        return;
    }
    for (body, mut node) in &mut bodies {
        node.display = if sections.is_open(body.0) {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (label, mut text) in &mut labels {
        let caption = match label.0 {
            PanelSection::Setup => "Run setup",
            PanelSection::Help => "Controls & shortcuts",
        };
        text.0 = format!(
            "{}  {caption}",
            if sections.is_open(label.0) {
                "−"
            } else {
                "+"
            }
        );
    }
}

pub fn toggle_properties(
    _: On<Activate>,
    mut panel: ResMut<PropertiesPanel>,
    mut focus: ResMut<InputFocus>,
) {
    panel.open = !panel.open;
    // A hidden text field or button must not keep owning keyboard shortcuts.
    focus.clear();
}

pub fn sync_properties_panel(
    panel: Res<PropertiesPanel>,
    mut roots: Query<&mut Node, With<PropertiesRoot>>,
    mut labels: Query<&mut Text, With<PropertiesToggleLabel>>,
) {
    if !panel.is_changed() {
        return;
    }
    for mut node in &mut roots {
        node.display = if panel.open {
            Display::Flex
        } else {
            Display::None
        };
    }
    for mut text in &mut labels {
        text.0 = if panel.open {
            "Hide properties"
        } else {
            "Show properties"
        }
        .into();
    }
}

/// One step of the food scale, in the same colour as a cell with that food.
fn food_swatch(food: i32) -> impl Scene {
    bsn! {
        Node { flex_grow: 1.0 }
        BackgroundColor({food_colour(food)})
    }
}

/// Legend for the cell shading. It is always on, whereas the overlay's bars and
/// speckles are optional, so the scale is shown regardless of the checkbox.
fn food_scale() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            margin: UiRect::vertical(px(6)),
        }
        Children [
            label_small("food in each cell"),
            (
                // A 2px gap keeps neighbouring steps readable as steps.
                Node { height: px(10), column_gap: px(2) }
                Children [
                    food_swatch(0), food_swatch(1), food_swatch(2), food_swatch(3),
                    food_swatch(4), food_swatch(5), food_swatch(6), food_swatch(7),
                    food_swatch(8), food_swatch(9),
                ]
            ),
            (
                Node { justify_content: JustifyContent::SpaceBetween }
                Children [ label_small("0 empty"), label_small("9 abundant") ]
            ),
        ]
    }
}

fn board_view_button(caption: &'static str, view: BoardView) -> impl Scene {
    bsn! {
        @FeathersButton
        Node { flex_grow: 1.0 }
        ViewButton({view})
        on(move |_: On<Activate>, mut current: ResMut<BoardView>, mut overlay: ResMut<FoodOverlay>| {
            *current = view;
            overlay.bypass_change_detection().selected_view = false;
        })
        Children [(Text(caption) ThemedText)]
    }
}

pub fn sync_board_view(
    view: Res<BoardView>,
    mut buttons: Query<(&ViewButton, &mut ButtonVariant)>,
    mut controls: Query<&mut Node, With<FoodControls>>,
) {
    if !view.is_changed() {
        return;
    }
    for (button, mut variant) in &mut buttons {
        *variant = if button.0 == *view {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Normal
        };
    }
    for mut node in &mut controls {
        node.display = if *view == BoardView::Food {
            Display::Flex
        } else {
            Display::None
        };
    }
}

pub fn spawn_panel(
    commands: &mut Commands,
    params: &Params,
    ui_camera: Entity,
    assets: &CreatureAssets,
) {
    let root = commands
        .spawn_scene(bsn! {
            Node {
                position_type: PositionType::Absolute,
                right: px(0), top: px(0), bottom: px(0), width: px(PANEL_WIDTH),
                display: Display::None,
            }
            PropertiesRoot
            ThemeBackgroundColor(tokens::WINDOW_BG)
                Children [
                (
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(16), right: px(16), top: px(20),
                        flex_direction: FlexDirection::Column, row_gap: px(6),
                    }
                    Children [
                        (
                            Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center }
                            Children [
                                ui_text("Properties", 18.0),
                                (
                                    @FeathersButton { @variant: ButtonVariant::Plain }
                                    on(toggle_properties)
                                    Children [(Text("Hide") ThemedText)]
                                ),
                            ]
                        ),
                        (
                            Node { column_gap: px(4), margin: UiRect::top(px(14)) }
                            Children [
                                tab_button("Overview", PanelTab::Overview),
                                tab_button("Parameters", PanelTab::Parameters),
                                tab_button("Edit", PanelTab::Edit),
                            ]
                        ),
                    ]
                ),
            ]
        })
        .insert(UiTargetCamera(ui_camera))
        .id();

    let scroll = commands.spawn_scene(bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(16), right: px(28), top: px(PANEL_HEADER), bottom: px(16),
            flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(),
        }
        PanelRoot ScrollPosition
        Children [
            (
                Node { flex_direction: FlexDirection::Column, row_gap: px(8), flex_shrink: 0.0 }
                TabPage({PanelTab::Overview})
                Children [
                    section_title("Latest activity"),
                    stat_row("Births", 3),
                    stat_row("Deaths", 4),
                    section_title("Board view"),
                    (
                        Node { column_gap: px(6) }
                        Children [board_view_button("Population", BoardView::Population), board_view_button("Food", BoardView::Food)]
                    ),
                    label_small("Aphids: green circles · Ladybugs: red diamonds at distant zoom."),
                    (
                        Node { flex_direction: FlexDirection::Column, row_gap: px(6) }
                        FoodControls
                        Children [
                    (
                        @FeathersCheckbox { @caption: bsn! { Text("Show food details (F)") ThemedText } }
                        FoodOverlayToggle
                        on(|change: On<ValueChange<bool>>, mut overlay: ResMut<FoodOverlay>, mut view: ResMut<BoardView>| { set_food_details(change.value, &mut overlay, &mut view); })
                    ),
                    food_scale(),
                        ]
                    ),
                    section_toggle(PanelSection::Setup),
                    (
                        Node { display: Display::None, flex_direction: FlexDirection::Column, row_gap: px(8), flex_shrink: 0.0 }
                        SectionBody({PanelSection::Setup})
                        Children [
                            stat_row("Run seed", 5),
                            (
                                Node { margin: UiRect::vertical(px(4)) }
                                Children [
                                    (
                                        @FeathersTextInputContainer
                                        Node { border: UiRect::ZERO, padding: UiRect::horizontal(px(8)) }
                                        Children [(@FeathersTextInput { @max_characters: {Some(SEED_DIGITS)} } SeedField)]
                                    ),
                                ]
                            ),
                            (
                                @FeathersButton
                                on(|_: On<Activate>, mut requests: MessageWriter<Reseed>| { requests.write(Reseed); })
                                Children [(Text("Restart with this seed") ThemedText)]
                            ),
                            label_small("Digits only. Enter, or the button, restarts the run from this seed."),
                            label("Reset restores the last saved setup, or the board loaded at launch, using the current parameters and seed."),
                            run_button("Save setup (S)", RunAction::Save),
                            label_small(save_destination()),
                            label_small("Food and creature life values are not saved; they are regenerated on load."),
                        ]
                    ),
                    section_toggle(PanelSection::Help),
                    (
                        Node { display: Display::None, flex_direction: FlexDirection::Column, row_gap: px(10), flex_shrink: 0.0 }
                        SectionBody({PanelSection::Help})
                        Children [
                            label("Drag to pan · Scroll to zoom"),
                            label("Home · Fit the whole board"),
                            label("Space · Play / pause\nN · Advance one turn\n[ / ] · Change speed\nR · Reset to saved setup"),
                            label("A / L · Place a creature\nX · Erase a cell\nE · Toggle editing\nZ · Undo an edit\nS · Save setup"),
                            label_small("When a control has focus, Space activates it. Escape returns to playback shortcuts."),
                        ]
                    ),
                ]
            ),
            (
                Node { display: Display::None, flex_direction: FlexDirection::Column, flex_shrink: 0.0 }
                TabPage({PanelTab::Parameters})
                Children [
                    label("Probabilities · apply to future turns"),
                    label_small("Type a percentage, then Enter or leave the field. • marks a change from the saved setup."),
                    param_group("Aphids", 0, 4),
                    param_row(0, params.get(0)), param_row(1, params.get(1)),
                    param_row(2, params.get(2)), param_row(3, params.get(3)),
                    param_group("Ladybugs", 4, 8),
                    param_row(4, params.get(4)), param_row(5, params.get(5)),
                    param_row(6, params.get(6)), param_row(7, params.get(7)),
                    param_group("Environment", 8, 9),
                    param_row(8, params.get(8)),
                    label_small("Focus a slider and use arrow keys for precise changes."),
                ]
            ),
            (
                Node { display: Display::None, flex_direction: FlexDirection::Column, row_gap: px(12), flex_shrink: 0.0 }
                TabPage({PanelTab::Edit})
                Children [
                    section_title("Shape the ecosystem"),
                    ({ui_text("Choose a tool to start editing", 14.0)} EditStatus),
                    (
                        Node { column_gap: px(6) }
                        Children [
                            edit_tool_button("Aphid", "A", EditTool::Aphid, Some(assets.aphid.clone())),
                            edit_tool_button("Ladybug", "L", EditTool::Ladybug, Some(assets.ladybug.clone())),
                            edit_tool_button("Erase", "X", EditTool::Erase, None),
                        ]
                    ),
                    label("Click to place. Right click removes one of the selected species."),
                    label("Erase clears all creatures in a cell; food stays in place."),
                    (
                        @FeathersButton
                        UndoButton
                        on(|_: On<Activate>, mut actions: MessageWriter<RunAction>| { actions.write(RunAction::Undo); })
                        Children [(Text("Undo edit (Z)") ThemedText)]
                    ),
                    label_small("Edits restart the turn count and chart. Undo restores the previous board and history, up to 20 edits."),
                    run_button("Done · return to paused view", RunAction::FinishEditing),
                    label_small("Play resumes the simulation. Advancing a turn clears edit undo."),
                ]
            ),
        ]
    }).id();
    commands.entity(root).add_child(scroll);
    let scrollbar = commands
        .spawn_scene(bsn! {
            @FeathersScrollbar { @target: {scroll} }
            PanelScrollbar
            Node {
                position_type: PositionType::Absolute,
                right: px(8), top: px(PANEL_HEADER), bottom: px(16), width: px(8),
            }
        })
        .id();
    commands.entity(root).add_child(scrollbar);
}

pub fn sync_scrollbar_visibility(
    panel: Query<&ComputedNode, With<PanelRoot>>,
    mut scrollbar: Query<&mut Visibility, With<PanelScrollbar>>,
) {
    if let Ok(node) = panel.single() {
        let visible = if node.content_size().y > node.size().y + 1.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        for mut visibility in &mut scrollbar {
            visibility.set_if_neq(visible);
        }
    }
}

pub fn sync_sidebar(
    active: Res<ActiveTab>,
    mut pages: Query<(&TabPage, &mut Node)>,
    mut tabs: Query<(&TabButton, &mut ButtonVariant)>,
    mut scroll: Query<&mut ScrollPosition, With<PanelRoot>>,
) {
    if !active.is_changed() {
        return;
    }
    for (page, mut node) in &mut pages {
        node.display = if page.0 == active.0 {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (tab, mut variant) in &mut tabs {
        *variant = if tab.0 == active.0 {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Normal
        };
    }
    for mut position in &mut scroll {
        position.y = 0.0;
    }
}

fn set_checked(commands: &mut Commands, entity: Entity, checked: bool) {
    if checked {
        commands.entity(entity).insert(Checked);
    } else {
        commands.entity(entity).remove::<Checked>();
    }
}

/// Wheel scrolling for the panel. `bevy_ui` 0.19 has no built-in wheel handling,
/// so `Overflow::scroll_y` alone clips the panel without ever scrolling it.
pub fn scroll_panel(
    windows: Query<&Window>,
    scroll: Res<AccumulatedMouseScroll>,
    properties: Res<PropertiesPanel>,
    mut panel: Query<(&mut ScrollPosition, &ComputedNode), With<PanelRoot>>,
) {
    if !properties.open || scroll.delta.y == 0.0 {
        return;
    }
    let (Ok(window), Ok((mut position, computed))) = (windows.single(), panel.single_mut()) else {
        return;
    };
    if !window
        .cursor_position()
        .is_some_and(|cursor| cursor.x >= window.width() - PANEL_WIDTH && cursor.y >= PANEL_HEADER)
    {
        return;
    }

    let max_offset =
        ((computed.content_size() - computed.size()) * computed.inverse_scale_factor()).y;
    position.y = scrolled_offset(position.y, scroll.delta.y, scroll.unit, max_offset);
}

/// The panel's scroll offset after one wheel movement. Wheel up (positive delta)
/// scrolls toward the top; the offset stays within the content.
pub fn scrolled_offset(position: f32, delta_y: f32, unit: MouseScrollUnit, max_offset: f32) -> f32 {
    let step = match unit {
        MouseScrollUnit::Line => 21.0,
        MouseScrollUnit::Pixel => 1.0,
    };
    (position - delta_y * step).clamp(0.0, max_offset.max(0.0))
}

/// Mirrors `FoodOverlay` onto the checkbox, so the `F` key updates it too.
pub fn sync_food_checkbox(
    mut commands: Commands,
    overlay: Res<FoodOverlay>,
    toggles: Query<Entity, With<FoodOverlayToggle>>,
) {
    if !overlay.is_changed() {
        return;
    }
    for toggle in &toggles {
        set_checked(&mut commands, toggle, overlay.on);
    }
}
