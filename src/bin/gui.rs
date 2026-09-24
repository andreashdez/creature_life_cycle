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

use bevy::asset::{RenderAssetUsages, embedded_asset, load_embedded_asset};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ScalingMode, Viewport};
use bevy::ecs::system::SystemParam;
use bevy::feathers::constants::fonts;
use bevy::feathers::controls::{
    ButtonVariant, FeathersButton, FeathersCheckbox, FeathersScrollbar, FeathersSlider,
    FeathersTextInput, FeathersTextInputContainer,
};
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::display::{label, label_small};
use bevy::feathers::theme::{ThemeBackgroundColor, ThemedText, UiTheme};
use bevy::feathers::{FeathersPlugins, tokens};
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::input_focus::InputFocus;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};
use bevy::text::{EditableText, EditableTextFilter};
use bevy::ui::{Checked, InteractionDisabled};
use bevy::ui_widgets::{Activate, SliderPrecision, SliderValue, ValueChange};
use creature_life_cycle::{
    AphidParams, Board, Coordinates, CreatureSnapshot, CreatureSnapshotKind, FoodParams,
    LadybugParams, Random, TurnStats, load_configured_board, save_configured_board,
};
use std::collections::{HashMap, VecDeque};

/// Sidebar widths in logical pixels.
const SUMMARY_WIDTH: f32 = 260.0;
const PANEL_WIDTH: f32 = 300.0;
const PANEL_HEADER: f32 = 104.0;
/// Offset of the cell popup from the pointer, and the margin it keeps from the
/// window edges, both as `draw_hover_tooltip` in the previous macroquad GUI.
const POPUP_OFFSET: f32 = 18.0;
const POPUP_MARGIN: f32 = 12.0;

/// World-space size of one board cell, including the gap.
const CELL: f32 = 32.0;
/// Nominal creature body radius as a fraction of the inner cell.
const CREATURE_RADIUS: f32 = 0.16;
/// Includes the sprites' transparent margin and appendages. The painted
/// silhouette stays inside the edit ring; crowded cells use the usual scale.
const CREATURE_SPRITE_SIZE: f32 = (CELL - GAP) * CREATURE_RADIUS * 2.75;
// Count badges, from `draw_count_badge` in the previous macroquad GUI: a small disc on a
// dropped shadow with the count in dark text, sized relative to the cell.
const BADGE_RADIUS: f32 = 0.157;
const BADGE_FONT: f32 = 0.23;
const BADGE_SHADOW_OFFSET: f32 = 0.014;
/// Aphid and ladybug badge corners, matching the mixed-cell creature layout.
const BADGE_SLOTS: [Vec2; 2] = [Vec2::new(0.22, 0.22), Vec2::new(0.78, 0.78)];
/// Badge counts are rasterised in steps of this many pixels. Every distinct
/// size builds its own font atlas, so zooming should not mint a new one per
/// frame.
const BADGE_RASTER_STEP: f32 = 4.0;
const BADGE_RASTER_RANGE: (f32, f32) = (8.0, 96.0);
/// A badge is only worth drawing once the cell is this many logical pixels
/// across, as the `size >= 26.0` gate in the previous macroquad GUI.
const BADGE_MIN_CELL_PIXELS: f32 = 26.0;
/// Macroquad's five-percent gutter and twelve-percent corner radius.
const GAP: f32 = CELL * 0.05;
const CELL_CORNER: f32 = 0.12;
const DEFAULT_SEED: u64 = 42;
const STEP_SECONDS: f32 = 0.45;
/// How long a message stays on the toolbar, matching `SaveStatus::ttl` in the
/// macroquad GUI.
const STATUS_SECONDS: f32 = 4.0;
/// Digits in `u64::MAX`, so the seed field cannot hold a number that could never
/// parse.
const SEED_DIGITS: usize = 20;

// Colours lifted from `draw_cell_background` and the palette in interface.rs.
const EMPTY_CELL: Color = Color::srgb_u8(31, 33, 34);
const FED_CELL: Color = Color::srgb_u8(53, 70, 48);
const EMPTY_CELL_RIM: Color = Color::srgba_u8(48, 53, 50, 90);
const FED_CELL_RIM: Color = Color::srgba_u8(135, 153, 89, 65);
const HOVER: Color = Color::srgb(0.85, 0.87, 0.70);
/// Starting life for hand-placed creatures: the same values `src/lib.rs` gives
/// creatures loaded from the config, and that the macroquad editor uses.
const EDIT_APHID_LIFE: i32 = 10;
const EDIT_LADYBUG_LIFE: i32 = 15;
/// How far the pointer may move between press and release and still count as a
/// click (an edit) rather than a drag (a pan), in logical pixels.
const CLICK_SLOP: f32 = 4.0;
// Edit preview outline per tool, from `draw_edit_overlay` in the previous macroquad GUI.
const BADGE_APHID: Color = Color::srgb_u8(93, 188, 85);
// A step lighter than the macroquad badge red (#d94234). The count sits inside
// the fill, and dark ink on that red measures 4.07:1, under WCAG's 4.5:1 for
// normal text, with white no better at 4.39:1. Holding the hue and chroma and
// lifting OKLCH lightness to 0.63 gives 4.60:1.
const BADGE_LADYBUG: Color = Color::srgb_u8(228, 76, 61);
/// Dark ink, chosen over white by the fills' luminance: 7.48:1 on the aphid
/// badge and 4.60:1 on the ladybug badge.
const BADGE_TEXT: Color = Color::srgb_u8(18, 25, 20);
const BADGE_SHADOW: Color = Color::srgba_u8(0, 0, 0, 115);
const EDIT_APHID: Color = Color::srgba_u8(111, 219, 91, 175);
const EDIT_LADYBUG: Color = Color::srgba_u8(231, 78, 61, 175);
/// Food cap per cell, mirroring `MAX_CELL_FOOD` in `src/lib.rs`.
const MAX_FOOD: i32 = 9;
// Food overlay marks, lifted from `draw_cell_background` and
// `draw_food_speckles` in the previous macroquad GUI.
const FOOD_BAR: Color = Color::srgba_u8(194, 184, 83, 85);
const FOOD_SPECKLE: Color = Color::srgba_u8(219, 205, 116, 42);
/// The streak a creature drags behind it while a move plays. A bright core sits
/// on a wider, fainter glow so the edges fall off instead of ending in a hard
/// rectangle; both ramp to nothing at the tail.
const TRAIL: Color = Color::srgba(0.96, 0.90, 0.69, 0.42);
const TRAIL_GLOW: Color = Color::srgba(0.96, 0.90, 0.69, 0.12);
/// Core and glow width as a fraction of the inner cell, so the streak keeps its
/// proportion to the creature it trails as the camera zooms.
const TRAIL_WIDTH: f32 = 0.15;
const TRAIL_GLOW_WIDTH: f32 = 0.34;
/// Physical-pixel clamp on those widths: under the minimum the streak is back to
/// the hairline it used to be, over the maximum it swamps the creature.
const TRAIL_WIDTH_RANGE: (f32, f32) = (1.5, 22.0);
/// How much of the move passes before the tail starts catching up. The streak
/// grows, holds its length, then collapses into the creature as it settles,
/// rather than popping out of existence when the tween ends.
const TRAIL_LAG: f32 = 0.64;
/// Points along the streak: enough for the alpha ramp to read as a gradient
/// rather than a staircase, few enough to stay cheap per creature.
const TRAIL_POINTS: usize = 8;

/// Height of the population history strip under the board, in logical pixels.
const CHART_HEIGHT: f32 = 220.0;
/// Turns kept in the history, matching `HISTORY_LIMIT` in the previous macroquad GUI.
const HISTORY_LIMIT: usize = 240;
// Plot insets inside the strip: tick gutter left, end labels right, title and
// legend on top, x ticks below.
const PLOT_LEFT: f32 = 52.0;
const PLOT_RIGHT: f32 = 96.0;
const PLOT_TOP: f32 = 68.0;
const PLOT_BOTTOM: f32 = 34.0;
const TOOLTIP_WIDTH: f32 = 240.0;
const CHART_COLLAPSED_HEIGHT: f32 = 40.0;
const CHART_MIN_HEIGHT: f32 = 180.0;
const DETAIL_MIN_CELL_PIXELS: f32 = 28.0;
const EVENT_RULES: Color = Color::srgb_u8(190, 173, 115);

// Chart palette. The series are the board's aphid and ladybug hues, stepped
// down into the dark-surface lightness band (OKLCH L 0.48-0.67, hue held)
// because the board's own steps fail that band on this surface. Checked with
// the dataviz palette validator against `CHART_SURFACE`: every check passes,
// worst deuteranopia delta E 10.0, normal-vision delta E 28.6. The board keeps
// its lighter steps, which are tuned for its green cells rather than this gray.
const CHART_SURFACE: Color = Color::srgb_u8(0x1f, 0x1f, 0x24); // feathers WINDOW_BG
const CHART_APHID: Color = Color::srgb_u8(0x5b, 0xac, 0x43);
const CHART_LADYBUG: Color = Color::srgb_u8(0xc2, 0x44, 0x2f);
const CHART_SERIES: [Color; 2] = [CHART_APHID, CHART_LADYBUG];
const CHART_SERIES_NAMES: [&str; 2] = ["aphids", "ladybugs"];
// Text wears ink, never series colour. Contrast on the surface: primary 14.0,
// secondary 7.66 (feathers TEXT_MAIN), muted 4.64. Feathers' own TEXT_DIM is
// 4.34:1, just under WCAG's 4.5:1 for normal text, hence the lighter muted.
const INK_PRIMARY: Color = Color::srgb_u8(0xed, 0xed, 0xee);
const INK_SECONDARY: Color = Color::srgb_u8(0xb1, 0xb1, 0xb2);
const INK_MUTED: Color = Color::srgb_u8(0x88, 0x88, 0x8b);
const GRIDLINE: Color = Color::srgb_u8(0x36, 0x37, 0x3b); // feathers GRAY_2
const BASELINE: Color = Color::srgb_u8(0x46, 0x47, 0x4d); // feathers GRAY_3
const TOOLTIP_BG: Color = Color::srgb_u8(0x2a, 0x2a, 0x2e); // feathers GRAY_1

/// The screenshot probe can also verify the smallest supported window.
fn review_window_size() -> (u32, u32) {
    if std::env::var_os("CLC_SCREENSHOT").is_some() && std::env::var_os("CLC_COMPACT").is_some() {
        (900, 640)
    } else {
        (1200, 860)
    }
}

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

// ---------------------------------------------------------------------------
// Resources and components
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct BoardRes(Board);

#[derive(Resource)]
struct Rng(Random);

#[derive(Resource)]
struct StepTimer(Timer);

#[derive(Resource)]
struct Playing(bool);

#[derive(Resource, Default)]
struct Hovered(Option<(usize, usize)>);

/// Whether the per-cell food bars and speckles are drawn. The panel checkbox
/// and the `F` key both write this, and the checkbox is synced from it, so the
/// two can never disagree.
#[derive(Resource, Default)]
struct FoodOverlay {
    on: bool,
    /// Whether switching the details on is what selected the food view, so
    /// that switching them off can hand the view back.
    selected_view: bool,
}

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
enum BoardView {
    #[default]
    Population,
    Food,
}

#[derive(Resource, Default)]
struct BoardDetail {
    simplified: bool,
}

#[derive(Component, Clone, Default, FromTemplate)]
struct ViewButton(BoardView);

#[derive(Component, Clone, Default, FromTemplate)]
struct FoodControls;

#[derive(Component)]
struct PopulationLayer;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum EditTool {
    #[default]
    Aphid,
    Ladybug,
    Erase,
}

/// Board editing state, as `editing_enabled` / `edit_tool` in the previous macroquad GUI.
/// Keys and panel controls write it; the panel controls are synced from it.
#[derive(Resource, Default)]
struct EditMode {
    enabled: bool,
    tool: EditTool,
}

