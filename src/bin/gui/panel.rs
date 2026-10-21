//! The properties panel on the right: its tabs, collapsible sections,
//! scrolling, and the overview tab's board view and food controls.

use crate::board::{BoardView, FoodOverlay, food_colour, set_food_details};
use crate::chart::INK_PRIMARY;
use crate::creatures::CreatureAssets;
use crate::editing::{EditStatus, EditTool, UndoButton, edit_tool_button};
use crate::history::event_list;
use crate::layout::{PANEL_HEADER, PANEL_WIDTH};
use crate::params::{Params, param_group, param_row};
use crate::run_control::{Reseed, RunAction, SEED_DIGITS, SeedField};
use crate::summary::HudText;
use crate::widgets::{run_button, ui_text};
use bevy::feathers::controls::{
    ButtonVariant, FeathersButton, FeathersCheckbox, FeathersDisclosureToggle, FeathersRadio,
    FeathersScrollbar, FeathersTextInput, FeathersTextInputContainer,
};
use bevy::feathers::display::{label, label_small};
use bevy::feathers::focus::FocusIndicator;
use bevy::feathers::theme::{ThemeBackgroundColor, ThemedText};
use bevy::feathers::tokens;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input_focus::InputFocus;
use bevy::picking::cursor::EntityCursor;
use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::ui::Checked;
use bevy::ui_widgets::{
    Activate, RadioGroup, ScrollArea, SelectedTab, Tab, TabActivation, TabList, ToggleChecked,
    ValueChange,
};
use bevy::window::SystemCursorIcon;

#[derive(Component, Clone, Default, FromTemplate)]
pub struct ViewButton(pub BoardView);

/// The radio group holding the two `ViewButton`s.
#[derive(Component, Clone, Default, FromTemplate)]
pub struct BoardViewGroup;

#[derive(Component, Clone, Default, FromTemplate)]
pub struct FoodControls;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
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

/// Identifies the controlled tab list so edit shortcuts can select it too.
#[derive(Component, Clone, Default, FromTemplate)]
pub struct PropertiesTabs;

#[derive(Component, Clone, FromTemplate)]
pub struct TabButton(pub PanelTab);

fn stat_row(caption: &'static str, index: usize) -> impl Scene {
    bsn! {
        Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center }
        Children [
            @label(caption)
            --
            @ui_text("0", 18.0) HudText({index})
        ]
    }
}

fn section_title(title: &'static str) -> impl Scene {
    bsn! {
        Node { margin: UiRect::new(px(0), px(0), px(8), px(6)) }
        Children [@ui_text(title, 16.0)]
    }
}

/// Use headless tabs rather than buttons so keyboard and accessibility behavior
/// comes from the tab list. The underline distinguishes selection from focus.
pub fn tab_button(caption: &'static str, tab: PanelTab) -> impl Scene {
    bsn! {
        Node {
            flex_grow: 1.0, justify_content: JustifyContent::Center,
            padding: UiRect::axes(px(4), px(8)),
            border: UiRect::bottom(px(2)),
        }
        Tab TabButton({tab}) FocusIndicator
        BorderColor::all(Color::NONE)
        Children [@ui_text(caption, 14.0)]
    }
}

