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
    /// How many further matches exist beyond the [`MAX_SUGGESTIONS`] shown.
    overflow: usize,
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
            overflow: 0,
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
        self.overflow = 0;
        if !self.empty_query {
            for (index, candidate) in self.candidates.iter().enumerate() {
                let lower = candidate.to_lowercase();
                if lower.contains(&needle) && lower != needle {
                    if self.matches.len() < MAX_SUGGESTIONS {
                        self.matches.push(index);
                    } else {
                        self.overflow += 1;
                    }
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
        let mut lines: Vec<Line<'static>> = self
            .matches
            .iter()
            .enumerate()
            .map(|(row, &candidate)| {
                let row_style = if Some(row) == self.selected {
                    style::bg(palette.selection)
                } else {
                    base
                };
                Line::from(vec![
                    Span::raw(pad.clone()),
                    Span::styled(self.candidates[candidate].clone(), row_style),
                ])
            })
            .collect();
        if self.overflow > 0 {
            lines.push(Line::from(vec![
                Span::raw(pad),
                Span::styled(
                    format!("+{} more", self.overflow),
                    style::secondary(palette),
                ),
            ]));
        }
        lines
    }

    /// Moves the highlight by `delta`, wrapping around the ends so `Up` on the
    /// first row lands on the last and `Down` on the last returns to the first.
    /// With nothing highlighted yet, `Down` enters at the top and `Up` at the
    /// bottom.
    fn move_selection(&mut self, delta: isize) {
        let len = self.matches.len();
        if len == 0 {
            self.selected = None;
            return;
        }
        self.selected = Some(match (self.selected, delta) {
            (None, step) if step > 0 => 0,
            (None, _) => len - 1,
            (Some(index), step) if step > 0 => (index + 1) % len,
            (Some(index), _) => (index + len - 1) % len,
        });
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
    fn overflow_counts_matches_beyond_the_cap() {
        let many: Vec<String> = (0..20).map(|n| format!("Store {n}")).collect();
        let mut ac = Autocomplete::new(many);
        ac.refresh("store");
        assert_eq!(ac.overflow, 20 - MAX_SUGGESTIONS);
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
    fn navigation_wraps_around_both_ends() {
        let mut ac = ac();
        ac.refresh("rewe"); // two matches: index 0 and 1
        ac.on_key(key(KeyCode::Down)); // -> 0
        assert_eq!(ac.selected, Some(0));
        ac.on_key(key(KeyCode::Up)); // wraps 0 -> last
        assert_eq!(ac.selected, Some(1));
        ac.on_key(key(KeyCode::Down)); // wraps last -> 0
        assert_eq!(ac.selected, Some(0));
    }

    #[test]
    fn up_from_nothing_selected_enters_at_the_bottom() {
        let mut ac = ac();
        ac.refresh("rewe");
        ac.on_key(key(KeyCode::Up));
        assert_eq!(ac.selected, Some(1));
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