impl EditMode {
    /// Picking a tool also starts editing, as `A` / `L` do in the previous macroquad GUI.
    fn select(&mut self, tool: EditTool) {
        self.tool = tool;
        self.enabled = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditAction {
    Add,
    Remove,
}

/// One requested change to one cell. Pointer input sends these and
/// `apply_cell_edits` applies them, so anything else that sends one (the
/// screenshot probe) goes through exactly the same path.
#[derive(Message, Clone, Copy, Debug)]
struct CellEdit {
    row: usize,
    col: usize,
    tool: EditTool,
    action: EditAction,
}

/// Where a board press started and how far the pointer has wandered since, so a
/// click can be told apart from a drag.
#[derive(Resource, Default)]
struct BoardPointer {
    pressed_on_board: bool,
    press_at: Vec2,
    travelled: f32,
}

impl BoardPointer {
    fn is_click(&self) -> bool {
        self.pressed_on_board && self.travelled <= CLICK_SLOP
    }
}

#[derive(Default, Reflect, GizmoConfigGroup)]
struct BoardEditGizmos;

// The movement streak is drawn twice, as a core over a glow, and gizmo line
// width is set per group, so each half needs its own group.
#[derive(Default, Reflect, GizmoConfigGroup)]
struct TrailGizmos;

#[derive(Default, Reflect, GizmoConfigGroup)]
struct TrailGlowGizmos;

#[derive(Component, Clone, Default, FromTemplate)]
struct EditStatus;

#[derive(Component, Clone, Copy, Default, FromTemplate)]
struct EditToolButton(EditTool);

#[derive(Component, Clone, Default, FromTemplate)]
struct UndoButton;

#[derive(Component)]
struct EditPreview;

/// Exact snapshots keep undo faithful to life, food, creature IDs and RNG state.
struct EditSnapshot {
    board: Board,
    random: Random,
    stats: Stats,
    history: History,
}

#[derive(Resource, Default)]
struct EditHistory(VecDeque<EditSnapshot>);

const EDIT_UNDO_LIMIT: usize = 20;

#[derive(Clone, Copy, Default)]
enum PanelSection {
    #[default]
    Setup,
    Help,
}

#[derive(Resource, Default)]
struct PanelSections {
    setup: bool,
    help: bool,
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
struct SectionBody(PanelSection);

#[derive(Component, Clone, Default, FromTemplate)]
struct SectionLabel(PanelSection);

#[derive(Component, Clone, Default, FromTemplate)]
struct SectionToggle(PanelSection);

/// The panel's scrolling root.
#[derive(Component, Clone, Default, FromTemplate)]
struct PanelRoot;

/// The size badge counts are currently rasterised at, and the entity scale that
/// puts that raster back at the right world size.
#[derive(Resource, Clone, Copy)]
struct BadgeRaster {
    font_size: f32,
    scale: f32,
}

impl Default for BadgeRaster {
    fn default() -> Self {
        let (font_size, scale) = badge_text_raster(1.0);
        Self { font_size, scale }
    }
}

/// Badges currently on the board, keyed by cell and creature kind.
#[derive(Resource, Default)]
struct BadgeIndex(HashMap<(usize, usize, usize), Badge>);

#[derive(Clone, Copy)]
struct Badge {
    root: Entity,
    text: Entity,
}

#[derive(Component)]
struct CountBadge;

/// The single mesh holding every cell's food bar and speckles.
#[derive(Component)]
struct FoodLayer;

/// Tags the panel's food-details checkbox.
#[derive(Component, Clone, Default, FromTemplate)]
struct FoodOverlayToggle;

/// Maps `CreatureSnapshot::id` to the entity currently rendering it, which is
/// what replaces the `previous_creatures` / `current_creatures` diffing in the
/// macroquad GUI.
#[derive(Resource, Default)]
struct CreatureIndex(HashMap<usize, Entity>);

/// Board changes that the renderers must react to. The macroquad GUI uses a
/// `board_revision` counter for the same reason; relying on `ResMut` change
/// detection instead would repaint on every parameter tweak, because editing a
/// parameter also touches the board.
#[derive(Resource)]
struct Revision(u64);

/// The nine tunable probabilities, owned by the panel and pushed into the board
/// when they change. Indices match `probability`/`set_probability` in
/// the previous macroquad GUI's parameter panel, so saved configs line up.
#[derive(Resource, Clone, Copy, Default)]
struct Params {
    aphid: AphidParams,
    ladybug: LadybugParams,
    food: FoodParams,
}

#[derive(Resource)]
struct SavedParams(Params);

#[derive(Component, Clone, Default, FromTemplate)]
struct ProbField(usize);

#[derive(Component, Clone, Default, FromTemplate)]
struct ParamCaption(usize);

#[derive(Message)]
struct ParameterReset(usize, usize);

const PARAM_LABELS: [&str; 9] = [
    "Movement",
    "Kill a ladybug",
    "Accomplice bonus",
    "Reproduction",
    "Movement",
    "Kill an aphid",
    "Direction change",
    "Reproduction",
    "Food regeneration",
];

const PARAM_HELP: [&str; 9] = [
    "Chance to move each turn.",
    "Base chance to kill a ladybug in the same cell.",
    "Extra kill chance per other aphid in the cell, capped at 100% total.",
    "Chance to reproduce when another aphid shares the cell.",
    "Chance to move each turn.",
    "Chance to kill an aphid in the same cell.",
    "Chance to choose new preferred directions before moving.",
    "Chance to reproduce when another ladybug shares the cell.",
    "Chance each cell regains one food per turn, up to 9.",
];

impl Params {
    fn get(&self, index: usize) -> f64 {
        match index {
            0 => self.aphid.prob_move,
            1 => self.aphid.prob_kill,
            2 => self.aphid.prob_accomplice,
            3 => self.aphid.prob_procreate,
            4 => self.ladybug.prob_move,
            5 => self.ladybug.prob_kill,
            6 => self.ladybug.prob_direction,
            7 => self.ladybug.prob_procreate,
            _ => self.food.prob_regenerate,
        }
    }

    fn set(&mut self, index: usize, value: f64) {
        match index {
            0 => self.aphid.prob_move = value,
            1 => self.aphid.prob_kill = value,
            2 => self.aphid.prob_accomplice = value,
            3 => self.aphid.prob_procreate = value,
            4 => self.ladybug.prob_move = value,
            5 => self.ladybug.prob_kill = value,
            6 => self.ladybug.prob_direction = value,
            7 => self.ladybug.prob_procreate = value,
            _ => self.food.prob_regenerate = value,
        }
    }
}

#[derive(Clone, Copy)]
struct HistoryPoint {
    turn: usize,
    aphids: usize,
    ladybugs: usize,
}

impl HistoryPoint {
    fn series(&self, index: usize) -> usize {
        if index == 0 {
            self.aphids
        } else {
            self.ladybugs
        }
    }
}

/// Rolling population history, capped like the previous macroquad GUI's.
#[derive(Resource, Default, Clone)]
struct History(VecDeque<HistoryPoint>, VecDeque<HistoryEvent>);

#[derive(Clone, Default)]
struct HistoryEvent {
    turn: usize,
    changes: [Option<(f64, f64)>; 9],
    extinctions: [bool; 2],
}

impl HistoryEvent {
    fn has_rules(&self) -> bool {
        self.changes.iter().any(Option::is_some)
    }

    fn caption(&self) -> String {
        let count = self
            .changes
            .iter()
            .filter(|change| change.is_some())
            .count();
        let mut lines = Vec::new();
        if count > 0 {
            lines.push(format!(
                "{count} rule{} changed after this turn",
                if count == 1 { "" } else { "s" }
            ));
        }
        for (index, extinct) in self.extinctions.iter().enumerate() {
            if *extinct {
                lines.push(format!("{} became extinct", CHART_SERIES_NAMES[index]));
            }
        }
        lines.join("\n")
    }
}

impl History {
    fn record(&mut self, turn: usize, aphids: usize, ladybugs: usize) {
        if self.0.len() == HISTORY_LIMIT {
            self.0.pop_front();
        }
        self.0.push_back(HistoryPoint {
            turn,
            aphids,
            ladybugs,
        });
        if let Some(first) = self.0.front() {
            self.1.retain(|event| event.turn >= first.turn);
        }
    }

    fn event(&mut self, turn: usize) -> &mut HistoryEvent {
        if self.1.back().is_none_or(|event| event.turn != turn) {
            self.1.push_back(HistoryEvent { turn, ..default() });
        }
        self.1.back_mut().unwrap()
    }
}

/// Logical width of the chart strip, set by `layout_viewports`.
#[derive(Resource, Default, PartialEq)]
struct ChartSize(f32, f32);

#[derive(Resource)]
struct ChartPanel {
    height: f32,
    collapsed: bool,
    dragging: bool,
}

impl Default for ChartPanel {
    fn default() -> Self {
        Self {
            height: CHART_HEIGHT,
            collapsed: false,
            dragging: false,
        }
    }
}

impl ChartPanel {
    fn apply(&mut self, action: ChartAction, window_height: f32) {
        match action {
            ChartAction::Toggle => {
                self.collapsed = !self.collapsed;
                self.dragging = false;
            }
            ChartAction::Smaller | ChartAction::Larger => {
                self.collapsed = false;
                let step = if action == ChartAction::Larger {
                    40.0
                } else {
                    -40.0
                };
                self.height = (self.effective_height(window_height) + step).max(CHART_MIN_HEIGHT);
            }
        }
    }

    fn effective_height(&self, window_height: f32) -> f32 {
        if self.collapsed {
            CHART_COLLAPSED_HEIGHT
        } else {
            self.height.clamp(
                CHART_MIN_HEIGHT,
                (window_height - 180.0).clamp(CHART_MIN_HEIGHT, 420.0),
            )
        }
    }
}

#[derive(Component)]
struct ChartRoot;

#[derive(Component)]
struct ChartEventLegend;

#[derive(Component)]
struct ChartExpandedOnly;

#[derive(Component, Clone, Default, FromTemplate)]
struct ChartToggleText;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ChartAction {
    #[default]
    Toggle,
    Smaller,
    Larger,
}

#[derive(Component, Clone, Default, FromTemplate)]
struct ChartControlButton(ChartAction);

#[derive(Component)]
struct ChartEmpty;

/// History index under the crosshair, if the pointer is over the plot.
#[derive(Resource, Default, PartialEq)]
struct ChartHover(Option<usize>);

#[derive(Default, Reflect, GizmoConfigGroup)]
struct ChartGridGizmos;

#[derive(Default, Reflect, GizmoConfigGroup)]
struct ChartSeriesGizmos;

// Cameras are told apart by marker, never by `.single()` over `Camera2d`:
// with the board, chart, and UI cameras all being `Camera2d`, such a query
// matches three entities and quietly returns an error.
#[derive(Component)]
struct BoardCamera;

#[derive(Component)]
struct ChartCamera;

/// End-of-line marker (both the dot and its surface ring) for one series.
#[derive(Component)]
struct EndDot(usize);

/// Every positioned piece of chart text, so one query can lay them all out.
#[derive(Component, Clone, Copy)]
enum ChartLabel {
    AxisTitle,
    YTick(usize),
    XTick(usize),
    EndLabel(usize),
    EndValue(usize),
    Tooltip,
    TooltipTurn,
    TooltipValue(usize),
    TooltipEvent,
}

#[derive(Resource, Default, Clone)]
struct Stats {
    turn: usize,
    aphids: usize,
    ladybugs: usize,
    food: i32,
    births: usize,
    deaths: usize,
    extinct: bool,
    deltas: [i64; 3],
}

#[derive(Component)]
struct Cell {
    row: usize,
    col: usize,
}

#[derive(Component)]
struct Creature;

#[derive(Component)]
struct MoveTween {
    from: Vec2,
    to: Vec2,
    from_scale: f32,
    to_scale: f32,
    timer: Timer,
}

impl MoveTween {
    fn still(at: Vec2, scale: f32) -> Self {
        Self {
            from: at,
            to: at,
            from_scale: scale,
            to_scale: scale,
            timer: Timer::from_seconds(STEP_SECONDS, TimerMode::Once),
        }
    }

    fn born(at: Vec2, scale: f32) -> Self {
        Self {
            from: at,
            to: at,
            from_scale: 0.0,
            to_scale: scale,
            timer: Timer::from_seconds(STEP_SECONDS, TimerMode::Once),
        }
    }
}

#[derive(Component, Clone, Default, FromTemplate)]
struct HudText(usize);

/// The board Reset returns to. Captured at launch and replaced by a save, so
/// Reset follows the config file the way the previous macroquad GUI's reset does by
/// reloading it from disk.
#[derive(Resource)]
struct StartingSetup(String);

/// Seed the current run was started from. The board loaded at launch uses
/// `DEFAULT_SEED`; the panel's seed field replaces it and restarts the run, as
/// `set_seed` does in the previous macroquad GUI.
#[derive(Resource, Clone, Copy)]
struct Seed(u64);

/// What the toolbar reports about the last thing the reader asked for, a save or
/// a seed, cleared after a few seconds as the previous macroquad GUI's footer status is.
#[derive(Resource, Default)]
struct StatusLine {
    message: String,
    success: bool,
    remaining: f32,
}

impl StatusLine {
    fn set(&mut self, message: impl Into<String>, success: bool) {
        self.message = message.into();
        self.success = success;
        self.remaining = STATUS_SECONDS;
    }

    /// Shows a message that stays until another one replaces it, for news the
    /// reader must not miss by looking away for a few seconds.
    fn pin(&mut self, message: impl Into<String>, success: bool) {
        self.message = message.into();
        self.success = success;
        self.remaining = f32::INFINITY;
    }
}

#[derive(Message, Clone, Copy)]
enum RunAction {
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
struct Reseed;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum PanelTab {
    #[default]
    Overview,
    Parameters,
    Edit,
}

#[derive(Resource, Default)]
struct ActiveTab(PanelTab);

/// Hidden initially so the board and history have room even in small windows.
#[derive(Resource, Default)]
struct PropertiesPanel {
    open: bool,
}

impl PropertiesPanel {
    fn width(&self) -> f32 {
        if self.open { PANEL_WIDTH } else { 0.0 }
    }
}

#[derive(Component, Clone, Default, FromTemplate)]
struct PropertiesRoot;

#[derive(Component, Clone, Default, FromTemplate)]
struct PropertiesToggleLabel;

#[derive(Component, Clone, Default, FromTemplate)]
struct PropertiesToggle;

#[derive(Component, Clone, FromTemplate)]
struct TabPage(PanelTab);

#[derive(Component, Clone, FromTemplate)]
struct TabButton(PanelTab);

#[derive(Component, Clone, Default, FromTemplate)]
struct PlayLabel;

#[derive(Component, Clone, Default, FromTemplate)]
struct PlaybackStatus;

#[derive(Component, Clone, Default, FromTemplate)]
struct SpeedLabel;

#[derive(Component, Clone, Default, FromTemplate)]
struct StatusLabel;

/// The panel's seed field. Its text is a draft: it reaches the run only when the
/// reader applies it, so `Seed` and this field can disagree until then.
#[derive(Component, Clone, Default, FromTemplate)]
struct SeedField;

/// Root of the cell inspector popup, which floats beside the pointer.
#[derive(Component)]
struct CellPopup;

/// The popup's two lines, so one system can fill both.
#[derive(Component, Clone, Copy)]
enum PopupLine {
    Heading,
    Counts,
}

#[derive(Component, Clone, Default, FromTemplate)]
struct PanelScrollbar;

/// Tags a slider with the parameter index it edits, so one observer can serve
/// all nine rather than nine closures capturing nine fields.
#[derive(Component, Clone, Copy, FromTemplate)]
struct ProbSlider(usize);

/// Set `CLC_SCREENSHOT=some/path.png` to let the app run a few turns, save a
/// frame, and quit. Lets the prototype be checked without a human watching it.
#[derive(Resource)]
struct ScreenshotProbe {
    path: String,
    timer: Timer,
    stage: u8,
}

/// Board cell the probe edits and hovers.
const PROBE_CELL: (usize, usize) = (2, 3);

type ProbeWidgets<'w, 's> = (
    Query<'w, 's, (Entity, &'static ProbSlider)>,
    Query<'w, 's, (Entity, Has<Checked>), With<FoodOverlayToggle>>,
    Query<'w, 's, (Entity, &'static EditToolButton, &'static ButtonVariant)>,
    Query<'w, 's, (Entity, &'static SectionToggle)>,
    Query<'w, 's, (Entity, &'static ViewButton)>,
    Query<'w, 's, (Entity, &'static ChartControlButton)>,
    Query<'w, 's, Entity, With<PropertiesToggle>>,
);

type ProbeState<'w> = (
    Res<'w, Params>,
    Res<'w, BoardRes>,
    Res<'w, History>,
    Res<'w, FoodOverlay>,
    Res<'w, EditMode>,
    Res<'w, Stats>,
    Res<'w, Playing>,
);

#[expect(
    clippy::too_many_arguments,
    reason = "debug-only probe, not a hot path"
)]
fn screenshot_probe(
    mut commands: Commands,
    time: Res<Time>,
    probe: Option<ResMut<ScreenshotProbe>>,
    mut board_cameras: Query<
        (&Camera, &GlobalTransform, &mut Projection, &mut Transform),
        With<BoardCamera>,
    >,
    (sliders, food_toggles, tool_buttons, sections, views, chart_controls, properties_toggles): ProbeWidgets,
    (params, board, history, overlay, edit, stats, playing): ProbeState,
    mut panel: Query<(&mut ScrollPosition, &ComputedNode), With<PanelRoot>>,
    tabs: Query<(Entity, &TabButton)>,
    mut edits: MessageWriter<CellEdit>,
    mut exit: MessageWriter<AppExit>,
    mut windows: Query<&mut Window>,
    mut actions: MessageWriter<RunAction>,
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
            if let Some((entity, _)) = sliders.iter().find(|(_, slider)| slider.0 == 0) {
                info!(
                    "before ValueChange: params[0]={:.3} board aphid prob_move={:.3}",
                    params.get(0),
                    board.0.aphid_params().prob_move,
                );
                commands.trigger(ValueChange {
                    source: entity,
                    value: 88.0f32,
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
            if let Some((entity, _)) = tabs.iter().find(|(_, tab)| tab.0 == PanelTab::Parameters) {
                commands.trigger(Activate { entity });
            }
            probe.timer = Timer::from_seconds(0.5, TimerMode::Once);
        }
        3 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.replace(".png", "-parameters.png")));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        4 => {
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
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(probe.path.replace(".png", "-scrolled.png")));
            probe.timer = Timer::from_seconds(1.0, TimerMode::Once);
        }
        6 => {
            if let Some((entity, _)) = tabs.iter().find(|(_, tab)| tab.0 == PanelTab::Edit) {
                commands.trigger(Activate { entity });
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
                commands.trigger(Activate { entity });
            }
            for (entity, section) in &sections {
                if matches!(section.0, PanelSection::Setup) {
                    commands.trigger(Activate { entity });
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
                    commands.trigger(Activate { entity });
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
            if let Some((entity, _)) = sliders.iter().find(|(_, slider)| slider.0 == 0) {
                commands.trigger(ValueChange {
                    source: entity,
                    value: 25.0f32,
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
                commands.trigger(Activate { entity });
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
        _ => {
            exit.write(AppExit::Success);
            return;
        }
    }

    probe.stage += 1;
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// Board coordinates are (row, col); Bevy's 2D y-axis points up, so rows count
/// downwards. This is the whole of the layout math that `BoardLayout` replaced.
fn cell_to_world(row: usize, col: usize) -> Vec2 {
    Vec2::new(col as f32 * CELL, -(row as f32) * CELL)
}

/// Where a creature sits in its cell and how large it is drawn, ported from
/// `creature_slots` and `creature_render_scale` in the previous macroquad GUI. The offset
/// is a fraction of the cell from its top-left corner (x right, y down).
///
/// `cell_slot` counts per kind, so a mixed cell needs separate layouts: with one
/// shared layout the first aphid and the first ladybug land on the same spot and
/// one hides the other. Aphids take the upper left and ladybugs the lower right.
fn creature_slot(
    kind: CreatureSnapshotKind,
    cell_slot: usize,
    aphids: usize,
    ladybugs: usize,
) -> (Vec2, f32) {
    const CENTER: [(f32, f32); 5] = [
        (0.50, 0.50),
        (0.36, 0.38),
        (0.64, 0.38),
        (0.38, 0.64),
        (0.64, 0.64),
    ];
    const APHIDS: [(f32, f32); 5] = [
        (0.34, 0.36),
        (0.48, 0.28),
        (0.25, 0.54),
        (0.50, 0.53),
        (0.36, 0.44),
    ];
    const LADYBUGS: [(f32, f32); 5] = [
        (0.68, 0.66),
        (0.54, 0.74),
        (0.76, 0.50),
        (0.57, 0.53),
        (0.68, 0.58),
    ];

    let slots = if aphids == 0 || ladybugs == 0 {
        &CENTER
    } else if kind == CreatureSnapshotKind::Aphid {
        &APHIDS
    } else {
        &LADYBUGS
    };
    let (x, y) = slots[cell_slot.min(slots.len() - 1)];
    let scale = if aphids + ladybugs > 5 { 0.82 } else { 1.0 };
    (Vec2::new(x, y), scale)
}

fn creature_world(location: Coordinates, offset: Vec2) -> Vec2 {
    // `Coordinates.x` is the row and `.y` is the column, matching
    // `creature_position` in the previous macroquad GUI.
    let inner = CELL - GAP;
    let top_left = cell_to_world(location.x, location.y) + Vec2::new(-inner * 0.5, inner * 0.5);
    top_left + Vec2::new(inner * offset.x, -inner * offset.y)
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut cell_materials: ResMut<Assets<CellMaterial>>,
    asset_server: Res<AssetServer>,
    mut status: ResMut<StatusLine>,
) {
    let mut random = Random::with_seed(DEFAULT_SEED);
    let board = match load_configured_board(&mut random) {
        Ok(loaded) => {
            for notice in &loaded.notices {
                eprintln!("{notice}");
            }
            loaded.board
        }
        Err(error) => {
            // A broken config should not keep the window from opening, so the
            // GUI shows the standard board and says so where the reader will
            // see it. Saving later moves the broken file aside instead of
            // overwriting it.
            eprintln!("Error: {error}; showing the standard board.");
            status.pin("Config file is invalid; showing the defaults.", false);
            Board::standard(&mut random)
        }
    };
    let (rows, cols) = (board.rows(), board.cols());
    let summary = board.summary();
    commands.insert_resource(StartingSetup(
        creature_life_cycle::format_simulation_config(&board),
    ));

    // Centre the camera on the board and scale so the whole thing fits, which
    // is the job `layout_for_size` does by hand in the previous macroquad GUI.
    let board_size = Vec2::new(cols as f32 * CELL, rows as f32 * CELL);
    let centre = Vec2::new(
        board_size.x * 0.5 - CELL * 0.5,
        -board_size.y * 0.5 + CELL * 0.5,
    );
    // `AutoMin` keeps the whole board visible whatever the window size and
    // aspect ratio are, preserving the aspect ratio. This is the single line
    // that replaces `layout_for_size` and its fit/clamp arithmetic.
    let mut projection = OrthographicProjection::default_2d();
    projection.scaling_mode = ScalingMode::AutoMin {
        min_width: board_size.x + CELL,
        min_height: board_size.y + CELL,
    };

    commands.spawn((
        BoardCamera,
        Camera2d,
        Camera {
            order: 0,
            ..default()
        },
        Projection::Orthographic(projection),
        Transform::from_translation(centre.extend(999.0)),
    ));

    // The chart gets its own camera so its viewport can be the strip under the
    // board, cleared to the chart surface. `layout_viewports` sets the viewport
    // and makes one world unit equal one logical pixel of the strip.
    commands.spawn((
        ChartCamera,
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::Custom(CHART_SURFACE),
            ..default()
        },
        Projection::Orthographic(OrthographicProjection::default_2d()),
        Transform::from_xyz(0.0, 0.0, 100.0),
        RenderLayers::layer(2),
    ));

    // `bevy_ui` lays out relative to its camera's viewport, so the panel needs
    // its own full-window camera. Sharing the board camera would shift the
    // whole panel right by the viewport offset.
    // `RenderLayers` keeps this camera from drawing the board a second time.
    // Nothing in the world is on layer 1, so it renders only its UI.
    let ui_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: 2,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            RenderLayers::layer(1),
        ))
        .id();

    // One shared quad and ten shared materials, one for each food level.
    // The quad extends past the fill to leave room for the outside border
    // and its antialiasing. The visible fill keeps its original dimensions.
    let cell_mesh = meshes.add(Rectangle::from_length(CELL + GAP));
    let cell_palette = CellPalette(std::array::from_fn(|food| {
        cell_materials.add(CellMaterial {
            fill: food_colour(food as i32).to_linear(),
            rim: EMPTY_CELL_RIM
                .mix(&FED_CELL_RIM, food as f32 / MAX_FOOD as f32)
                .to_linear(),
            // Fill half-size, corner radius, quad size, maximum border width.
            shape: Vec4::new(
                (CELL - GAP) * 0.5,
                (CELL - GAP) * CELL_CORNER,
                CELL + GAP,
                GAP * 0.25,
            ),
        })
    }));
    for row in 0..rows {
        for col in 0..cols {
            commands.spawn((
                Cell { row, col },
                Mesh2d(cell_mesh.clone()),
                MeshMaterial2d(cell_palette.0[0].clone()),
                Transform::from_translation(cell_to_world(row, col).extend(0.0)),
            ));
        }
    }
    commands.insert_resource(cell_palette);

    // The 32px insect art uses nearest sampling at every zoom (also in the
    // sidebar and edit preview). Keep smooth sampling for other UI images.
    let inner = CELL - GAP;
    let font: Handle<Font> = asset_server.load(fonts::REGULAR);
    let creature_assets = CreatureAssets {
        aphid: load_embedded_asset!(
            &*asset_server,
            "../../assets/sprites/aphid.png",
            |settings: &mut ImageLoaderSettings| settings.sampler = ImageSampler::nearest()
        ),
        ladybug: load_embedded_asset!(
            &*asset_server,
            "../../assets/sprites/ladybug.png",
            |settings: &mut ImageLoaderSettings| settings.sampler = ImageSampler::nearest()
        ),
        badge: meshes.add(Circle::new(inner * BADGE_RADIUS)),
        badge_shadow_mesh: meshes.add(Circle::new(inner * (BADGE_RADIUS + BADGE_SHADOW_OFFSET))),
        badge_colours: [
            materials.add(ColorMaterial::from_color(BADGE_APHID)),
            materials.add(ColorMaterial::from_color(BADGE_LADYBUG)),
        ],
        badge_shadow: materials.add(ColorMaterial::from_color(BADGE_SHADOW)),
        font: font.clone(),
    };
    commands.spawn((
        EditPreview,
        Sprite {
            image: creature_assets.aphid.clone(),
            custom_size: Some(Vec2::splat(CREATURE_SPRITE_SIZE * 1.6)),
            color: Color::srgba(1.0, 1.0, 1.0, 0.65),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 5.0),
        Visibility::Hidden,
    ));

    commands.insert_resource(Stats {
        turn: 0,
        aphids: summary.aphids,
        ladybugs: summary.ladybugs,
        food: summary.food,
        births: 0,
        deaths: 0,
        extinct: summary.is_extinct(),
        deltas: [0; 3],
    });
    let params = Params {
        aphid: board.aphid_params(),
        ladybug: board.ladybug_params(),
        food: board.food_params(),
    };
    spawn_panel(&mut commands, &params, ui_camera, &creature_assets);
    spawn_summary(
        &mut commands,
        ui_camera,
        &creature_assets,
        load_embedded_asset!(
            &*asset_server,
            "../../assets/sprites/leaf.png",
            |settings: &mut ImageLoaderSettings| settings.sampler = ImageSampler::nearest()
        ),
    );
    commands.insert_resource(creature_assets);
    commands.insert_resource(SavedParams(params));
    commands.insert_resource(params);

    let mut history = History::default();
    history.record(0, summary.aphids, summary.ladybugs);
    commands.insert_resource(history);

    // End-of-line dots: an 8px dot on a 2px ring of the surface colour, so the
    // dot stays legible where the other line passes through it.
    let dot = meshes.add(Circle::new(4.0));
    let ring = meshes.add(Circle::new(6.0));
    let ring_material = materials.add(ColorMaterial::from_color(CHART_SURFACE));
    for (series, colour) in CHART_SERIES.into_iter().enumerate() {
        commands.spawn((
            EndDot(series),
            Mesh2d(ring.clone()),
            MeshMaterial2d(ring_material.clone()),
            Transform::from_xyz(0.0, 0.0, 10.0),
            Visibility::Hidden,
            RenderLayers::layer(2),
        ));
        commands.spawn((
            EndDot(series),
            Mesh2d(dot.clone()),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(colour))),
            Transform::from_xyz(0.0, 0.0, 11.0),
            Visibility::Hidden,
            RenderLayers::layer(2),
        ));
    }
    spawn_chart_overlay(&mut commands, ui_camera, font.clone());
    spawn_cell_popup(&mut commands, ui_camera, font);
    // Hidden until switched on; `rebuild_food_overlay` fills it in. Sits
    // between the cells (z 0) and the creatures (z 1).
    commands.spawn((
        FoodLayer,
        Mesh2d(meshes.add(food_overlay_mesh(&board))),
        MeshMaterial2d(materials.add(ColorMaterial::default())),
        Transform::from_xyz(0.0, 0.0, 0.5),
        Visibility::Hidden,
    ));
    commands.spawn((
        PopulationLayer,
        Mesh2d(meshes.add(population_mesh(&board))),
        MeshMaterial2d(materials.add(ColorMaterial::default())),
        Transform::from_xyz(0.0, 0.0, 2.0),
        Visibility::Hidden,
    ));
    commands.insert_resource(BoardRes(board));
    commands.insert_resource(Rng(random));

    if let Ok(path) = std::env::var("CLC_SCREENSHOT") {
        // Run turns quickly so the captured chart has real history to show.
        commands.insert_resource(StepTimer(Timer::from_seconds(0.03, TimerMode::Repeating)));
        commands.insert_resource(ScreenshotProbe {
            path,
            timer: Timer::from_seconds(STEP_SECONDS * 6.0, TimerMode::Once),
            stage: 0,
        });
    }
}

/// Bundle board art and shaders so standalone launches find their assets.
fn board_render_assets(app: &mut App) {
    embedded_asset!(app, "../../assets/shaders/board_cell.wgsl");
    embedded_asset!(app, "../../assets/sprites/aphid.png");
    embedded_asset!(app, "../../assets/sprites/ladybug.png");
    embedded_asset!(app, "../../assets/sprites/leaf.png");
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct CellMaterial {
    #[uniform(0)]
    fill: LinearRgba,
    #[uniform(0)]
    rim: LinearRgba,
    #[uniform(0)]
    shape: Vec4,
}

impl Material2d for CellMaterial {
    fn fragment_shader() -> ShaderRef {
        bevy::asset::AssetPath::from_path_buf(bevy::asset::embedded_path!(
            "../../assets/shaders/board_cell.wgsl"
        ))
        .with_source("embedded")
        .into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Resource)]
struct CellPalette([Handle<CellMaterial>; MAX_FOOD as usize + 1]);

#[derive(Resource)]
struct CreatureAssets {
    aphid: Handle<Image>,
    ladybug: Handle<Image>,
    badge: Handle<Mesh>,
    badge_shadow_mesh: Handle<Mesh>,
    badge_colours: [Handle<ColorMaterial>; 2],
    badge_shadow: Handle<ColorMaterial>,
    font: Handle<Font>,
}

// ---------------------------------------------------------------------------
// Parameters panel (bevy_feathers)
// ---------------------------------------------------------------------------

/// A slider for exploration and an exact percentage field for deliberate changes.
fn param_row(index: usize, value: f64) -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column, row_gap: px(6),
            margin: UiRect::bottom(px(18)), flex_shrink: 0.0,
        }
        Children [
            ({ui_text(PARAM_LABELS[index], 14.0)} ParamCaption({index})),
            (
                Node { column_gap: px(8), align_items: AlignItems::Center }
                Children [
                    (
                        @FeathersSlider { @min: 0.0, @max: 100.0, @value: {(value * 100.0) as f32} }
                        Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0) }
                        SliderPrecision(1) ProbSlider({index}) on(on_prob_changed)
                    ),
                    (
                        @FeathersTextInputContainer
                        // Feathers pads only the right edge and reserves a left
                        // border for the number input's coloured sigil. Nothing
                        // paints that border here, and the background is drawn
                        // inside it with the corner radii shrunk by its width,
                        // so the digits sat on the edge and the left corners
                        // came out square. Drop it and pad both sides evenly.
                        Node {
                            width: px(68), min_width: px(68), max_width: px(68),
                            flex_grow: 0.0, flex_shrink: 0.0,
                            border: UiRect::ZERO, padding: UiRect::horizontal(px(8)),
                        }
                        Children [(
                            @FeathersTextInput { @max_characters: {Some(12)} }
                            ProbField({index})
                            EditableTextFilter::new(|c| c.is_ascii_digit() || c == '.')
                        )]
                    ),
                    label_small("%"),
                ]
            ),
            label_small(PARAM_HELP[index]),
        ]
    }
}

fn param_group(title: &'static str, start: usize, end: usize) -> impl Scene {
    bsn! {
        Node { margin: UiRect::vertical(px(10)), align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween }
        Children [
            ui_text(title, 16.0),
            (
                @FeathersButton { @variant: ButtonVariant::Plain }
                on(move |_: On<Activate>, mut resets: MessageWriter<ParameterReset>| {
                    resets.write(ParameterReset(start, end));
                })
                Children [(Text("Restore defaults") ThemedText)]
            ),
        ]
    }
}

fn parse_percentage(text: &str) -> Option<f64> {
    let value = text.trim().parse::<f64>().ok()?;
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value / 100.0)
}

