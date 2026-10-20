//! The nine tunable probabilities: the parameters tab's numeric fields,
//! and pushing their values into the board.

use crate::history::History;
use crate::run_control::StatusLine;
use crate::simulation::{BoardRes, Stats};
use crate::widgets::ui_text;
use bevy::feathers::controls::{
    ButtonVariant, FeathersButton, FeathersNumberInput, HardLimit, NumberInputPrecision,
    NumberInputStep, NumberInputValue, SoftLimit,
};
use bevy::feathers::display::label_small;
use bevy::feathers::theme::ThemedText;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui_widgets::{Activate, ValueChange};
use creature_life_cycle::{AphidParams, Board, FoodParams, LadybugParams};

/// The nine tunable probabilities, owned by the panel and pushed into the board
/// when they change. Indices follow the parameters tab from top to bottom, the
/// order of `PARAM_LABELS`. Anything else in the params structs, such as the
/// ladybug prey life gain, is carried through unchanged.
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

/// Identifies explicit step controls for interaction probes.
#[derive(Component, Clone, Default, FromTemplate)]
pub struct ProbStep(pub usize, pub f64);

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

/// One percentage field supports typing and dragging, with explicit step buttons.
pub fn param_row(index: usize, value: f64) -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column, row_gap: px(6),
            margin: UiRect::bottom(px(18)), flex_shrink: 0.0,
        }
        Children [
            @ui_text(PARAM_LABELS[index], 14.0) ParamCaption({index})
            --
            Node { column_gap: px(6), align_items: AlignItems::Center }
            Children [
                @FeathersNumberInput
                ProbField({index})
                NumberInputValue::F64({value * 100.0})
                HardLimit::f64(0.0..=100.0)
                SoftLimit::f64(0.0..=100.0)
                NumberInputPrecision(1)
                NumberInputStep(0.1)
                Node {
                    flex_grow: 1.0, flex_basis: px(0), min_width: px(0),
                }
                on(on_number_changed)
                --
                Node { flex_direction: FlexDirection::Column, row_gap: px(2), flex_shrink: 0.0 }
                Children [
                    @percentage_step(index, 0.1, "+")
                    --
                    @percentage_step(index, -0.1, "−")
                ]
                --
                @label_small("%")
            ]
            --
            @label_small(PARAM_HELP[index])
        ]
    }
}

/// Explicit step buttons complement scrubbing: the native number input's
/// hover chevrons are visual hints, not clickable increment/decrement controls.
fn percentage_step(index: usize, delta: f64, caption: &'static str) -> impl Scene {
    bsn! {
        @FeathersButton { @variant: ButtonVariant::Plain }
        ProbStep({index}, {delta})
        Node {
            width: px(20), min_width: px(20), height: px(16), min_height: px(16),
            padding: UiRect::ZERO,
        }
        on(move |_: On<Activate>, fields: Query<(Entity, &ProbField)>, params: Res<Params>, mut commands: Commands| {
            if let Some((source, _)) = fields.iter().find(|(_, field)| field.0 == index) {
                let value = ((params.get(index) * 100.0 + delta) * 10.0).round() / 10.0;
                commands.trigger(ValueChange { source, value: value.clamp(0.0, 100.0), is_final: true });
            }
        })
        Children [@ui_text(caption, 12.0)]
    }
}

pub fn param_group(title: &'static str, start: usize, end: usize) -> impl Scene {
    bsn! {
        Node { margin: UiRect::vertical(px(10)), align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween }
        Children [
            @ui_text(title, 16.0)
            --
            @FeathersButton { @variant: ButtonVariant::Plain }
            on(move |_: On<Activate>, mut resets: MessageWriter<ParameterReset>| {
                resets.write(ParameterReset(start, end));
            })
            Children [Text("Restore defaults") ThemedText]
        ]
    }
}

