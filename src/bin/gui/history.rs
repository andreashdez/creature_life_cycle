//! The rolling population history behind the chart, and the rule changes
//! and extinctions marked on it.

use crate::chart::CHART_SERIES_NAMES;
use bevy::prelude::*;
use std::collections::VecDeque;

/// Turns kept in the history, matching `HISTORY_LIMIT` in the previous macroquad GUI.
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

/// Rolling population history, capped like the previous macroquad GUI's.
#[derive(Resource, Default, Clone)]
pub struct History(pub VecDeque<HistoryPoint>, pub VecDeque<HistoryEvent>);

#[derive(Clone, Default)]
pub struct HistoryEvent {
    pub turn: usize,
    pub changes: [Option<(f64, f64)>; 9],
    pub extinctions: [bool; 2],
}

impl HistoryEvent {
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