/// Apply native pointer and keyboard selection requests to the same state used
/// by edit shortcuts; `sync_sidebar` reflects it back into `SelectedTab`.
pub fn select_properties_tab(
    change: On<ValueChange<Option<Entity>>>,
    lists: Query<(), With<PropertiesTabs>>,
    tabs: Query<(&TabButton, &ChildOf)>,
    mut active: ResMut<ActiveTab>,
    mut panel: ResMut<PropertiesPanel>,
) {
    if lists.contains(change.source)
        && let Some(entity) = change.value
        && let Ok((tab, parent)) = tabs.get(entity)
        && parent.parent() == change.source
    {
        active.0 = tab.0;
        panel.open = true;
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

impl PanelSection {
    fn caption(self) -> &'static str {
        match self {
            PanelSection::Setup => "Run setup",
            PanelSection::Help => "Controls & shortcuts",
        }
    }
}

/// A native disclosure toggle carries the open state as `Checked`, so focus,
/// Space/Enter, and assistive technology all see it. The toggle itself is only
/// a 12px chevron; clicking the caption row toggles it through the same
/// `ToggleChecked` path, keeping the whole row a target as the old button was.
fn section_toggle(section: PanelSection) -> impl Scene {
    bsn! {
        Node {
            margin: UiRect::top(px(16)), padding: UiRect::vertical(px(4)),
            column_gap: px(8), align_items: AlignItems::Center,
        }
        EntityCursor::System(SystemCursorIcon::Pointer)
        on(move |click: On<PointerClick>, toggles: Query<(Entity, &SectionToggle)>, mut commands: Commands| {
            for (entity, toggle) in &toggles {
                if toggle.0 == section && click.button == PointerButton::Primary {
                    commands.trigger(ToggleChecked { entity });
                }
            }
        })
        Children [
            @FeathersDisclosureToggle
            SectionToggle({section})
            on(move |change: On<ValueChange<bool>>, mut sections: ResMut<PanelSections>| {
                match section {
                    PanelSection::Setup => sections.setup = change.value,
                    PanelSection::Help => sections.help = change.value,
                }
            })
            --
            @ui_text(section.caption(), 16.0)
        ]
    }
}

/// Shows open section bodies and mirrors `PanelSections` onto the toggles, so
/// the chevron follows state changed from anywhere, not only its own clicks.
pub fn sync_panel_sections(
    mut commands: Commands,
    sections: Res<PanelSections>,
    mut bodies: Query<(&SectionBody, &mut Node)>,
    toggles: Query<(Entity, &SectionToggle)>,
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
    for (entity, toggle) in &toggles {
        set_checked(&mut commands, entity, sections.is_open(toggle.0));
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
            @label_small("food in each cell")
            --
            // A 2px gap keeps neighbouring steps readable as steps.
            Node { height: px(10), column_gap: px(2) }
            Children [
                @food_swatch(0) -- @food_swatch(1) -- @food_swatch(2) -- @food_swatch(3) -- @food_swatch(4)
                -- @food_swatch(5) -- @food_swatch(6) -- @food_swatch(7) -- @food_swatch(8) -- @food_swatch(9)
            ]
            --
            Node { justify_content: JustifyContent::SpaceBetween }
            Children [@label_small("0 empty") -- @label_small("9 abundant")]
        ]
    }
}

fn board_view_radio(caption: &'static str, view: BoardView) -> impl Scene {
    bsn! {
        @FeathersRadio { @caption: bsn! { Text(caption) ThemedText } }
        ViewButton({view})
    }
}

/// The Population and Food radios. A radio group gives the pair one checked
/// state that assistive technology reads as a single choice.
pub fn board_view_group() -> impl Scene {
    bsn! {
        Node { column_gap: px(16) }
        RadioGroup BoardViewGroup
        on(select_board_view)
        Children [@board_view_radio("Population", BoardView::Population) -- @board_view_radio("Food", BoardView::Food)]
    }
}

/// Picking a view through the radio group. Choosing a view directly forgets
/// that the food checkbox switched to the food view, so unticking the checkbox
/// later does not switch away from a view the user picked.
fn select_board_view(
    change: On<ValueChange<Entity>>,
    buttons: Query<&ViewButton>,
    mut current: ResMut<BoardView>,
    mut overlay: ResMut<FoodOverlay>,
) {
    if let Ok(button) = buttons.get(change.value) {
        *current = button.0;
        overlay.bypass_change_detection().selected_view = false;
    }
}

