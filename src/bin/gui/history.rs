//! The rolling population history behind the chart, and the rule changes
//! and extinctions marked on it.

use crate::chart::{CHART_SERIES_NAMES, ChartPanel};
use crate::params::percentage_text;
use crate::widgets::ui_text;
use bevy::ecs::system::SystemParam;
use bevy::feathers::controls::{ButtonVariant, FeathersButton, FeathersListRow, FeathersListView};
use bevy::feathers::focus::FocusIndicator;
use bevy::feathers::theme::ThemedText;
use bevy::prelude::*;
use bevy::scene::Ready;
use bevy::ui::{InteractionDisabled, Selected};
use bevy::ui_widgets::{Activate, ActiveDescendant, ReselectListRow, ScrollArea, ValueChange};
use std::collections::VecDeque;

/// Turns kept in the history. Older turns scroll off the chart, and their
/// events expire with them.
pub const HISTORY_LIMIT: usize = 240;

#[derive(Clone, Copy)]
pub struct HistoryPoint {
    pub turn: usize,
    pub aphids: usize,
    pub ladybugs: usize,
}

impl HistoryPoint {
    pub fn series(&self, index: usize) -> usize {
        if index == 0 {
            self.aphids
        } else {
            self.ladybugs
        }
    }
}

/// Rolling population history, capped at `HISTORY_LIMIT` turns, with the
/// rule-change and extinction events that fall inside it.
#[derive(Resource, Default, Clone)]
pub struct History(pub VecDeque<HistoryPoint>, pub VecDeque<HistoryEvent>);

#[derive(Clone, Default, PartialEq)]
pub struct HistoryEvent {
    pub turn: usize,
    pub changes: [Option<(f64, f64)>; 9],
    pub extinctions: [bool; 2],
}

impl HistoryEvent {
    /// Full event details disambiguate repeated parameter names and show both
    /// values. Rules apply after the named turn; extinction happens on it.
    pub fn details(&self) -> String {
        const NAMES: [&str; 9] = [
            "Aphid movement",
            "Aphid kill chance",
            "Aphid accomplice bonus",
            "Aphid reproduction",
            "Ladybug movement",
            "Ladybug kill chance",
            "Ladybug direction change",
            "Ladybug reproduction",
            "Food regeneration",
        ];
        let mut lines = vec![format!("Turn {}", self.turn)];
        for (index, change) in self.changes.iter().enumerate() {
            if let Some((from, to)) = change {
                lines.push(format!(
                    "{}: {}% → {}%",
                    NAMES[index],
                    percentage_text(*from),
                    percentage_text(*to)
                ));
            }
        }
        for (index, extinct) in self.extinctions.iter().enumerate() {
            if *extinct {
                lines.push(format!("{} became extinct", ["Aphids", "Ladybugs"][index]));
            }
        }
        lines.join("\n")
    }

    pub fn has_rules(&self) -> bool {
        self.changes.iter().any(Option::is_some)
    }

    pub fn caption(&self) -> String {
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
    pub fn record(&mut self, turn: usize, aphids: usize, ladybugs: usize) {
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

    pub fn event(&mut self, turn: usize) -> &mut HistoryEvent {
        if self.1.back().is_none_or(|event| event.turn != turn) {
            self.1.push_back(HistoryEvent { turn, ..default() });
        }
        self.1.back_mut().unwrap()
    }
}

/// A selected event is identified by turn, never by its shifting deque index.
/// Reset, edits, and Undo clear it; expiry or reverting the event clears it too.
#[derive(Resource, Default, PartialEq)]
pub struct HistorySelection(pub Option<usize>);

impl HistorySelection {
    /// Resolve only events that still have a population sample to inspect.
    pub fn index(&self, history: &History) -> Option<usize> {
        let turn = self.0?;
        history
            .1
            .iter()
            .any(|event| event.turn == turn)
            .then_some(())?;
        history.0.iter().position(|point| point.turn == turn)
    }
}

/// The native list keeps keyboard focus while its rows are updated in place.
#[derive(Component, Clone, Default)]
pub struct EventList;

/// The scrolling child created by Feathers, where dynamic event rows belong.
#[derive(Component)]
pub struct EventListBody;

/// Retaining the event lets the list update text without replacing a focused row.
#[derive(Component)]
pub struct EventRow(pub HistoryEvent);

#[derive(Component, Clone, Default)]
pub struct ClearHistoryPin;

#[derive(Component, Clone, Default)]
struct EventListEmpty;

/// A bounded list in Overview makes older events reachable without pushing
/// the rest of the properties panel down as the simulation runs.
pub fn event_list() -> impl Scene {
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: px(8), flex_shrink: 0.0 }
        Children [
            Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center }
            Children [
                @ui_text("Events", 16.0)
                --
                @FeathersButton { @variant: ButtonVariant::Plain }
                ClearHistoryPin InteractionDisabled
                on(clear_history_pin)
                Children [Text("Clear pin") ThemedText]
            ]
            --
            @ui_text("Newest first · last 240 turns. Select to pin the chart; rule changes apply after the named turn.", 12.0)
            --
            @ui_text("No rule changes or extinctions yet.", 13.0)
            EventListEmpty
            --
            @FeathersListView
            EventList FocusIndicator
            Node { height: px(200), min_height: px(200), flex_shrink: 0.0, display: Display::None }
            on(init_event_list)
            on(select_history_event)
            on(reselect_history_event)
        ]
    }
}

/// Mark the native scrolling child once its hierarchy exists. Its viewport
/// must shrink within the fixed-height list instead of growing with its rows.
fn init_event_list(
    ready: On<Ready>,
    children: Query<&Children>,
    mut bodies: Query<&mut Node, With<ScrollArea>>,
    mut commands: Commands,
) {
    if let Ok(children) = children.get(ready.entity) {
        for child in children.iter() {
            if let Ok(mut node) = bodies.get_mut(child) {
                node.min_height = px(0);
                node.flex_grow = 1.0;
                commands.entity(child).insert(EventListBody);
            }
        }
    }
}

