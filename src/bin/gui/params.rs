//! The nine tunable probabilities: the parameters tab's sliders and fields,
//! and pushing their values into the board.

use crate::history::History;
use crate::run_control::StatusLine;
use crate::simulation::{BoardRes, Stats};
use crate::widgets::ui_text;
use bevy::feathers::controls::{
    ButtonVariant, FeathersButton, FeathersSlider, FeathersTextInput, FeathersTextInputContainer,
};
use bevy::feathers::display::label_small;
use bevy::feathers::theme::ThemedText;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::{EditableText, EditableTextFilter};
use bevy::ui_widgets::{Activate, SliderPrecision, SliderValue, ValueChange};
use creature_life_cycle::{AphidParams, Board, FoodParams, LadybugParams};

/// The nine tunable probabilities, owned by the panel and pushed into the board
/// when they change. Indices match `probability`/`set_probability` in
/// the previous macroquad GUI's parameter panel, so saved configs line up.
#[derive(Resource, Clone, Copy, Default)]
pub struct Params {
    pub aphid: AphidParams,
    pub ladybug: LadybugParams,
    pub food: FoodParams,
}

#[derive(Resource)]
pub struct SavedParams(pub Params);

#[derive(Component, Clone, Default, FromTemplate)]
pub struct ProbField(pub usize);

#[derive(Component, Clone, Default, FromTemplate)]
pub struct ParamCaption(pub usize);

#[derive(Message)]
pub struct ParameterReset(pub usize, pub usize);

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
    pub fn get(&self, index: usize) -> f64 {
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

    pub fn set(&mut self, index: usize, value: f64) {
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

/// Tags a slider with the parameter index it edits, so one observer can serve
/// all nine rather than nine closures capturing nine fields.
#[derive(Component, Clone, Copy, FromTemplate)]
pub struct ProbSlider(pub usize);

/// A slider for exploration and an exact percentage field for deliberate changes.
pub fn param_row(index: usize, value: f64) -> impl Scene {
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

pub fn param_group(title: &'static str, start: usize, end: usize) -> impl Scene {
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

pub fn parse_percentage(text: &str) -> Option<f64> {
    let value = text.trim().parse::<f64>().ok()?;
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value / 100.0)
}

pub fn percentage_text(value: f64) -> String {
    // Stable decimal percentages; merely focusing a field must not round saved settings.
    format!("{:.6}", value * 100.0)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

pub fn commit_parameter_fields(
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

pub fn sync_parameter_widgets(
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
pub fn apply_params(
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

pub fn apply_parameter_values(
    params: &Params,
    board: &mut Board,
    history: &mut History,
    turn: usize,
) {
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