fn percentage_text(value: f64) -> String {
    // Stable decimal percentages; merely focusing a field must not round saved settings.
    format!("{:.6}", value * 100.0)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn commit_parameter_fields(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    mut previous: Local<Option<Entity>>,
    mut fields: Query<(&ProbField, &mut EditableText)>,
    mut params: ResMut<Params>,
    mut status: ResMut<StatusLine>,
    mut resets: MessageReader<ParameterReset>,
) {
    let focused = focus.get();
    let commit = if *previous != focused {
        *previous
    } else if keys.just_pressed(KeyCode::Enter) {
        focused
    } else {
        None
    };
    if let Some(entity) = commit
        && let Ok((field, mut text)) = fields.get_mut(entity)
    {
        let typed = text.value().to_string();
        if let Some(value) = parse_percentage(&typed) {
            if typed.trim() != percentage_text(params.get(field.0)) {
                params.set(field.0, value);
                status.set(
                    format!(
                        "{} set to {}%.",
                        PARAM_LABELS[field.0],
                        percentage_text(value)
                    ),
                    true,
                );
            }
        } else {
            status.set("Enter a percentage from 0 to 100. Value unchanged.", false);
        }
        text.editor.set_text(&percentage_text(params.get(field.0)));
    }
    *previous = focused;
    for reset in resets.read() {
        let defaults = Params::default();
        for index in reset.0..reset.1 {
            params.set(index, defaults.get(index));
        }
    }
}

fn sync_parameter_widgets(
    mut commands: Commands,
    (params, saved): (Res<Params>, Res<SavedParams>),
    focus: Res<InputFocus>,
    mut fields: Query<(Entity, &ProbField, &mut EditableText)>,
    sliders: Query<(Entity, &ProbSlider, &SliderValue)>,
    added_sliders: Query<&Children, Added<ProbSlider>>,
    mut captions: Query<(&ParamCaption, &mut Text)>,
) {
    // The editable field is the single numeric readout; retain Feathers' slider interaction.
    for children in &added_sliders {
        for child in children.iter() {
            commands.entity(child).insert(Visibility::Hidden);
        }
    }
    if params.is_changed() || focus.is_changed() {
        for (entity, field, mut text) in &mut fields {
            if focus.get() != Some(entity) {
                let digits = percentage_text(params.get(field.0));
                if text.value().to_string() != digits {
                    text.editor.set_text(&digits);
                }
            }
        }
        for (entity, slider, value) in &sliders {
            let next = (params.get(slider.0) * 100.0) as f32;
            if value.0 != next {
                commands.entity(entity).insert(SliderValue(next));
            }
        }
    }
    if params.is_changed() || saved.is_changed() {
        for (caption, mut text) in &mut captions {
            text.0 = format!(
                "{}{}",
                PARAM_LABELS[caption.0],
                if (params.get(caption.0) - saved.0.get(caption.0)).abs() > 1e-9 {
                    " •"
                } else {
                    ""
                }
            );
        }
    }
}

/// Shared explicit typography prevents unstyled text falling back to Bevy's
/// larger default font. Feathers controls keep their own inherited styles.
fn ui_text(text: impl Into<String>, size: f32) -> impl Scene {
    bsn! {
        Text(text)
        TextFont {
            font: bevy::text::FontSourceTemplate::Handle(fonts::REGULAR),
            font_size: FontSize::Px(size),
        }
        bevy::app::PropagateOver<TextFont>
        TextColor(INK_PRIMARY)
    }
}

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

fn run_button(caption: &'static str, action: RunAction) -> impl Scene {
    bsn! {
        @FeathersButton
        on(move |_: On<Activate>, mut actions: MessageWriter<RunAction>| { actions.write(action); })
        Children [(Text(caption) ThemedText)]
    }
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

fn sync_panel_sections(
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

fn edit_tool_button(
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

fn population_card(
    caption: &'static str,
    index: usize,
    artwork: Option<Handle<Image>>,
) -> impl Scene {
    bsn! {
        Node {
            flex_shrink: 0.0,
            height: px(76), padding: UiRect::axes(px(12), px(8)),
            column_gap: px(10), align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(8)),
        }
        BackgroundColor(TOOLTIP_BG)
        Children [
            {artwork.map(|image| bsn! {
                Node { width: px(32), height: px(32), flex_shrink: 0.0 }
                ImageNode { image: {image} }
            })},
            (
                Node { flex_direction: FlexDirection::Column, row_gap: px(2), min_width: px(0) }
                Children [
                    label_small(caption),
                    ({ui_text("0", 24.0)} HudText({index})),
                ]
            ),
            (Node { flex_grow: 1.0 }),
            ({ui_text("0", 12.0)} HudText({index + 6}) TextColor(INK_SECONDARY)),
        ]
    }
}

fn spawn_summary(
    commands: &mut Commands,
    camera: Entity,
    assets: &CreatureAssets,
    food_icon: Handle<Image>,
) {
    commands.spawn_scene(bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(0), top: px(0), bottom: px(0), width: px(SUMMARY_WIDTH),
            padding: UiRect::all(px(16)),
            flex_direction: FlexDirection::Column, row_gap: px(12),
        }
        ThemeBackgroundColor(tokens::WINDOW_BG)
        Children [
            ui_text("Aphids & Ladybugs", 21.0),
            population_card("Aphids", 0, Some(assets.aphid.clone())),
            population_card("Ladybugs", 1, Some(assets.ladybug.clone())),
            population_card("Food available", 2, Some(food_icon)),
            (
                Node { column_gap: px(8), align_items: AlignItems::Center }
                Children [
                    (
                        @FeathersButton { @variant: ButtonVariant::Primary }
                        Node { flex_grow: 1.0, min_width: px(72) }
                        on(|_: On<Activate>, mut actions: MessageWriter<RunAction>| { actions.write(RunAction::TogglePlay); })
                        Children [(Text("Pause") ThemedText PlayLabel)]
                    ),
                    run_button("Step", RunAction::Step),
                    run_button("Reset", RunAction::Reset),
                ]
            ),
            (
                Node { column_gap: px(8), align_items: AlignItems::Center }
                Children [
                    run_button("−", RunAction::Slower),
                    (Node { flex_grow: 1.0, justify_content: JustifyContent::Center }
                        Children [({ui_text("", 14.0)} SpeedLabel)]),
                    run_button("+", RunAction::Faster),
                ]
            ),
            ({ui_text("", 14.0)} PlaybackStatus),
            run_button("Fit board", RunAction::Fit),
            ({ui_text("", 14.0)} StatusLabel),
            (Node { flex_grow: 1.0 }),
            (
                @FeathersButton
                PropertiesToggle
                on(toggle_properties)
                Children [(Text("Show properties") ThemedText PropertiesToggleLabel)]
            ),
        ]
    }).insert(UiTargetCamera(camera));
}

fn toggle_properties(
    _: On<Activate>,
    mut panel: ResMut<PropertiesPanel>,
    mut focus: ResMut<InputFocus>,
) {
    panel.open = !panel.open;
    // A hidden text field or button must not keep owning keyboard shortcuts.
    focus.clear();
}

fn sync_properties_panel(
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

/// The food details only say anything over the food view's shading, so
/// switching them on selects that view. Switching them off then hands the view
/// back, rather than leaving the board tinted with no details on it. A view
/// picked from the buttons owns itself: the details come and go under it.
fn set_food_details(on: bool, overlay: &mut FoodOverlay, view: &mut BoardView) {
    overlay.on = on;
    if on {
        overlay.selected_view = *view != BoardView::Food;
        if *view != BoardView::Food {
            *view = BoardView::Food;
        }
    } else if std::mem::take(&mut overlay.selected_view) {
        *view = BoardView::Population;
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

fn sync_board_view(
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

/// One marker per occupied cell and species replaces illegible distant sprites.
fn population_mesh(board: &Board) -> Mesh {
    let mut mesh = OverlayMesh::default();
    for row in 0..board.rows() {
        for col in 0..board.cols() {
            let Some(cell) = board.cell_snapshot(row, col) else {
                continue;
            };
            let mixed = cell.aphids > 0 && cell.ladybugs > 0;
            let centre = cell_to_world(row, col);
            for (index, count) in [cell.aphids, cell.ladybugs].into_iter().enumerate() {
                if count == 0 {
                    continue;
                }
                let offset = if mixed {
                    Vec2::new(-1.0, 1.0) * CELL * 0.2 * if index == 0 { 1.0 } else { -1.0 }
                } else {
                    Vec2::ZERO
                };
                let radius = CELL * if mixed { 0.18 } else { 0.27 };
                let at = centre + offset;
                mesh.disc(at, radius + 1.4, Color::srgb_u8(10, 16, 17));
                if index == 0 {
                    mesh.disc(at, radius, BADGE_APHID);
                } else {
                    mesh.diamond(at, radius, BADGE_LADYBUG);
                }
            }
        }
    }
    mesh.into_mesh()
}

fn update_board_detail(
    cameras: Query<(&Camera, &Projection), With<BoardCamera>>,
    board: Res<BoardRes>,
    revision: Res<Revision>,
    mut detail: ResMut<BoardDetail>,
    mut creatures: Query<&mut Visibility, (With<Creature>, Without<PopulationLayer>)>,
    mut layer: Query<(&Mesh2d, &mut Visibility), With<PopulationLayer>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok((camera, Projection::Orthographic(ortho))) = cameras.single() else {
        return;
    };
    let Some(viewport) = camera.logical_viewport_size() else {
        return;
    };
    let simplified = (CELL - GAP) * viewport.y / ortho.area.height() < DETAIL_MIN_CELL_PIXELS;
    let changed = simplified != detail.simplified;
    if changed {
        detail.simplified = simplified;
    }
    if !(changed || revision.is_changed()) {
        return;
    }
    for mut visibility in &mut creatures {
        *visibility = if simplified {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    if let Ok((handle, mut visibility)) = layer.single_mut() {
        *visibility = if simplified {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if simplified && let Some(mut mesh) = meshes.get_mut(&handle.0) {
            *mesh = population_mesh(&board.0);
        }
    }
}

fn spawn_panel(
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

/// Cell inspector popup. It lives outside the panel and follows the pointer, as
/// `draw_hover_tooltip` in the previous macroquad GUI does, so hovering a cell never
/// moves the controls. `update_cell_popup` places it and fills it in.
fn spawn_cell_popup(commands: &mut Commands, ui_camera: Entity, font: Handle<Font>) {
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

fn sync_scrollbar_visibility(
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

fn sync_sidebar(
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

/// Single observer for all nine sliders. `slider_self_update` would write the
/// value back for us, but we need the index anyway, so do both here.
fn on_prob_changed(
    change: On<ValueChange<f32>>,
    mut commands: Commands,
    sliders: Query<&ProbSlider>,
    mut params: ResMut<Params>,
) {
    commands
        .entity(change.source)
        .insert(SliderValue(change.value));

    if let Ok(slider) = sliders.get(change.source)
        && change.value.is_finite()
    {
        params.set(
            slider.0,
            (f64::from(change.value) * 10.0).round().clamp(0.0, 1000.0) / 1000.0,
        );
    }
}

/// Pushes edited parameters into the board. Deliberately does not bump
/// `Revision`, so tweaking a slider never triggers a full board repaint.
fn apply_params(
    params: Res<Params>,
    mut board: ResMut<BoardRes>,
    mut history: ResMut<History>,
    stats: Res<Stats>,
) {
    if !params.is_changed() {
        return;
    }

    apply_parameter_values(&params, &mut board.0, &mut history, stats.turn);
}

fn apply_parameter_values(params: &Params, board: &mut Board, history: &mut History, turn: usize) {
    let previous = Params {
        aphid: board.aphid_params(),
        ladybug: board.ladybug_params(),
        food: board.food_params(),
    };
    for index in 0..9 {
        if (previous.get(index) - params.get(index)).abs() > 1e-9 {
            let event = history.event(turn);
            let from = event.changes[index].map_or(previous.get(index), |(from, _)| from);
            event.changes[index] = if (from - params.get(index)).abs() > 1e-9 {
                Some((from, params.get(index)))
            } else {
                None
            };
        }
    }
    history
        .1
        .retain(|event| event.has_rules() || event.extinctions.iter().any(|extinct| *extinct));
    board.set_aphid_params(params.aphid);
    board.set_ladybug_params(params.ladybug);
    board.set_food_params(params.food);
}

/// Fits the board and chart between the summary and optional properties panel.
/// Runs when the window or either panel changes. Setting camera viewports also keeps
/// hover picking right for free, since `viewport_to_world_2d` is
/// viewport-aware.
fn layout_viewports(
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

fn chart_resize_input(
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut panel: ResMut<ChartPanel>,
    properties: Res<PropertiesPanel>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    if mouse.just_released(MouseButton::Left) && panel.dragging {
        panel.dragging = false;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let edge = window.height() - panel.effective_height(window.height());
    if !panel.collapsed
        && mouse.just_pressed(MouseButton::Left)
        && cursor.x >= SUMMARY_WIDTH
        && cursor.x < window.width() - properties.width()
        && (cursor.y - edge).abs() <= 5.0
    {
        panel.dragging = true;
    }
    if panel.dragging {
        panel.height = (window.height() - cursor.y).clamp(
            CHART_MIN_HEIGHT,
            (window.height() - 180.0).clamp(CHART_MIN_HEIGHT, 420.0),
        );
    }
}

type ChartNodes<'w, 's> = Query<
    'w,
    's,
    (
        Has<ChartRoot>,
        Has<ChartExpandedOnly>,
        Has<ChartEmpty>,
        Has<ChartEventLegend>,
        &'static mut Node,
    ),
    Or<(With<ChartRoot>, With<ChartExpandedOnly>, With<ChartEmpty>)>,
>;

fn sync_chart_panel(
    size: Res<ChartSize>,
    panel: Res<ChartPanel>,
    properties: Res<PropertiesPanel>,
    history: Res<History>,
    mut nodes: ChartNodes,
    mut labels: Query<&mut Text, With<ChartToggleText>>,
    mut backgrounds: Query<&mut BackgroundColor, With<ChartRoot>>,
) {
    if !(size.is_changed() || panel.is_changed() || properties.is_changed() || history.is_changed())
    {
        return;
    }
    for (root, expanded, empty, event_legend, mut node) in &mut nodes {
        if root {
            node.height = px(size.1);
            node.right = px(properties.width());
        }
        if expanded {
            node.display = if panel.collapsed {
                Display::None
            } else {
                Display::Flex
            };
        }
        if empty {
            node.display = if !panel.collapsed && history.0.len() < 2 {
                Display::Flex
            } else {
                Display::None
            };
            let plot = plot_rect(size.0, size.1);
            node.top = px(plot.min.y + plot.height() * 0.4);
        }
        if event_legend {
            // Both sidebars leave a narrower chart in compact windows.
            node.top = px(if size.0 < 440.0 { 60.0 } else { 42.0 });
        }
    }
    for mut label in &mut labels {
        label.0 = if panel.collapsed {
            "Show chart"
        } else {
            "Hide chart"
        }
        .into();
    }
    for mut background in &mut backgrounds {
        background.0 = if panel.collapsed {
            CHART_SURFACE
        } else {
            Color::NONE
        };
    }
}

// ---------------------------------------------------------------------------
// Population history chart
// ---------------------------------------------------------------------------

/// Plot area inside the strip, in strip-local logical pixels (origin top-left,
/// y down), shared by the marks and the text overlay so they cannot disagree.
fn plot_rect(width: f32, height: f32) -> Rect {
    Rect::new(
        PLOT_LEFT,
        if width < 440.0 {
            PLOT_TOP + 18.0
        } else {
            PLOT_TOP
        },
        (width - PLOT_RIGHT).max(PLOT_LEFT + 1.0),
        height - PLOT_BOTTOM,
    )
}

fn history_x(index: usize, len: usize, plot: Rect) -> f32 {
    if len <= 1 {
        plot.min.x
    } else {
        plot.min.x + plot.width() * index as f32 / (len - 1) as f32
    }
}

/// Snaps a pointer x to the closest recorded turn. Readers aim at a turn, not
/// at a 2px line.
fn nearest_index(x: f32, len: usize, plot: Rect) -> usize {
    if len <= 1 {
        return 0;
    }
    let t = ((x - plot.min.x) / plot.width()).clamp(0.0, 1.0);
    (t * (len - 1) as f32).round() as usize
}

fn value_y(value: usize, top: usize, plot: Rect) -> f32 {
    plot.max.y - plot.height() * value as f32 / top.max(1) as f32
}

/// Strip-local point to chart-camera world space (origin centre, y up).
fn chart_world(point: Vec2, width: f32, height: f32) -> Vec2 {
    Vec2::new(point.x - width * 0.5, height * 0.5 - point.y)
}

/// A clean y-axis for populations up to `max`: returns `(top, step)` with a
/// 1/2/5 x 10^k step and at most four intervals.
fn nice_axis(max: usize) -> (usize, usize) {
    let max = max.max(4);
    let raw = max.div_ceil(4);
    let mut magnitude = 1;
    while magnitude * 10 <= raw {
        magnitude *= 10;
    }
    let step = [1, 2, 5, 10]
        .into_iter()
        .map(|m| m * magnitude)
        .find(|&step| step >= raw)
        .unwrap_or(10 * magnitude);
    (max.div_ceil(step) * step, step)
}

fn history_peak(history: &History) -> usize {
    history
        .0
        .iter()
        .map(|point| point.aphids.max(point.ladybugs))
        .max()
        .unwrap_or(0)
}

fn spawn_chart_overlay(commands: &mut Commands, ui_camera: Entity, font: Handle<Font>) {
    let text = |value: &str, size: f32, colour: Color| {
        (
            Text::new(value),
            TextFont {
                font: font.clone().into(),
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(colour),
        )
    };
    let absolute = || Node {
        position_type: PositionType::Absolute,
        ..default()
    };

    let root = commands
        .spawn((
            ChartRoot,
            BackgroundColor(Color::NONE),
            UiTargetCamera(ui_camera),
            Node {
                position_type: PositionType::Absolute,
                left: px(SUMMARY_WIDTH),
                right: px(0),
                bottom: px(0),
                height: px(CHART_HEIGHT),
                ..default()
            },
        ))
        .with_children(|strip| {
            strip.spawn((
                Node {
                    left: px(16),
                    top: px(12),
                    ..absolute()
                },
                text("Population history", 14.0, INK_PRIMARY),
            ));

            // Legend, always present for two series. Direct end labels only
            // supplement it.
            strip
                .spawn((
                    ChartExpandedOnly,
                    Node {
                        left: px(16),
                        top: px(42),
                        align_items: AlignItems::Center,
                        column_gap: px(6),
                        ..absolute()
                    },
                ))
                .with_children(|legend| {
                    for (index, name) in CHART_SERIES_NAMES.into_iter().enumerate() {
                        let margin = if index > 0 {
                            UiRect::left(px(10))
                        } else {
                            UiRect::ZERO
                        };
                        spawn_line_key(legend, index, margin);
                        legend.spawn(text(name, 13.0, INK_SECONDARY));
                    }
                });

            strip
                .spawn((
                    ChartExpandedOnly,
                    ChartEventLegend,
                    Node {
                        right: px(16),
                        top: px(42),
                        align_items: AlignItems::Center,
                        column_gap: px(6),
                        ..absolute()
                    },
                ))
                .with_children(|legend| {
                    legend.spawn((
                        Node {
                            width: px(6),
                            height: px(6),
                            border: UiRect::all(px(1)),
                            margin: UiRect::right(px(3)),
                            ..default()
                        },
                        BorderColor::all(EVENT_RULES),
                        UiTransform::from_rotation(Rot2::radians(std::f32::consts::FRAC_PI_4)),
                    ));
                    legend.spawn(text("rules changed   × extinction", 12.0, INK_SECONDARY));
                });
            strip.spawn((
                ChartEmpty,
                Node {
                    left: px(16),
                    right: px(16),
                    top: px(100),
                    justify_content: JustifyContent::Center,
                    ..absolute()
                },
                text(
                    "Run the simulation to build population history.",
                    14.0,
                    INK_SECONDARY,
                ),
            ));
            strip.spawn((
                ChartExpandedOnly,
                Node {
                    left: percent(43),
                    right: percent(43),
                    top: px(0),
                    height: px(3),
                    border_radius: BorderRadius::all(px(2)),
                    ..absolute()
                },
                BackgroundColor(BASELINE),
            ));
            for index in 0..6 {
                strip.spawn((
                    ChartLabel::YTick(index),
                    Node {
                        left: px(0),
                        width: px(PLOT_LEFT - 8.0),
                        display: Display::None,
                        ..absolute()
                    },
                    TextLayout::justify(Justify::Right),
                    text("", 12.0, INK_MUTED),
                ));
            }

            // Axis title, right-aligned in the tick gutter on the x-tick row, so
            // the row reads "turn  0 ... 45 ... 90".
            strip.spawn((
                ChartLabel::AxisTitle,
                Node {
                    left: px(0),
                    top: px(CHART_HEIGHT - PLOT_BOTTOM + 8.0),
                    width: px(PLOT_LEFT - 8.0),
                    ..absolute()
                },
                TextLayout::justify(Justify::Right),
                text("turn", 12.0, INK_MUTED),
            ));
            for index in 0..3 {
                strip.spawn((
                    ChartLabel::XTick(index),
                    Node {
                        width: px(64),
                        display: Display::None,
                        ..absolute()
                    },
                    TextLayout::justify(Justify::Center),
                    text("", 12.0, INK_MUTED),
                ));
            }

            for (index, name) in CHART_SERIES_NAMES.into_iter().enumerate() {
                strip
                    .spawn((
                        ChartLabel::EndLabel(index),
                        Node {
                            column_gap: px(4),
                            display: Display::None,
                            ..absolute()
                        },
                    ))
                    .with_children(|label| {
                        label.spawn((ChartLabel::EndValue(index), text("", 13.0, INK_PRIMARY)));
                        label.spawn(text(name, 13.0, INK_SECONDARY));
                    });
            }

            // Spawned last so it draws over the other chart text.
            strip
                .spawn((
                    ChartLabel::Tooltip,
                    Node {
                        width: px(TOOLTIP_WIDTH),
                        padding: UiRect::all(px(8)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4),
                        border_radius: BorderRadius::all(px(6)),
                        display: Display::None,
                        ..absolute()
                    },
                    BackgroundColor(TOOLTIP_BG),
                ))
                .with_children(|tooltip| {
                    tooltip.spawn((ChartLabel::TooltipTurn, text("", 12.0, INK_SECONDARY)));
                    for (index, name) in CHART_SERIES_NAMES.into_iter().enumerate() {
                        tooltip
                            .spawn(Node {
                                align_items: AlignItems::Center,
                                column_gap: px(6),
                                ..default()
                            })
                            .with_children(|row| {
                                spawn_line_key(row, index, UiRect::ZERO);
                                // Values lead; the series name follows.
                                row.spawn((
                                    ChartLabel::TooltipValue(index),
                                    text("", 13.0, INK_PRIMARY),
                                ));
                                row.spawn(text(name, 12.0, INK_SECONDARY));
                            });
                    }
                    tooltip.spawn((ChartLabel::TooltipEvent, text("", 12.0, INK_SECONDARY)));
                });
        })
        .id();
    let controls = commands.spawn_scene(bsn! {
        Node { position_type: PositionType::Absolute, right: px(16), top: px(8), column_gap: px(6), align_items: AlignItems::Center }
        Children [
            label_small("Height"),
            (
                @FeathersButton
                ChartControlButton({ChartAction::Smaller})
                on(|_: On<Activate>, mut panel: ResMut<ChartPanel>, windows: Query<&Window>| {
                    if let Ok(window) = windows.single() { panel.apply(ChartAction::Smaller, window.height()); }
                })
                Children [(Text("−") ThemedText)]
            ),
            (
                @FeathersButton
                ChartControlButton({ChartAction::Larger})
                on(|_: On<Activate>, mut panel: ResMut<ChartPanel>, windows: Query<&Window>| {
                    if let Ok(window) = windows.single() { panel.apply(ChartAction::Larger, window.height()); }
                })
                Children [(Text("+") ThemedText)]
            ),
            (
                @FeathersButton
                ChartControlButton({ChartAction::Toggle})
                on(|_: On<Activate>, mut panel: ResMut<ChartPanel>| { panel.apply(ChartAction::Toggle, 0.0); })
                Children [(Text("Hide chart") ThemedText ChartToggleText)]
            ),
        ]
    }).id();
    commands.entity(root).add_child(controls);
}

/// Legends and tooltips key a line series with a short stroke rather than a
/// box, and the stroke mirrors the mark: aphids solid, ladybugs dashed.
fn spawn_line_key(parent: &mut ChildSpawnerCommands, series: usize, margin: UiRect) {
    parent
        .spawn(Node {
            width: px(16),
            height: px(2),
            column_gap: px(2),
            margin,
            ..default()
        })
        .with_children(|key| {
            for _ in 0..if series == 0 { 1 } else { 3 } {
                key.spawn((
                    Node {
                        flex_grow: 1.0,
                        height: px(2),
                        ..default()
                    },
                    BackgroundColor(CHART_SERIES[series]),
                ));
            }
        });
}

/// Set by the screenshot probe to pin the hovered board cell, since it has no
/// pointer.
#[derive(Resource)]
struct ProbeCell((usize, usize));

/// Turns clicks on the hovered cell into edits while editing: a left click adds
/// the selected creature, a right click removes one.
fn board_edit_input(
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

fn apply_cell_edits(mut edits: MessageReader<CellEdit>, mut sim: Simulation) {
    for edit in edits.read() {
        sim.edit(*edit);
    }
}

/// Editing pauses the run, and resuming the run ends editing, whichever control
/// did it: a key, the panel, or an edit.
fn exclusive_play_and_edit(mut playing: ResMut<Playing>, mut edit: ResMut<EditMode>) {
    if edit.is_changed() && edit.enabled && playing.0 {
        playing.0 = false;
    } else if playing.is_changed() && playing.0 && edit.enabled {
        edit.enabled = false;
    }
}

fn set_checked(commands: &mut Commands, entity: Entity, checked: bool) {
    if checked {
        commands.entity(entity).insert(Checked);
    } else {
        commands.entity(entity).remove::<Checked>();
    }
}

/// Tool selection is shared by pointer controls, shortcuts and playback.
fn sync_edit_controls(
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

fn update_edit_palette(
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

fn update_edit_preview(
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

/// Wheel scrolling for the panel. `bevy_ui` 0.19 has no built-in wheel handling,
/// so `Overflow::scroll_y` alone clips the panel without ever scrolling it.
fn scroll_panel(
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
fn scrolled_offset(position: f32, delta_y: f32, unit: MouseScrollUnit, max_offset: f32) -> f32 {
    let step = match unit {
        MouseScrollUnit::Line => 21.0,
        MouseScrollUnit::Pixel => 1.0,
    };
    (position - delta_y * step).clamp(0.0, max_offset.max(0.0))
}

/// Set by the screenshot probe to pin the crosshair, since it has no pointer.
#[derive(Resource)]
struct ProbeHover(usize);

fn chart_hover(
    windows: Query<&Window>,
    history: Res<History>,
    size: Res<ChartSize>,
    probe: Option<Res<ProbeHover>>,
    mut hover: ResMut<ChartHover>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let len = history.0.len();
    let plot = plot_rect(size.0, size.1);

    let next = if size.1 <= CHART_COLLAPSED_HEIGHT || len < 2 {
        None
    } else {
        match probe {
            Some(probe) => (len > 0).then(|| probe.0.min(len - 1)),
            None => window.cursor_position().and_then(|cursor| {
                let local = Vec2::new(
                    cursor.x - SUMMARY_WIDTH,
                    cursor.y - (window.height() - size.1),
                );
                // A little wider than the plot, so the first and last turns are as
                // easy to reach as the ones in the middle.
                let target = Rect::from_corners(
                    plot.min - Vec2::new(12.0, 0.0),
                    plot.max + Vec2::new(12.0, 0.0),
                );
                (len > 0 && size.0 > 0.0 && target.contains(local))
                    .then(|| nearest_index(local.x, len, plot))
            }),
        }
    };

    // Only write on change, so the label layout below does not rerun every frame.
    if hover.0 != next {
        hover.0 = next;
    }
}

fn draw_chart_marks(
    history: Res<History>,
    size: Res<ChartSize>,
    hover: Res<ChartHover>,
    mut grid: Gizmos<ChartGridGizmos>,
    mut series: Gizmos<ChartSeriesGizmos>,
    mut dots: Query<(&EndDot, &mut Transform, &mut Visibility)>,
) {
    let width = size.0;
    if size.1 <= CHART_COLLAPSED_HEIGHT || history.0.len() < 2 {
        for (_, _, mut visibility) in &mut dots {
            *visibility = Visibility::Hidden;
        }
        return;
    }
    let Some(last) = history.0.back() else {
        return;
    };
    if width <= PLOT_LEFT + PLOT_RIGHT {
        return;
    }
    let plot = plot_rect(width, size.1);
    let len = history.0.len();
    let (top, step) = nice_axis(history_peak(&history));
    let world = |x: f32, y: f32| chart_world(Vec2::new(x, y), width, size.1);

    // Recessive solid hairlines; the zero line is the baseline, a step brighter.
    for value in (0..=top).step_by(step) {
        let y = value_y(value, top, plot);
        let colour = if value == 0 { BASELINE } else { GRIDLINE };
        grid.line_2d(world(plot.min.x, y), world(plot.max.x, y), colour);
    }

    // Populations are small integers, so the two lines often sit on exactly
    // the same value for many turns, and a solid line on top would hide the
    // other completely. Ladybugs are dashed so aphids show through the gaps,
    // as in the previous macroquad GUI. (The palette passes CVD checks on its own; the
    // dash is for coincident values, not colour.)
    for (index, colour) in CHART_SERIES.into_iter().enumerate() {
        let points: Vec<Vec2> = history
            .0
            .iter()
            .enumerate()
            .map(|(i, point)| {
                world(
                    history_x(i, len, plot),
                    value_y(point.series(index), top, plot),
                )
            })
            .collect();
        if index == 0 {
            series.linestrip_2d(points, colour);
        } else {
            // Each dash is a two-point strip, not `line_2d`: gizmos batch lines
            // and strips separately and draw strips last, so `line_2d` dashes
            // would land underneath the aphid strip and vanish in the overlap.
            // Within the strip batch, call order is draw order.
            for (start, end) in dash_segments(&points) {
                series.linestrip_2d([start, end], colour);
            }
        }
    }

    for event in &history.1 {
        let Some(index) = history.0.iter().position(|point| point.turn == event.turn) else {
            continue;
        };
        let x = history_x(index, len, plot);
        let colour = if event.has_rules() {
            EVENT_RULES
        } else {
            INK_SECONDARY
        };
        for y in (plot.min.y as i32..plot.max.y as i32).step_by(6) {
            grid.line_2d(
                world(x, y as f32),
                world(x, (y as f32 + 2.0).min(plot.max.y)),
                GRIDLINE,
            );
        }
        let y = plot.min.y + 5.0;
        if event.has_rules() {
            series.linestrip_2d(
                [
                    world(x, y - 4.0),
                    world(x + 4.0, y),
                    world(x, y + 4.0),
                    world(x - 4.0, y),
                    world(x, y - 4.0),
                ],
                colour,
            );
        }
        if event.extinctions.iter().any(|extinct| *extinct) {
            let y = if event.has_rules() { y + 12.0 } else { y };
            series.line_2d(
                world(x - 3.0, y - 3.0),
                world(x + 3.0, y + 3.0),
                INK_PRIMARY,
            );
            series.line_2d(
                world(x - 3.0, y + 3.0),
                world(x + 3.0, y - 3.0),
                INK_PRIMARY,
            );
        }
    }

    if let Some(index) = hover.0 {
        let x = history_x(index, len, plot);
        grid.line_2d(world(x, plot.min.y), world(x, plot.max.y), INK_MUTED);
    }

    for (dot, mut transform, mut visibility) in &mut dots {
        let end = world(
            history_x(len - 1, len, plot),
            value_y(last.series(dot.0), top, plot),
        );
        transform.translation.x = end.x;
        transform.translation.y = end.y;
        visibility.set_if_neq(Visibility::Inherited);
    }
}

const DASH: f32 = 7.0;
const DASH_PERIOD: f32 = 12.0;

/// Splits a polyline into dash pieces with the dash phase carried across
/// vertices, so the pattern stays even however short the per-turn segments
/// get. Gizmos' own `Dashed` style restarts the pattern on every segment, which
/// draws a full 240-turn history (segments shorter than one dash) as solid.
fn dash_segments(points: &[Vec2]) -> Vec<(Vec2, Vec2)> {
    let mut dashes = Vec::new();
    let mut travelled = 0.0;
    for pair in points.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let length = from.distance(to);
        if length <= f32::EPSILON {
            continue;
        }
        let mut offset = 0.0;
        while offset < length {
            let phase = (travelled + offset) % DASH_PERIOD;
            let remaining = if phase < DASH {
                DASH - phase
            } else {
                DASH_PERIOD - phase
            };
            // The floor stops a vanishing run from stalling the loop once f32
            // can no longer represent `offset + run` as larger than `offset`.
            let run = remaining.min(length - offset).max(0.01);
            if phase < DASH {
                let end = (offset + run).min(length);
                dashes.push((from.lerp(to, offset / length), from.lerp(to, end / length)));
            }
            offset += run;
        }
        travelled += length;
    }
    dashes
}

fn update_chart_labels(
    history: Res<History>,
    size: Res<ChartSize>,
    hover: Res<ChartHover>,
    mut labels: Query<(&ChartLabel, &mut Node, Option<&mut Text>)>,
) {
    if !(history.is_changed() || size.is_changed() || hover.is_changed()) {
        return;
    }
    let width = size.0;
    let Some(last) = history.0.back() else {
        return;
    };
    if width <= PLOT_LEFT + PLOT_RIGHT {
        return;
    }
    let plot = plot_rect(width, size.1);
    let len = history.0.len();
    let (top, step) = nice_axis(history_peak(&history));
    let y_ticks: Vec<usize> = (0..=top).step_by(step).collect();
    let mut x_ticks = vec![0, (len - 1) / 2, len - 1];
    x_ticks.dedup();

    // Direct end labels only while the line ends are far enough apart to label
    // without stacking. When they converge, the legend, the tooltip, and the
    // panel's stats carry the current values instead.
    let end_y = [0, 1].map(|index| value_y(last.series(index), top, plot));
    let show_end_labels = (end_y[0] - end_y[1]).abs() >= 18.0;
    let hovered = hover
        .0
        .and_then(|index| history.0.get(index).map(|point| (index, *point)));

    for (label, mut node, text) in &mut labels {
        let mut show = true;
        match *label {
            ChartLabel::AxisTitle => {
                node.top = px(plot.max.y + 8.0);
            }
            ChartLabel::YTick(index) => match y_ticks.get(index) {
                Some(&value) => {
                    node.top = px(value_y(value, top, plot) - 8.0);
                    set_text(text, value.to_string());
                }
                None => show = false,
            },
            ChartLabel::XTick(index) => match x_ticks.get(index) {
                Some(&at) => {
                    node.left = px(history_x(at, len, plot) - 32.0);
                    node.top = px(plot.max.y + 8.0);
                    set_text(text, history.0[at].turn.to_string());
                }
                None => show = false,
            },
            ChartLabel::EndLabel(index) => {
                show = show_end_labels;
                node.left = px(plot.max.x + 10.0);
                node.top = px(end_y[index] - 8.0);
            }
            ChartLabel::EndValue(index) => set_text(text, last.series(index).to_string()),
            ChartLabel::Tooltip => match hovered {
                Some((index, _)) => {
                    // Beside the crosshair, flipping left near the right edge.
                    let x = history_x(index, len, plot);
                    let left = if x + 12.0 + TOOLTIP_WIDTH <= width {
                        x + 12.0
                    } else {
                        x - 12.0 - TOOLTIP_WIDTH
                    };
                    node.left = px(left.max(0.0));
                    node.top = px(plot.min.y);
                }
                None => show = false,
            },
            ChartLabel::TooltipEvent => {
                let event = hovered
                    .and_then(|(_, point)| history.1.iter().find(|event| event.turn == point.turn));
                show = event.is_some();
                set_text(text, event.map_or_else(String::new, HistoryEvent::caption));
            }
            ChartLabel::TooltipTurn => {
                if let Some((_, point)) = hovered {
                    set_text(text, format!("turn {}", point.turn));
                }
            }
            ChartLabel::TooltipValue(index) => {
                if let Some((_, point)) = hovered {
                    set_text(text, point.series(index).to_string());
                }
            }
        }

        show &= size.1 > CHART_COLLAPSED_HEIGHT && history.0.len() > 1;
        let display = if show { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
}

fn set_text(text: Option<Mut<Text>>, value: String) {
    if let Some(mut text) = text
        && text.0 != value
    {
        text.0 = value;
    }
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

fn keyboard_controls(
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

/// What a run is started from: the board, the rules, and the seed.
#[derive(SystemParam)]
struct RunSetup<'w> {
    start: Res<'w, StartingSetup>,
    params: Res<'w, Params>,
    seed: Res<'w, Seed>,
}

fn apply_run_actions(
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
fn init_seed_field(
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
fn seed_field_keyboard(
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

fn apply_seed(
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
fn save_setup(
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
fn expire_status(time: Res<Time>, mut status: ResMut<StatusLine>) {
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

fn camera_controls(
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

/// The board's area in logical window coordinates: between the sidebars and
/// above the chart strip.
fn board_rect(window: &Window, chart_height: f32, properties: &PropertiesPanel) -> Rect {
    Rect::new(
        SUMMARY_WIDTH,
        0.0,
        window.width() - properties.width(),
        (window.height() - chart_height).max(0.0),
    )
}

/// Replaces the inverse-layout arithmetic the previous macroquad GUI does to find the
/// hovered cell: project the cursor back through the camera instead.
fn hovered_cell(
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

// ---------------------------------------------------------------------------
// Simulation
// ---------------------------------------------------------------------------

fn step_simulation(time: Res<Time>, mut timer: ResMut<StepTimer>, mut sim: Simulation) {
    if !sim.playing.0 || sim.stats.extinct || !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    sim.step();
}

/// Everything one turn touches. Shared by the timer and the panel's "step"
/// button so the two can never drift apart.
#[derive(SystemParam)]
struct Simulation<'w> {
    board: ResMut<'w, BoardRes>,
    rng: ResMut<'w, Rng>,
    stats: ResMut<'w, Stats>,
    revision: ResMut<'w, Revision>,
    history: ResMut<'w, History>,
    playing: ResMut<'w, Playing>,
    edits: ResMut<'w, EditHistory>,
}

impl Simulation<'_> {
    /// Applies one cell edit and returns whether the board changed. Adding
    /// always succeeds in bounds; removing does nothing on a cell without that
    /// kind of creature, and then the run carries on untouched.
    fn edit(&mut self, edit: CellEdit) -> bool {
        let Some(cell) = self.board.0.cell_snapshot(edit.row, edit.col) else {
            return false;
        };
        let can_change = match (edit.tool, edit.action) {
            (EditTool::Erase, _) => cell.aphids + cell.ladybugs > 0,
            (EditTool::Aphid, EditAction::Remove) => cell.aphids > 0,
            (EditTool::Ladybug, EditAction::Remove) => cell.ladybugs > 0,
            (_, EditAction::Add) => true,
        };
        if !can_change {
            return false;
        }
        let snapshot = EditSnapshot {
            board: self.board.0.clone(),
            random: self.rng.0.clone(),
            stats: self.stats.clone(),
            history: self.history.clone(),
        };
        let board = &mut self.board.0;
        let changed = match (edit.tool, edit.action) {
            (EditTool::Aphid, EditAction::Add) => board
                .add_aphid(edit.row, edit.col, EDIT_APHID_LIFE)
                .is_some(),
            (EditTool::Ladybug, EditAction::Add) => board
                .add_ladybug(edit.row, edit.col, EDIT_LADYBUG_LIFE, &mut self.rng.0)
                .is_some(),
            (EditTool::Aphid, EditAction::Remove) => board.remove_aphid_at(edit.row, edit.col),
            (EditTool::Ladybug, EditAction::Remove) => board.remove_ladybug_at(edit.row, edit.col),
            (EditTool::Erase, _) => {
                while board.remove_aphid_at(edit.row, edit.col) {}
                while board.remove_ladybug_at(edit.row, edit.col) {}
                true
            }
        };
        if changed {
            if self.edits.0.len() == EDIT_UNDO_LIMIT {
                self.edits.0.pop_front();
            }
            self.edits.0.push_back(snapshot);
            self.restart_from_edit();
        }
        changed
    }

    fn undo(&mut self) -> bool {
        let Some(snapshot) = self.edits.0.pop_back() else {
            return false;
        };
        self.board.0 = snapshot.board;
        self.rng.0 = snapshot.random;
        *self.stats = snapshot.stats;
        *self.history = snapshot.history;
        self.playing.0 = false;
        self.revision.0 += 1;
        true
    }

    /// An edited board is a new starting setup, as `after_board_edit` treats it
    /// in the previous macroquad GUI: the run pauses, and the turn count and population
    /// history start over from what is on the board now.
    fn restart_from_edit(&mut self) {
        let summary = self.board.0.summary();
        *self.stats = Stats {
            turn: 0,
            aphids: summary.aphids,
            ladybugs: summary.ladybugs,
            food: summary.food,
            births: 0,
            deaths: 0,
            extinct: summary.is_extinct(),
            deltas: [0; 3],
        };
        self.playing.0 = false;
        self.history.0.clear();
        self.history.1.clear();
        self.history.record(0, summary.aphids, summary.ladybugs);
        self.revision.0 += 1;
    }

    /// Advances one turn, records it, and marks the board dirty for the
    /// renderers.
    fn step(&mut self) {
        self.edits.0.clear();
        let TurnStats {
            births,
            deaths,
            summary,
        } = self.board.0.refresh(&mut self.rng.0);

        let extinctions = [
            self.stats.aphids > 0 && summary.aphids == 0,
            self.stats.ladybugs > 0 && summary.ladybugs == 0,
        ];
        let stats = &mut self.stats;
        stats.deltas = [
            summary.aphids as i64 - stats.aphids as i64,
            summary.ladybugs as i64 - stats.ladybugs as i64,
            i64::from(summary.food) - i64::from(stats.food),
        ];
        stats.turn += 1;
        stats.births = births;
        stats.deaths = deaths;
        stats.aphids = summary.aphids;
        stats.ladybugs = summary.ladybugs;
        stats.food = summary.food;
        stats.extinct = summary.is_extinct();
        self.history
            .record(stats.turn, summary.aphids, summary.ladybugs);
        if extinctions.iter().any(|extinct| *extinct) {
            self.history.event(stats.turn).extinctions = extinctions;
        }
        self.revision.0 += 1;
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn recolor_cells(
    board: Res<BoardRes>,
    revision: Res<Revision>,
    palette: Res<CellPalette>,
    view: Res<BoardView>,
    mut cells: Query<(&Cell, &mut MeshMaterial2d<CellMaterial>)>,
) {
    if !(revision.is_changed() || view.is_changed()) {
        return;
    }

    for (cell, mut material) in &mut cells {
        let Some(snapshot) = board.0.cell_snapshot(cell.row, cell.col) else {
            continue;
        };
        let level = if *view == BoardView::Food {
            snapshot.food.clamp(0, MAX_FOOD) as usize
        } else {
            0
        };
        let next = &palette.0[level];
        if material.0 != *next {
            material.0 = next.clone();
        }
    }
}

/// Cell shading by food. Shared with the panel's food scale so the legend is
/// the same colours as the board by construction.
fn food_colour(food: i32) -> Color {
    let t = (food as f32 / MAX_FOOD as f32).clamp(0.0, 1.0);
    EMPTY_CELL.mix(&FED_CELL, t)
}

/// Collects the overlay's triangles. Bars and speckles are appended in draw
/// order, so speckles land on top of bars as in the previous macroquad GUI.
#[derive(Default)]
struct OverlayMesh {
    positions: Vec<[f32; 3]>,
    colours: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl OverlayMesh {
    fn diamond(&mut self, centre: Vec2, radius: f32, colour: Color) {
        let base = self.positions.len() as u32;
        for offset in [Vec2::Y, Vec2::X, Vec2::NEG_Y, Vec2::NEG_X] {
            let point = centre + offset * radius;
            self.positions.push([point.x, point.y, 0.0]);
            self.colours.push(colour.to_linear().to_f32_array());
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A single triangle fan avoids dark seams from overlapping translucent
    /// rectangles and end caps. The radius is clamped for very short bars.
    fn rounded_rect(&mut self, min: Vec2, max: Vec2, radius: f32, colour: Color) {
        let base = self.positions.len() as u32;
        let colour = colour.to_linear().to_f32_array();
        let radius = radius.min((max.x - min.x) * 0.5).min((max.y - min.y) * 0.5);
        let centre = (min + max) * 0.5;
        self.positions.push([centre.x, centre.y, 0.0]);
        self.colours.push(colour);
        for (corner, start) in [
            (Vec2::new(max.x - radius, max.y - radius), 0.0),
            (Vec2::new(min.x + radius, max.y - radius), 1.0),
            (Vec2::new(min.x + radius, min.y + radius), 2.0),
            (Vec2::new(max.x - radius, min.y + radius), 3.0),
        ] {
            for step in 0..=6 {
                let angle = (start + step as f32 / 6.0) * std::f32::consts::FRAC_PI_2;
                let point = corner + Vec2::new(angle.cos(), angle.sin()) * radius;
                self.positions.push([point.x, point.y, 0.0]);
                self.colours.push(colour);
            }
        }
        for step in 0..28 {
            self.indices
                .extend([base, base + 1 + step, base + 1 + (step + 1) % 28]);
        }
    }

    /// A hexagon: at speckle size it is indistinguishable from a circle, at a
    /// fraction of the triangles.
    fn disc(&mut self, centre: Vec2, radius: f32, colour: Color) {
        let base = self.positions.len() as u32;
        let colour = colour.to_linear().to_f32_array();
        self.positions.push([centre.x, centre.y, 0.0]);
        self.colours.push(colour);
        for step in 0..6 {
            let angle = step as f32 * std::f32::consts::TAU / 6.0;
            self.positions.push([
                centre.x + radius * angle.cos(),
                centre.y + radius * angle.sin(),
                0.0,
            ]);
            self.colours.push(colour);
        }
        for step in 0..6 {
            self.indices
                .extend([base, base + 1 + step, base + 1 + (step + 1) % 6]);
        }
    }

    fn into_mesh(mut self) -> Mesh {
        // A board with no food would otherwise produce an empty vertex buffer.
        // One zero-area, fully transparent triangle keeps the mesh valid.
        if self.indices.is_empty() {
            self.positions.extend([[0.0; 3]; 3]);
            self.colours.extend([[0.0; 4]; 3]);
            self.indices.extend([0, 1, 2]);
        }
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colours)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

/// Builds every cell's food bar and speckles into one mesh, using the
/// macroquad GUI's proportions (`draw_cell_background`, `draw_food_speckles`)
/// converted from a y-down cell rectangle to y-up world space.
fn food_overlay_mesh(board: &Board) -> Mesh {
    let inner = CELL - GAP;
    let mut mesh = OverlayMesh::default();

    for row in 0..board.rows() {
        for col in 0..board.cols() {
            let Some(snapshot) = board.cell_snapshot(row, col) else {
                continue;
            };
            let food = snapshot.food.clamp(0, MAX_FOOD);
            if food == 0 {
                continue;
            }
            // Top-left corner of the cell's inner square.
            let origin = cell_to_world(row, col) + Vec2::new(-inner * 0.5, inner * 0.5);

            // Bar along the bottom, its length the food level.
            let left = origin.x + inner * 0.14;
            let top = origin.y - inner * 0.82;
            let width = inner * 0.72 * food as f32 / MAX_FOOD as f32;
            let height = inner * 0.055;
            mesh.rounded_rect(
                Vec2::new(left, top - height),
                Vec2::new(left + width, top),
                height * 0.5,
                FOOD_BAR,
            );

            for offset in speckle_offsets(row, col, food) {
                let centre = origin + Vec2::new(inner * offset.x, -inner * offset.y);
                mesh.disc(centre, inner * 0.02, FOOD_SPECKLE);
            }
        }
    }

    mesh.into_mesh()
}

/// Speckle positions for a cell, one per unit of food, as fractions of the cell
/// measured from its top-left corner (x right, y down). Positions are fixed per
/// cell, so speckles stay put as food changes rather than jittering every turn.
///
/// The macroquad GUI capped this at six, which made cells holding 6, 7, 8 and 9
/// food look identical apart from bar length. There is room for all nine: across
/// a 100x100 board the closest two centres are 0.17 of a cell apart against a
/// speckle diameter of 0.04, and none comes within reach of the bar.
fn speckle_offsets(row: usize, col: usize, food: i32) -> impl Iterator<Item = Vec2> {
    (0..food.clamp(0, MAX_FOOD) as usize).map(move |dot| {
        let seed = row * 73 + col * 41 + dot * 19;
        Vec2::new(
            0.18 + (seed % 59) as f32 / 92.0,
            0.18 + (seed % 43) as f32 / 78.0,
        )
    })
}

/// Rebuilds the overlay after a turn, or when it is switched on. While it is
/// off nothing is built at all.
fn rebuild_food_overlay(
    board: Res<BoardRes>,
    revision: Res<Revision>,
    overlay: Res<FoodOverlay>,
    view: Res<BoardView>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut layer: Query<(&Mesh2d, &mut Visibility), With<FoodLayer>>,
) {
    if !(revision.is_changed() || overlay.is_changed() || view.is_changed()) {
        return;
    }
    let Ok((handle, mut visibility)) = layer.single_mut() else {
        return;
    };

    if !overlay.on || *view != BoardView::Food {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    if let Some(mut mesh) = meshes.get_mut(&handle.0) {
        *mesh = food_overlay_mesh(&board.0);
    }
    visibility.set_if_neq(Visibility::Inherited);
}

/// Mirrors `FoodOverlay` onto the checkbox, so the `F` key updates it too.
fn sync_food_checkbox(
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

/// Reconciles live creature entities against `Board::creature_snapshots`,
/// keyed by the stable snapshot id: spawn the new, despawn the gone, retarget
/// the rest. This is the counterpart to building `AnimatedCreature` values.
fn sync_creatures(
    mut commands: Commands,
    board: Res<BoardRes>,
    revision: Res<Revision>,
    assets: Res<CreatureAssets>,
    mut index: ResMut<CreatureIndex>,
    mut tweens: Query<(&mut MoveTween, &Transform, &mut Sprite), With<Creature>>,
) {
    if !revision.is_changed() {
        return;
    }

    let snapshots = board.0.creature_snapshots();
    let mut seen = HashMap::with_capacity(snapshots.len());

    for snapshot in &snapshots {
        let location = snapshot.location;
        let (aphids, ladybugs) = board
            .0
            .cell_snapshot(location.x, location.y)
            .map_or((0, 0), |cell| (cell.aphids, cell.ladybugs));
        let (offset, scale) = creature_slot(snapshot.kind, snapshot.cell_slot, aphids, ladybugs);
        let target = creature_world(location, offset);
        let image = match snapshot.kind {
            CreatureSnapshotKind::Aphid => assets.aphid.clone(),
            CreatureSnapshotKind::Ladybug => assets.ladybug.clone(),
        };

        match index.0.get(&snapshot.id) {
            Some(&entity) => {
                if let Ok((mut tween, transform, mut sprite)) = tweens.get_mut(entity) {
                    // Undo followed by a new edit can reuse an ID for another species.
                    if sprite.image != image {
                        sprite.image = image;
                    }
                    *tween = MoveTween {
                        from: transform.translation.truncate(),
                        to: target,
                        from_scale: transform.scale.x.max(0.001),
                        to_scale: scale,
                        timer: Timer::from_seconds(STEP_SECONDS, TimerMode::Once),
                    };
                }
                seen.insert(snapshot.id, entity);
            }
            None => {
                let entity = commands
                    .spawn((
                        Creature,
                        Sprite {
                            image,
                            custom_size: Some(Vec2::splat(CREATURE_SPRITE_SIZE)),
                            ..default()
                        },
                        Transform::from_translation(target.extend(1.0))
                            .with_scale(Vec3::splat(0.001)),
                        if index.0.is_empty() {
                            MoveTween::still(target, scale)
                        } else {
                            MoveTween::born(target, scale)
                        },
                    ))
                    .id();
                seen.insert(snapshot.id, entity);
            }
        }
    }

    for (id, entity) in index.0.iter() {
        if !seen.contains_key(id) {
            commands.entity(*entity).despawn();
        }
    }

    index.0 = seen;
}

/// Cells holding more than one creature of a kind, as `(location, kind index,
/// count)`. Kind index 0 is aphids and 1 ladybugs, matching `BADGE_SLOTS`.
fn crowded_cells(snapshots: &[CreatureSnapshot]) -> Vec<(Coordinates, usize, usize)> {
    let mut counts: HashMap<(Coordinates, usize), usize> = HashMap::new();
    for snapshot in snapshots {
        let kind = usize::from(snapshot.kind == CreatureSnapshotKind::Ladybug);
        *counts.entry((snapshot.location, kind)).or_default() += 1;
    }
    let mut crowded: Vec<(Coordinates, usize, usize)> = counts
        .into_iter()
        .filter(|&(_, count)| count > 1)
        .map(|((location, kind), count)| (location, kind, count))
        .collect();
    // A stable order keeps spawning deterministic, which the tests rely on.
    crowded.sort_by_key(|&(location, kind, _)| (location.x, location.y, kind));
    crowded
}

/// Keeps one badge per crowded cell and kind, spawning, relabelling and
/// despawning as the board changes.
fn sync_count_badges(
    mut commands: Commands,
    board: Res<BoardRes>,
    revision: Res<Revision>,
    assets: Res<CreatureAssets>,
    mut badges: ResMut<BadgeIndex>,
    raster: Res<BadgeRaster>,
    mut labels: Query<&mut Text2d, With<CountBadge>>,
) {
    if !revision.is_changed() {
        return;
    }

    let inner = CELL - GAP;
    let mut seen = HashMap::new();
    for (location, kind, count) in crowded_cells(&board.0.creature_snapshots()) {
        let key = (location.x, location.y, kind);
        let label = count.to_string();

        let badge = match badges.0.get(&key) {
            Some(&badge) => {
                if let Ok(mut text) = labels.get_mut(badge.text)
                    && text.0 != label
                {
                    text.0 = label;
                }
                badge
            }
            None => {
                // Above the creatures, which sit at z 1.
                let position = creature_world(location, BADGE_SLOTS[kind]).extend(2.0);
                let offset = inner * BADGE_SHADOW_OFFSET;
                let root = commands
                    .spawn((
                        CountBadge,
                        Mesh2d(assets.badge.clone()),
                        MeshMaterial2d(assets.badge_colours[kind].clone()),
                        Transform::from_translation(position),
                    ))
                    .id();
                let text = commands
                    .spawn((
                        CountBadge,
                        Text2d::new(label),
                        TextFont {
                            font: assets.font.clone().into(),
                            font_size: FontSize::Px(raster.font_size),
                            ..default()
                        },
                        TextColor(BADGE_TEXT),
                        Transform::from_xyz(0.0, 0.0, 0.02).with_scale(Vec3::splat(raster.scale)),
                        ChildOf(root),
                    ))
                    .id();
                commands.spawn((
                    CountBadge,
                    Mesh2d(assets.badge_shadow_mesh.clone()),
                    MeshMaterial2d(assets.badge_shadow.clone()),
                    Transform::from_xyz(offset, -offset, -0.01),
                    ChildOf(root),
                ));
                Badge { root, text }
            }
        };
        seen.insert(key, badge);
    }

    for (key, badge) in badges.0.iter() {
        if !seen.contains_key(key) {
            commands.entity(badge.root).despawn();
        }
    }
    badges.0 = seen;
}

/// Badges are only legible once a cell covers enough of the screen, so they
/// follow the camera's zoom rather than being drawn at every scale.
fn scale_count_badges(
    camera: Query<(&Camera, &Projection), With<BoardCamera>>,
    badges: Res<BadgeIndex>,
    mut raster: ResMut<BadgeRaster>,
    mut visibility: Query<&mut Visibility, With<CountBadge>>,
    mut labels: BadgeLabels,
) {
    let Ok((camera, Projection::Orthographic(ortho))) = camera.single() else {
        return;
    };
    let viewport_height = camera.logical_viewport_size().map(|viewport| viewport.y);
    let legible = viewport_height.is_some_and(|height| badges_legible(height, ortho.area.height()));

    // Rasterise the counts at the size they are actually drawn. `Text2d` builds
    // its glyph atlas from the font size and the window's scale factor alone,
    // ignoring camera zoom, so a font size in world units is upscaled on screen
    // and looks pixelated. The entity is scaled back down to keep the badge the
    // same size on the board.
    if let Some(height) = viewport_height
        && ortho.area.height() > 0.0
    {
        let (font_size, scale) = badge_text_raster(height / ortho.area.height());
        if font_size != raster.font_size {
            *raster = BadgeRaster { font_size, scale };
            for (mut font, mut transform) in &mut labels {
                font.font_size = FontSize::Px(font_size);
                transform.scale = Vec3::splat(scale);
            }
        }
    }

    let wanted = if legible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for badge in badges.0.values() {
        if let Ok(mut visibility) = visibility.get_mut(badge.root) {
            visibility.set_if_neq(wanted);
        }
    }
}

/// The badge count labels, whose raster size follows the zoom.
type BadgeLabels<'w, 's> = Query<
    'w,
    's,
    (&'static mut TextFont, &'static mut Transform),
    (With<CountBadge>, With<Text2d>),
>;

/// The font size to rasterise a badge count at, with the entity scale that keeps
/// its world size unchanged, for a board drawn at `pixels_per_unit` logical
/// pixels per world unit. `font_size * scale` is always the badge's world height.
fn badge_text_raster(pixels_per_unit: f32) -> (f32, f32) {
    let height = (CELL - GAP) * BADGE_FONT;
    let (min, max) = BADGE_RASTER_RANGE;
    let wanted = height * pixels_per_unit.max(0.0);
    let font_size = ((wanted / BADGE_RASTER_STEP).round() * BADGE_RASTER_STEP).clamp(min, max);
    (font_size, height / font_size)
}

/// Whether a cell is wide enough on screen for a badge to be readable, given the
/// camera's viewport height and the world height it shows.
fn badges_legible(viewport_height: f32, area_height: f32) -> bool {
    if area_height <= 0.0 {
        return false;
    }
    (CELL - GAP) * (viewport_height / area_height) >= BADGE_MIN_CELL_PIXELS
}

/// Replaces `draw_animated_creatures`, `animation_elapsed`, `animation_progress`
/// and `smooth_progress` with one system over a component.
fn advance_tweens(
    time: Res<Time>,
    detail: Res<BoardDetail>,
    mut creatures: Query<(&mut Transform, &mut MoveTween)>,
    mut trail: Gizmos<TrailGizmos>,
    mut glow: Gizmos<TrailGlowGizmos>,
) {
    for (mut transform, mut tween) in &mut creatures {
        tween.timer.tick(time.delta());

        let t = tween.timer.fraction().clamp(0.0, 1.0);
        let progress = t * t * (3.0 - 2.0 * t); // same smoothstep as gui.rs:1280

        let position = tween.from.lerp(tween.to, progress);
        transform.translation = position.extend(1.0);
        let scale = tween.from_scale + (tween.to_scale - tween.from_scale) * progress;
        transform.scale = Vec3::splat(scale.max(0.001));

        // Movement trail, carried over from `draw_animated_creatures`. Gizmos
        // is immediate-mode, so this stays a per-frame draw.
        if !detail.simplified && tween.from.distance_squared(tween.to) > 0.01 {
            let ramp = |base: Color| {
                trail_points(tween.from, position, t)
                    .map(move |(point, alpha)| (point, base.with_alpha(base.alpha() * alpha)))
            };
            glow.linestrip_gradient_2d(ramp(TRAIL_GLOW));
            trail.linestrip_gradient_2d(ramp(TRAIL));
        }
    }
}

/// The streak behind a creature that is `t` through its move, from the tail up
/// to the creature itself, each point paired with the fraction of the streak's
/// alpha it carries. The ramp is quadratic, so the streak is a faint wash that
/// gathers into a bright head rather than a slab of even colour.
fn trail_points(from: Vec2, head: Vec2, t: f32) -> impl Iterator<Item = (Vec2, f32)> {
    // The tail stays put until `TRAIL_LAG` of the move has passed, then closes
    // on the head, reaching it exactly as the move ends.
    let lag = ((t - TRAIL_LAG) / (1.0 - TRAIL_LAG)).clamp(0.0, 1.0);
    let tail = from.lerp(head, lag * lag * (3.0 - 2.0 * lag));
    (0..TRAIL_POINTS).map(move |i| {
        let along = i as f32 / (TRAIL_POINTS - 1) as f32;
        (tail.lerp(head, along), along * along)
    })
}

/// Keeps the streak's weight proportional to the creatures. Gizmo line width is
/// in physical pixels and takes no notice of the camera, so both widths are
/// recomputed from the board camera's zoom instead of being set once.
fn scale_trail_gizmos(
    cameras: Query<(&Camera, &Projection), With<BoardCamera>>,
    mut gizmo_config: ResMut<GizmoConfigStore>,
) {
    let Ok((camera, Projection::Orthographic(ortho))) = cameras.single() else {
        return;
    };
    let Some(viewport) = camera.physical_viewport_size() else {
        return;
    };
    if ortho.area.height() <= 0.0 {
        return;
    }
    let pixels_per_unit = viewport.y as f32 / ortho.area.height();
    let width = |fraction: f32| {
        let (min, max) = TRAIL_WIDTH_RANGE;
        ((CELL - GAP) * fraction * pixels_per_unit).clamp(min, max)
    };

    // Written only on a change: `config_mut` marks the whole store dirty.
    let (core, halo) = (width(TRAIL_WIDTH), width(TRAIL_GLOW_WIDTH));
    if gizmo_config.config::<TrailGizmos>().0.line.width != core {
        gizmo_config.config_mut::<TrailGizmos>().0.line.width = core;
    }
    if gizmo_config.config::<TrailGlowGizmos>().0.line.width != halo {
        gizmo_config.config_mut::<TrailGlowGizmos>().0.line.width = halo;
    }
}

fn draw_hover(
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
fn update_cell_popup(
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
fn clamp_to_window(anchor: f32, size: f32, window: f32) -> f32 {
    (anchor + POPUP_OFFSET)
        .min(window - size - POPUP_MARGIN)
        .max(POPUP_MARGIN)
}

type HudTexts<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Text,
        Option<&'static HudText>,
        Has<PlayLabel>,
        Has<PlaybackStatus>,
        Has<SpeedLabel>,
    ),
>;

fn update_hud(
    stats: Res<Stats>,
    playing: Res<Playing>,
    timer: Res<StepTimer>,
    edit: Res<EditMode>,
    seed: Res<Seed>,
    mut texts: HudTexts,
) {
    if !(stats.is_changed()
        || playing.is_changed()
        || timer.is_changed()
        || edit.is_changed()
        || seed.is_changed())
    {
        return;
    }
    let state = if edit.enabled {
        "Editing (paused)"
    } else if stats.extinct {
        "Extinct"
    } else if playing.0 {
        "Running"
    } else {
        "Paused"
    };
    for (mut text, hud, play, status, speed) in &mut texts {
        let next = if let Some(hud) = hud {
            match hud.0 {
                0 => stats.aphids.to_string(),
                1 => stats.ladybugs.to_string(),
                2 => stats.food.to_string(),
                3 => format!("+{}", stats.births),
                4 => format!("−{}", stats.deaths),
                5 => seed.0.to_string(),
                index => format!("{:+}\nthis turn", stats.deltas[index - 6]),
            }
        } else if play {
            if playing.0 && !stats.extinct {
                "Pause".into()
            } else {
                "Play".into()
            }
        } else if status {
            format!("{state}  ·  Turn {}", stats.turn)
        } else if speed {
            format!("{:.2} turns/s", 1.0 / timer.0.duration().as_secs_f32())
        } else {
            continue;
        };
        if text.0 != next {
            text.0 = next;
        }
    }
}

/// Fills the toolbar's status line. The toolbar and the chart share a surface, so
/// the chart's contrast-checked green and red are the right pair here too.
fn update_status_text(
    status: Res<StatusLine>,
    mut labels: Query<(&mut Text, &mut TextColor), With<StatusLabel>>,
) {
    if !status.is_changed() {
        return;
    }
    let tint = if status.success {
        CHART_APHID
    } else {
        CHART_LADYBUG
    };
    for (text, mut colour) in &mut labels {
        set_text(Some(text), status.message.clone());
        if colour.0 != tint {
            colour.0 = tint;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

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
                        offset.min_element() - radius >= 0.0
                            && offset.max_element() + radius <= 1.0,
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
}