/// Route native pointer and Enter/Space requests through the same pin handler.
fn select_history_event(change: On<ValueChange<Entity>>, mut commands: Commands) {
    commands.trigger(PinHistoryEvent {
        list: change.source,
        row: change.value,
    });
}

/// Reselecting an event also reveals a chart that has since been collapsed.
fn reselect_history_event(change: On<ReselectListRow>, mut commands: Commands) {
    commands.trigger(PinHistoryEvent {
        list: change.source,
        row: change.row,
    });
}

#[derive(EntityEvent)]
pub struct PinHistoryEvent {
    #[event_target]
    pub list: Entity,
    pub row: Entity,
}

/// Validate a row against current history before pinning it. Selection reveals
/// the chart, but never pauses, rewinds, or otherwise changes the simulation.
pub fn pin_history_event(
    change: On<PinHistoryEvent>,
    lists: Query<(), With<EventList>>,
    rows: Query<&EventRow>,
    parents: Query<&ChildOf>,
    history: Res<History>,
    mut selection: ResMut<HistorySelection>,
    mut chart: ResMut<ChartPanel>,
) {
    if lists.contains(change.list)
        && parents
            .iter_ancestors(change.row)
            .any(|parent| parent == change.list)
        && let Ok(row) = rows.get(change.row)
    {
        let next = HistorySelection(Some(row.0.turn));
        if next.index(&history).is_some() {
            selection.set_if_neq(next);
            chart.collapsed = false;
        }
    }
}

/// Return chart inspection to the pointer without discarding any history.
fn clear_history_pin(_: On<Activate>, mut selection: ResMut<HistorySelection>) {
    selection.0 = None;
}

type EventListVisibility<'w, 's> = Query<
    'w,
    's,
    (&'static mut Node, Has<EventListEmpty>),
    Or<(With<EventList>, With<EventListEmpty>)>,
>;

/// Queries for the list's hierarchy and empty/selected states stay together so
/// synchronization can preserve row identity and keyboard navigation.
#[derive(SystemParam)]
pub struct EventListUi<'w, 's> {
    bodies: Query<'w, 's, (Entity, Option<&'static Children>), With<EventListBody>>,
    lists: Query<'w, 's, (Entity, &'static ActiveDescendant), With<EventList>>,
    rows: Query<'w, 's, (Entity, &'static EventRow, &'static Children, Has<Selected>)>,
    visibility: EventListVisibility<'w, 's>,
    clear: Query<'w, 's, Entity, With<ClearHistoryPin>>,
}

/// Update only changed rows, retain the navigation target, and prune pins
/// before chart hover is resolved against the settled history for this frame.
pub fn sync_event_list(
    mut commands: Commands,
    (history, mut selection): (Res<History>, ResMut<HistorySelection>),
    mut ui: EventListUi,
    mut texts: Query<&mut Text>,
) {
    if !(history.is_changed() || selection.is_changed()) {
        return;
    }
    if selection.0.is_some() && selection.index(&history).is_none() {
        selection.0 = None;
    }
    for (mut node, empty) in &mut ui.visibility {
        node.display = if history.1.is_empty() == empty {
            Display::Flex
        } else {
            Display::None
        };
    }
    for entity in &ui.clear {
        if selection.0.is_some() {
            commands.entity(entity).remove::<InteractionDisabled>();
        } else {
            commands.entity(entity).insert(InteractionDisabled);
        }
    }
    let Ok((body, old_children)) = ui.bodies.single() else {
        return;
    };
    let existing: std::collections::BTreeMap<_, _> = ui
        .rows
        .iter()
        .map(|(entity, row, children, selected)| (row.0.turn, (entity, row, children, selected)))
        .collect();
    let mut ordered = Vec::with_capacity(history.1.len());
    for event in history.1.iter().rev() {
        let selected = selection.0 == Some(event.turn);
        let entity = if let Some(&(entity, row, children, was_selected)) = existing.get(&event.turn)
        {
            if row.0 != *event {
                for child in children.iter() {
                    if let Ok(mut text) = texts.get_mut(child) {
                        text.0 = event.details();
                    }
                }
                commands.entity(entity).insert(EventRow(event.clone()));
            }
            if selected != was_selected {
                if selected {
                    commands.entity(entity).insert(Selected);
                } else {
                    commands.entity(entity).remove::<Selected>();
                }
            }
            entity
        } else {
            let entity = commands.spawn_scene(bsn! {
                @FeathersListRow
                Node { flex_shrink: 0.0, padding: UiRect::all(px(8)), align_items: AlignItems::Start }
                // Primary ink passes 4.5:1 on the native normal, hovered, and
                // selected row surfaces; the palette test guards all three.
                Children [@ui_text(event.details(), 13.0) Node { min_width: px(0), flex_grow: 1.0 }]
            }).insert(EventRow(event.clone())).id();
            if selected {
                commands.entity(entity).insert(Selected);
            }
            entity
        };
        ordered.push(entity);
    }
    for (entity, _, _, _) in existing.values() {
        if !ordered.contains(entity) {
            commands.entity(*entity).despawn();
        }
    }
    for (entity, active) in &ui.lists {
        if active.0.is_some_and(|row| !ordered.contains(&row)) {
            commands
                .entity(entity)
                .insert(ActiveDescendant(ordered.first().copied()));
        }
    }
    if old_children.is_none_or(|children| children.iter().ne(ordered.iter().copied())) {
        commands.entity(body).replace_children(&ordered);
    }
}
