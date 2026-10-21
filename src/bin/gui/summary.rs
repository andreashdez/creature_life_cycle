//! The summary sidebar on the left: population cards, playback controls, and
//! the readouts they share with the panel.

use crate::chart::{CHART_APHID, CHART_LADYBUG, INK_SECONDARY, TOOLTIP_BG};
use crate::creatures::CreatureAssets;
use crate::editing::EditMode;
use crate::layout::SUMMARY_WIDTH;
use crate::panel::{PropertiesToggle, PropertiesToggleLabel, toggle_properties};
use crate::run_control::{RunAction, StatusLine};
use crate::simulation::{Playing, STEP_SECONDS, Seed, Stats, StepTimer};
use crate::widgets::{run_button, set_text, ui_text};
use bevy::feathers::controls::{ButtonVariant, FeathersButton, FeathersListRow, FeathersSelect};
use bevy::feathers::display::{caption, label, label_small};
use bevy::feathers::theme::{ThemeBackgroundColor, ThemedText};
use bevy::feathers::tokens;
use bevy::prelude::*;
use bevy::scene::SceneList;
use bevy::ui::Selected;
use bevy::ui_widgets::{Activate, ValueChange};

#[derive(Component, Clone, Default, FromTemplate)]
pub struct HudText(pub usize);

#[derive(Component, Clone, Default, FromTemplate)]
pub struct PlayLabel;

#[derive(Component, Clone, Default, FromTemplate)]
pub struct PlaybackStatus;

/// Seconds per turn, ordered slowest to fastest, including the launch default.
/// The `[` and `]` shortcuts step through these, so the speed is always one of
/// them and the select always has a row to show.
pub const SPEED_PRESETS: [f32; 7] = [4.0, 2.0, 1.0, STEP_SECONDS, 0.2, 0.1, 0.05];

/// The preset one step faster or slower than `seconds`, or `seconds` itself
/// at either end.
pub fn adjacent_speed(seconds: f32, faster: bool) -> f32 {
    let step = if faster {
        SPEED_PRESETS
            .into_iter()
            .find(|preset| *preset < seconds - 1e-6)
    } else {
        SPEED_PRESETS
            .into_iter()
            .rev()
            .find(|preset| *preset > seconds + 1e-6)
    };
    step.unwrap_or(seconds)
}

#[derive(Component, Clone, Default, FromTemplate)]
pub struct SpeedSelect;

/// The interval a speed row stands for.
#[derive(Component, Clone, Default, FromTemplate)]
pub struct SpeedPreset(pub f32);

/// A row's caption, which the select also shows once the row is selected.
pub fn speed_caption(seconds: f32) -> String {
    let rate = format!("{:.2}", 1.0 / seconds);
    let rate = rate.trim_end_matches('0').trim_end_matches('.');
    let default_label = if seconds == STEP_SECONDS {
        " (default)"
    } else {
        ""
    };
    format!("{rate} turns/s{default_label}")
}

fn speed_rows() -> Box<dyn SceneList> {
    Box::new(
        SPEED_PRESETS
            .into_iter()
            .map(|seconds| -> Box<dyn SceneList> {
                bsn! {
                    @FeathersListRow
                    SpeedPreset(seconds)
                    Children [@caption(speed_caption(seconds))]
                }
                .into()
            })
            .collect::<Vec<_>>(),
    )
}

/// A native select supplies the popup, keyboard navigation, dismissal, focus
/// restoration, and a caption that follows the selected row.
pub fn speed_select() -> impl Scene {
    bsn! {
        Node { width: percent(100), column_gap: px(8), align_items: AlignItems::Center, flex_shrink: 0.0 }
        Children [
            @label("Speed")
            --
            @FeathersSelect { @options: {speed_rows()} }
            SpeedSelect
            Node { flex_grow: 1.0 }
            on(|change: On<ValueChange<Entity>>, presets: Query<&SpeedPreset>, mut actions: MessageWriter<RunAction>| {
                if let Ok(preset) = presets.get(change.value) {
                    actions.write(RunAction::SetSpeed(preset.0));
                }
            })
        ]
    }
}

/// Selects the row for the current speed, which the shortcuts change too. This
/// sets `Selected` directly rather than through `SetSelected`, whose
/// `ValueChange` would send the speed back as a request and restart the timer.
pub fn sync_speed_select(
    mut commands: Commands,
    timer: Res<StepTimer>,
    rows: Query<(Entity, &SpeedPreset, Has<Selected>)>,
) {
    let current = timer.0.duration();
    for (entity, preset, selected) in &rows {
        let wanted = current == std::time::Duration::from_secs_f32(preset.0);
        if wanted && !selected {
            commands.entity(entity).insert(Selected);
        } else if !wanted && selected {
            commands.entity(entity).remove::<Selected>();
        }
    }
}

#[derive(Component, Clone, Default, FromTemplate)]
pub struct StatusLabel;

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
            {artwork.map(|image| bsn_list! {
                Node { width: px(32), height: px(32), flex_shrink: 0.0 }
                ImageNode { image: {image} }
            })}
            --
            Node { flex_direction: FlexDirection::Column, row_gap: px(2), min_width: px(0) }
            Children [
                @label_small(caption)
                --
                @ui_text("0", 24.0) HudText({index})
            ]
            --
            Node { flex_grow: 1.0 }
            --
            @ui_text("0", 12.0) HudText({index + 6}) TextColor(INK_SECONDARY)
        ]
    }
}

pub fn spawn_summary(
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
            @ui_text("Aphids & Ladybugs", 21.0)
            --
            @population_card("Aphids", 0, Some(assets.aphid.clone()))
            --
            @population_card("Ladybugs", 1, Some(assets.ladybug.clone()))
            --
            @population_card("Food available", 2, Some(food_icon))
            --
            Node { column_gap: px(8), align_items: AlignItems::Center }
            Children [
                @FeathersButton { @variant: ButtonVariant::Primary }
                Node { flex_grow: 1.0, min_width: px(72) }
                on(|_: On<Activate>, mut actions: MessageWriter<RunAction>| { actions.write(RunAction::TogglePlay); })
                Children [Text("Pause") ThemedText PlayLabel]
                --
                @run_button("Step", RunAction::Step)
                --
                @run_button("Reset", RunAction::Reset)
            ]
            --
            @speed_select()
            --
            @ui_text("", 14.0) PlaybackStatus
            --
            @run_button("Fit board", RunAction::Fit)
            --
            @ui_text("", 14.0) StatusLabel
            --
            Node { flex_grow: 1.0 }
            --
            @FeathersButton
            PropertiesToggle
            on(toggle_properties)
            Children [Text("Show properties") ThemedText PropertiesToggleLabel]
        ]
    }).insert(UiTargetCamera(camera));
}

pub type HudTexts<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Text,
        Option<&'static HudText>,
        Has<PlayLabel>,
        Has<PlaybackStatus>,
    ),
>;

pub fn update_hud(
    stats: Res<Stats>,
    playing: Res<Playing>,
    edit: Res<EditMode>,
    seed: Res<Seed>,
    mut texts: HudTexts,
) {
    if !(stats.is_changed() || playing.is_changed() || edit.is_changed() || seed.is_changed()) {
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
    for (mut text, hud, play, status) in &mut texts {
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
pub fn update_status_text(
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