/// Reject invalid drafts before they can change the simulation.
pub fn parse_percentage(text: &str) -> Option<f64> {
    let value = text.trim().parse::<f64>().ok()?;
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value / 100.0)
}

/// Preserve the full value when a number field is focused or committed unchanged.
pub fn percentage_text(value: f64) -> String {
    (value * 100.0).to_string()
}

/// Keep the app's reject-and-restore policy for invalid drafts (Feathers otherwise
/// clamps them or leaves unparsable text visible). Commit blur before simulation
/// and Save run, and let Restore defaults override a pending edit.
pub fn commit_parameter_fields(
    (keys, focus): (Res<ButtonInput<KeyCode>>, Res<InputFocus>),
    mut previous: Local<Option<Entity>>,
    fields: Query<(&ProbField, &NumberInputValue)>,
    mut texts: Query<(&ChildOf, &mut EditableText)>,
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
        && let Ok((parent, mut text)) = texts.get_mut(entity)
        && let Ok((field, current)) = fields.get(parent.parent())
    {
        let typed = text.value().to_string();
        if let Some(value) = parse_percentage(&typed) {
            // An unchanged draft must not overwrite a step-button
            // update that happened while the field was losing focus.
            if typed.trim().parse::<f64>().ok().map(NumberInputValue::F64) != Some(*current) {
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
        // FocusLost runs later, in PostUpdate. Replace the draft now so its
        // native number-input observer cannot put it back after this reset.
        for (parent, mut text) in &mut texts {
            if let Ok((field, _)) = fields.get(parent.parent())
                && (reset.0..reset.1).contains(&field.0)
            {
                text.editor.set_text(&percentage_text(params.get(field.0)));
            }
        }
    }
}

/// Reflect changes from fields, resets, and saves without rewriting drafts.
pub fn sync_parameter_widgets(
    mut commands: Commands,
    (params, saved): (Res<Params>, Res<SavedParams>),
    fields: Query<(Entity, &ProbField, &NumberInputValue)>,
    mut captions: Query<(&ParamCaption, &mut Text)>,
) {
    if params.is_changed() {
        for (entity, field, value) in &fields {
            let next = NumberInputValue::F64(params.get(field.0) * 100.0);
            if *value != next {
                commands.entity(entity).insert(next);
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

/// Accept native typing, scrubbing, and chevron events while keeping invalid
/// drafts from being silently clamped. Reinsert even an unchanged value on a
/// final event so the native field restores its canonical text.
fn on_number_changed(
    change: On<ValueChange<f64>>,
    mut commands: Commands,
    fields: Query<(&ProbField, &Children)>,
    mut texts: Query<&mut EditableText>,
    mut params: ResMut<Params>,
    mut status: ResMut<StatusLine>,
) {
    let Ok((field, children)) = fields.get(change.source) else {
        return;
    };
    let invalid_draft = change.is_final
        && children.iter().any(|child| {
            texts
                .get(child)
                .is_ok_and(|text| parse_percentage(&text.value().to_string()).is_none())
        });
    if invalid_draft || !change.value.is_finite() || !(0.0..=100.0).contains(&change.value) {
        status.set("Enter a percentage from 0 to 100. Value unchanged.", false);
    } else if change.value != params.get(field.0) * 100.0 {
        params.set(field.0, change.value / 100.0);
        if change.is_final {
            status.set(
                format!("{} set to {}%.", PARAM_LABELS[field.0], change.value),
                true,
            );
        }
    }
    // FocusLost can follow the button's activation later in this frame. Keep
    // its draft in sync immediately so it cannot undo the step just applied.
    for child in children.iter() {
        if let Ok(mut text) = texts.get_mut(child) {
            text.editor.set_text(&percentage_text(params.get(field.0)));
        }
    }
    commands
        .entity(change.source)
        .insert(NumberInputValue::F64(params.get(field.0) * 100.0));
}

/// Pushes edited parameters into the board. Deliberately does not bump
/// `Revision`, so changing a probability never triggers a full board repaint.
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
