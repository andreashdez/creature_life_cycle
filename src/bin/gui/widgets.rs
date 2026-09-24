//! Small UI building blocks shared by the sidebars and the chart.

use crate::chart::INK_PRIMARY;
use crate::run_control::RunAction;
use bevy::feathers::constants::fonts;
use bevy::feathers::controls::FeathersButton;
use bevy::feathers::theme::ThemedText;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

/// Shared explicit typography prevents unstyled text falling back to Bevy's
/// larger default font. Feathers controls keep their own inherited styles.
pub fn ui_text(text: impl Into<String>, size: f32) -> impl Scene {
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

pub fn run_button(caption: &'static str, action: RunAction) -> impl Scene {
    bsn! {
        @FeathersButton
        on(move |_: On<Activate>, mut actions: MessageWriter<RunAction>| { actions.write(action); })
        Children [(Text(caption) ThemedText)]
    }
}

pub fn set_text(text: Option<Mut<Text>>, value: String) {
    if let Some(mut text) = text
        && text.0 != value
    {
        text.0 = value;
    }
}
