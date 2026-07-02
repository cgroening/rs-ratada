//! Inline autocomplete dropdown for text fields.
//!
//! Holds a newest-first candidate list and, for the current query, the subset
//! that contains it (case-insensitive). A small state machine decides whether a
//! key navigates the dropdown, accepts a suggestion, hides it, or is left for
//! the caller. Rendering returns ready-made [`Line`]s the host can append.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::style;
use crate::theme::Palette;

/// Most suggestions shown at once.
const MAX_SUGGESTIONS: usize = 6;

/// What a key press means while the dropdown may be open.
pub enum AcOutcome {
    /// The user accepted this suggestion; the caller fills the field with it.
    Accepted(String),
    /// The highlight moved; the caller just redraws.
    Navigated,
    /// The dropdown was hidden; the caller just redraws.
    Closed,
    /// Not an autocomplete key (or the dropdown is closed); handle normally.
    Ignored,
}

/// Suggestion state for one text field.
pub struct Autocomplete {
    candidates: Vec<String>,
    matches: Vec<usize>,
    selected: Option<usize>,
    dismissed: bool,
    empty_query: bool,
}

impl Autocomplete {
    /// Builds the autocomplete over `candidates` (newest first). It stays
    /// hidden until the first non-empty [`refresh`](Self::refresh).
    pub fn new(candidates: Vec<String>) -> Self {
        Self {
            candidates,
            matches: Vec::new(),
            selected: None,
            dismissed: true,
            empty_query: true,
        }
    }

    /// Recomputes the matches for `query`: candidates that contain it
    /// (case-insensitive) but are not exactly it, newest first, capped.
    pub fn refresh(&mut self, query: &str) {
        self.dismissed = false;
        self.empty_query = query.trim().is_empty();
        let needle = query.trim().to_lowercase();
        self.matches.clear();
        if !self.empty_query {
            for (index, candidate) in self.candidates.iter().enumerate() {
                let lower = candidate.to_lowercase();
                if lower.contains(&needle) && lower != needle {
                    self.matches.push(index);
                }
                if self.matches.len() == MAX_SUGGESTIONS {
                    break;
                }
            }
        }
        self.selected = match self.selected {
            Some(index) if index < self.matches.len() => Some(index),
            _ => None,
        };
    }

    /// Whether the dropdown is currently shown.
    pub fn is_open(&self) -> bool {
        !self.dismissed && !self.empty_query && !self.matches.is_empty()
    }

    /// Interprets `key` against the dropdown state.
    pub fn on_key(&mut self, key: KeyEvent) -> AcOutcome {
        if !self.is_open() {
            return AcOutcome::Ignored;
        }
        match key.code {
            KeyCode::Esc => {
                self.dismissed = true;
                self.selected = None;
                AcOutcome::Closed
            }
            KeyCode::Down => {
                self.move_selection(1);
                AcOutcome::Navigated
            }
            KeyCode::Up => {
                self.move_selection(-1);
                AcOutcome::Navigated
            }
            KeyCode::Enter | KeyCode::Tab | KeyCode::Right => {
                match self.accepted() {
                    Some(value) => {
                        self.dismissed = true;
                        self.selected = None;
                        AcOutcome::Accepted(value)
                    }
                    None => AcOutcome::Ignored,
                }
            }
            _ => AcOutcome::Ignored,
        }
    }

    /// The rendered dropdown lines, each indented by `indent` columns; the
    /// highlighted row uses the selection style. Empty when closed.
    pub fn lines(
        &self,
        palette: &Palette,
        indent: usize,
        base: Style,
    ) -> Vec<Line<'static>> {
        if !self.is_open() {
            return Vec::new();
        }
        let pad = " ".repeat(indent);
        self.matches
            .iter()
            .enumerate()
            .map(|(row, &candidate)| {
                let row_style = if Some(row) == self.selected {
                    style::bg(palette.selection_bg)
                } else {
                    base
                };
                Line::from(vec![
                    Span::raw(pad.clone()),
                    Span::styled(self.candidates[candidate].clone(), row_style),
                ])
            })
            .collect()
    }

    fn move_selection(&mut self, delta: isize) {
        let last = self.matches.len().saturating_sub(1);
        self.selected = match (self.selected, delta) {
            (None, step) if step > 0 => Some(0),
            (Some(index), step) if step < 0 => index.checked_sub(1),
            (Some(index), _) => Some((index + 1).min(last)),
            (None, _) => None,
        };
    }

    fn accepted(&self) -> Option<String> {
        self.selected
            .and_then(|row| self.matches.get(row))
            .map(|&index| self.candidates[index].clone())
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    fn ac() -> Autocomplete {
        Autocomplete::new(vec![
            "Rewe big shop".to_string(),
            "Edeka".to_string(),
            "Rewe small".to_string(),
        ])
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn matched(ac: &Autocomplete) -> Vec<String> {
        ac.matches
            .iter()
            .map(|&i| ac.candidates[i].clone())
            .collect()
    }

    #[test]
    fn matches_contain_query_newest_first() {
        let mut ac = ac();
        ac.refresh("rewe");
        assert_eq!(matched(&ac), vec!["Rewe big shop", "Rewe small"]);
        assert!(ac.is_open());
    }

    #[test]
    fn exact_match_is_excluded() {
        let mut ac = ac();
        ac.refresh("Edeka");
        assert!(matched(&ac).is_empty());
        assert!(!ac.is_open());
    }

    #[test]
    fn cap_limits_match_count() {
        let many: Vec<String> = (0..20).map(|n| format!("Store {n}")).collect();
        let mut ac = Autocomplete::new(many);
        ac.refresh("store");
        assert_eq!(ac.matches.len(), MAX_SUGGESTIONS);
    }

    #[test]
    fn esc_closes_and_a_later_edit_reopens() {
        let mut ac = ac();
        ac.refresh("rewe");
        assert!(matches!(ac.on_key(key(KeyCode::Esc)), AcOutcome::Closed));
        assert!(!ac.is_open());
        ac.refresh("rewe s");
        assert!(ac.is_open());
    }

    #[test]
    fn accept_returns_the_highlighted_string() {
        let mut ac = ac();
        ac.refresh("rewe");
        ac.on_key(key(KeyCode::Down));
        match ac.on_key(key(KeyCode::Enter)) {
            AcOutcome::Accepted(value) => assert_eq!(value, "Rewe big shop"),
            _ => panic!("expected acceptance"),
        }
        assert!(!ac.is_open());
    }
}
