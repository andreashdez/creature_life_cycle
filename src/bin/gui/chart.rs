//! The population history chart under the board: its own camera and gizmo
//! lines for the marks, a `bevy_ui` overlay for the text, and the palette
//! both are checked against.

use crate::history::{History, HistoryEvent};
use crate::layout::SUMMARY_WIDTH;
use crate::panel::PropertiesPanel;
use crate::probe::ProbeHover;
use crate::widgets::set_text;
use bevy::feathers::controls::FeathersButton;
use bevy::feathers::display::label_small;
use bevy::feathers::theme::ThemedText;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

/// Height of the population history strip under the board, in logical pixels.
pub const CHART_HEIGHT: f32 = 220.0;

// Plot insets inside the strip: tick gutter left, end labels right, title and
// legend on top, x ticks below.
const PLOT_LEFT: f32 = 52.0;
const PLOT_RIGHT: f32 = 96.0;
const PLOT_TOP: f32 = 68.0;
const PLOT_BOTTOM: f32 = 34.0;
const TOOLTIP_WIDTH: f32 = 240.0;
pub const CHART_COLLAPSED_HEIGHT: f32 = 40.0;
pub const CHART_MIN_HEIGHT: f32 = 180.0;

const EVENT_RULES: Color = Color::srgb_u8(190, 173, 115);

// Chart palette. The series are the board's aphid and ladybug hues, stepped
// down into the dark-surface lightness band (OKLCH L 0.48-0.67, hue held)
// because the board's own steps fail that band on this surface. Checked with
// the dataviz palette validator against `CHART_SURFACE`: every check passes,
// worst deuteranopia delta E 10.0, normal-vision delta E 28.6. The board keeps
// its lighter steps, which are tuned for its green cells rather than this gray.
pub const CHART_SURFACE: Color = Color::srgb_u8(0x1f, 0x1f, 0x24); // feathers WINDOW_BG
pub const CHART_APHID: Color = Color::srgb_u8(0x5b, 0xac, 0x43);
pub const CHART_LADYBUG: Color = Color::srgb_u8(0xc2, 0x44, 0x2f);
pub const CHART_SERIES: [Color; 2] = [CHART_APHID, CHART_LADYBUG];
pub const CHART_SERIES_NAMES: [&str; 2] = ["aphids", "ladybugs"];
// Text wears ink, never series colour. Contrast on the surface: primary 14.0,
// secondary 7.66 (feathers TEXT_MAIN), muted 4.64. Feathers' own TEXT_DIM is
// 4.34:1, just under WCAG's 4.5:1 for normal text, hence the lighter muted.
pub const INK_PRIMARY: Color = Color::srgb_u8(0xed, 0xed, 0xee);
pub const INK_SECONDARY: Color = Color::srgb_u8(0xb1, 0xb1, 0xb2);
const INK_MUTED: Color = Color::srgb_u8(0x88, 0x88, 0x8b);
pub const GRIDLINE: Color = Color::srgb_u8(0x36, 0x37, 0x3b); // feathers GRAY_2
const BASELINE: Color = Color::srgb_u8(0x46, 0x47, 0x4d); // feathers GRAY_3
pub const TOOLTIP_BG: Color = Color::srgb_u8(0x2a, 0x2a, 0x2e); // feathers GRAY_1

/// Logical width of the chart strip, set by `layout_viewports`.
#[derive(Resource, Default, PartialEq)]
pub struct ChartSize(pub f32, pub f32);

#[derive(Resource)]
pub struct ChartPanel {
    pub height: f32,
    pub collapsed: bool,
    pub dragging: bool,
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
    pub fn apply(&mut self, action: ChartAction, window_height: f32) {
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

    pub fn effective_height(&self, window_height: f32) -> f32 {
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
pub struct ChartRoot;

#[derive(Component)]
pub struct ChartEventLegend;

#[derive(Component)]
pub struct ChartExpandedOnly;

#[derive(Component, Clone, Default, FromTemplate)]
pub struct ChartToggleText;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ChartAction {
    #[default]
    Toggle,
    Smaller,
    Larger,
}

#[derive(Component, Clone, Default, FromTemplate)]
pub struct ChartControlButton(pub ChartAction);

#[derive(Component)]
pub struct ChartEmpty;

/// History index under the crosshair, if the pointer is over the plot.
#[derive(Resource, Default, PartialEq)]
pub struct ChartHover(pub Option<usize>);

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct ChartGridGizmos;

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct ChartSeriesGizmos;

/// End-of-line marker (both the dot and its surface ring) for one series.
#[derive(Component)]
pub struct EndDot(pub usize);

/// Every positioned piece of chart text, so one query can lay them all out.
#[derive(Component, Clone, Copy)]
pub enum ChartLabel {
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

pub fn chart_resize_input(
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

pub type ChartNodes<'w, 's> = Query<
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

pub fn sync_chart_panel(
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

/// Plot area inside the strip, in strip-local logical pixels (origin top-left,
/// y down), shared by the marks and the text overlay so they cannot disagree.
pub fn plot_rect(width: f32, height: f32) -> Rect {
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

pub fn history_x(index: usize, len: usize, plot: Rect) -> f32 {
    if len <= 1 {
        plot.min.x
    } else {
        plot.min.x + plot.width() * index as f32 / (len - 1) as f32
    }
}

/// Snaps a pointer x to the closest recorded turn. Readers aim at a turn, not
/// at a 2px line.
pub fn nearest_index(x: f32, len: usize, plot: Rect) -> usize {
    if len <= 1 {
        return 0;
    }
    let t = ((x - plot.min.x) / plot.width()).clamp(0.0, 1.0);
    (t * (len - 1) as f32).round() as usize
}

pub fn value_y(value: usize, top: usize, plot: Rect) -> f32 {
    plot.max.y - plot.height() * value as f32 / top.max(1) as f32
}

/// Strip-local point to chart-camera world space (origin centre, y up).
fn chart_world(point: Vec2, width: f32, height: f32) -> Vec2 {
    Vec2::new(point.x - width * 0.5, height * 0.5 - point.y)
}

/// A clean y-axis for populations up to `max`: returns `(top, step)` with a
/// 1/2/5 x 10^k step and at most four intervals.
pub fn nice_axis(max: usize) -> (usize, usize) {
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

pub fn spawn_chart_overlay(commands: &mut Commands, ui_camera: Entity, font: Handle<Font>) {
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

pub fn chart_hover(
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

pub fn draw_chart_marks(
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

pub const DASH: f32 = 7.0;
pub const DASH_PERIOD: f32 = 12.0;

/// Splits a polyline into dash pieces with the dash phase carried across
/// vertices, so the pattern stays even however short the per-turn segments
/// get. Gizmos' own `Dashed` style restarts the pattern on every segment, which
/// draws a full 240-turn history (segments shorter than one dash) as solid.
pub fn dash_segments(points: &[Vec2]) -> Vec<(Vec2, Vec2)> {
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

pub fn update_chart_labels(
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
