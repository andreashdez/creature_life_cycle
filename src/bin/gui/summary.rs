//! The summary sidebar on the left: population cards, playback controls, and
//! the readouts they share with the panel.

use crate::chart::{CHART_APHID, CHART_LADYBUG, INK_SECONDARY, TOOLTIP_BG};
use crate::creatures::CreatureAssets;
use crate::editing::EditMode;
use crate::layout::SUMMARY_WIDTH;
use crate::panel::{PropertiesToggle, PropertiesToggleLabel, toggle_properties};
use crate::run_control::{RunAction, StatusLine};
use crate::simulation::{Playing, Seed, Stats, StepTimer};
use crate::widgets::{run_button, set_text, ui_text};
use bevy::feathers::controls::{ButtonVariant, FeathersButton};
use bevy::feathers::display::label_small;
use bevy::feathers::theme::{ThemeBackgroundColor, ThemedText};
use bevy::feathers::tokens;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

#[derive(Component, Clone, Default, FromTemplate)]
pub struct HudText(pub usize);

#[derive(Component, Clone, Default, FromTemplate)]
pub struct PlayLabel;

#[derive(Component, Clone, Default, FromTemplate)]
pub struct PlaybackStatus;

#[derive(Component, Clone, Default, FromTemplate)]
pub struct SpeedLabel;

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

pub type HudTexts<'w, 's> = Query<
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

pub fn update_hud(
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