/// Checks the radio for the current view, which the `F` key and the food
/// checkbox can change too, and shows the food controls only in the food view.
pub fn sync_board_view(
    mut commands: Commands,
    view: Res<BoardView>,
    buttons: Query<(Entity, &ViewButton)>,
    mut controls: Query<&mut Node, With<FoodControls>>,
) {
    if !view.is_changed() {
        return;
    }
    for (entity, button) in &buttons {
        set_checked(&mut commands, entity, button.0 == *view);
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
                Node {
                    position_type: PositionType::Absolute,
                    left: px(16), right: px(16), top: px(20),
                    flex_direction: FlexDirection::Column, row_gap: px(6),
                }
                Children [
                    Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center }
                    Children [
                        @ui_text("Properties", 18.0)
                        --
                        @FeathersButton { @variant: ButtonVariant::Plain }
                        on(toggle_properties)
                        Children [Text("Hide") ThemedText]
                    ]
                    --
                    Node { column_gap: px(4), margin: UiRect::top(px(14)) }
                    TabList { activation: TabActivation::Automatic }
                    PropertiesTabs on(select_properties_tab)
                    Children [
                        @tab_button("Overview", PanelTab::Overview)
                        --
                        @tab_button("Parameters", PanelTab::Parameters)
                        --
                        @tab_button("Edit", PanelTab::Edit)
                    ]
                ]
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
        PanelRoot ScrollArea
        Children [
            Node { flex_direction: FlexDirection::Column, row_gap: px(8), flex_shrink: 0.0 }
            TabPage({PanelTab::Overview})
            Children [
                @section_title("Latest activity")
                --
                @stat_row("Births", 3)
                --
                @stat_row("Deaths", 4)
                --
                @event_list()
                --
                @section_title("Board view")
                --
                @board_view_group()
                --
                @label_small("Aphids: green circles · Ladybugs: red diamonds at distant zoom.")
                --
                Node { flex_direction: FlexDirection::Column, row_gap: px(6) }
                FoodControls
                Children [
                    @FeathersCheckbox { @caption: bsn! { Text("Show food details (F)") ThemedText } }
                    FoodOverlayToggle
                    on(|change: On<ValueChange<bool>>, mut overlay: ResMut<FoodOverlay>, mut view: ResMut<BoardView>| { set_food_details(change.value, &mut overlay, &mut view); })
                    --
                    @food_scale()
                ]
                --
                @section_toggle(PanelSection::Setup)
                --
                Node { display: Display::None, flex_direction: FlexDirection::Column, row_gap: px(8), flex_shrink: 0.0 }
                SectionBody({PanelSection::Setup})
                Children [
                    @stat_row("Run seed", 5)
                    --
                    Node { margin: UiRect::vertical(px(4)) }
                    Children [
                        @FeathersTextInputContainer
                        Node { border: UiRect::ZERO, padding: UiRect::horizontal(px(8)) }
                        Children [@FeathersTextInput { @max_characters: {Some(SEED_DIGITS)} } SeedField]
                    ]
                    --
                    @FeathersButton
                    on(|_: On<Activate>, mut requests: MessageWriter<Reseed>| { requests.write(Reseed); })
                    Children [Text("Restart with this seed") ThemedText]
                    --
                    @label_small("Digits only. Enter, or the button, restarts the run from this seed.")
                    --
                    @label("Reset restores the last saved setup, or the board loaded at launch, using the current parameters and seed.")
                    --
                    @run_button("Save setup (S)", RunAction::Save)
                    --
                    @label_small(save_destination())
                    --
                    @label_small("Food and creature life values are not saved; they are regenerated on load.")
                ]
                --
                @section_toggle(PanelSection::Help)
                --
                Node { display: Display::None, flex_direction: FlexDirection::Column, row_gap: px(10), flex_shrink: 0.0 }
                SectionBody({PanelSection::Help})
                Children [
                    @label("Drag to pan · Scroll to zoom")
                    --
                    @label("Home · Fit the whole board")
                    --
                    @label("Space · Play / pause\nN · Advance one turn\n[ / ] · Change speed\nR · Reset to saved setup")
                    --
                    @label("A / L · Place a creature\nX · Erase a cell\nE · Toggle editing\nZ · Undo an edit\nS · Save setup")
                    --
                    @label_small("When a control has focus, Space activates it. Escape returns to playback shortcuts.")
                ]
            ]
            --
            Node { display: Display::None, flex_direction: FlexDirection::Column, flex_shrink: 0.0 }
            TabPage({PanelTab::Parameters})
            Children [
                @label("Probabilities · apply to future turns")
                --
                @label_small("Drag a value or use − / +. Click to type; Enter or leaving applies it. • marks a change from the saved setup.")
                --
                @param_group("Aphids", 0, 4)
                --
                @param_row(0, params.get(0))
                --
                @param_row(1, params.get(1))
                --
                @param_row(2, params.get(2))
                --
                @param_row(3, params.get(3))
                --
                @param_group("Ladybugs", 4, 8)
                --
                @param_row(4, params.get(4))
                --
                @param_row(5, params.get(5))
                --
                @param_row(6, params.get(6))
                --
                @param_row(7, params.get(7))
                --
                @param_group("Environment", 8, 9)
                --
                @param_row(8, params.get(8))
                --
                @label_small("Type a percentage, drag to adjust, or use − / + for 0.1% steps.")
            ]
            --
            Node { display: Display::None, flex_direction: FlexDirection::Column, row_gap: px(12), flex_shrink: 0.0 }
            TabPage({PanelTab::Edit})
            Children [
                @section_title("Shape the ecosystem")
                --
                @ui_text("Choose a tool to start editing", 14.0) EditStatus
                --
                Node { column_gap: px(6) }
                Children [
                    @edit_tool_button("Aphid", "A", EditTool::Aphid, Some(assets.aphid.clone()))
                    --
                    @edit_tool_button("Ladybug", "L", EditTool::Ladybug, Some(assets.ladybug.clone()))
                    --
                    @edit_tool_button("Erase", "X", EditTool::Erase, None)
                ]
                --
                @label("Click to place. Right click removes one of the selected species.")
                --
                @label("Erase clears all creatures in a cell; food stays in place.")
                --
                @FeathersButton
                UndoButton
                on(|_: On<Activate>, mut actions: MessageWriter<RunAction>| { actions.write(RunAction::Undo); })
                Children [Text("Undo edit (Z)") ThemedText]
                --
                @label_small("Edits restart the turn count and chart. Undo restores the previous board and history, up to 20 edits.")
                --
                @run_button("Done · return to paused view", RunAction::FinishEditing)
                --
                @label_small("Play resumes the simulation. Advancing a turn clears edit undo.")
            ]
        ]
    }).id();
    commands.entity(root).add_child(scroll);
    let scrollbar = commands
        .spawn_scene(bsn! {
            // Feathers hides the scrollbar itself while the content fits.
            @FeathersScrollbar { @target: {scroll} }
            Node {
                position_type: PositionType::Absolute,
                right: px(8), top: px(PANEL_HEADER), bottom: px(16), width: px(8),
            }
        })
        .id();
    commands.entity(root).add_child(scrollbar);
}

/// Keep page visibility, underline, and native tab selection in agreement,
/// including when a global edit shortcut opens a different page.
pub fn sync_sidebar(
    active: Res<ActiveTab>,
    mut pages: Query<(&TabPage, &mut Node)>,
    mut tabs: Query<(Entity, &TabButton, &mut BorderColor)>,
    mut lists: Query<&mut SelectedTab, With<PropertiesTabs>>,
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
    let mut selected = None;
    for (entity, tab, mut border) in &mut tabs {
        let is_selected = tab.0 == active.0;
        // Reuse the primary ink, whose contrast is checked against WINDOW_BG,
        // so selection stays visible without changing the caption's contrast.
        *border = BorderColor::all(if is_selected {
            INK_PRIMARY
        } else {
            Color::NONE
        });
        if is_selected {
            selected = Some(entity);
        }
    }
    for mut selection in &mut lists {
        selection.set_if_neq(SelectedTab(selected));
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
