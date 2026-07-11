//! Inline autocomplete dropdown for text fields.
//!
//! Holds a newest-first candidate list and, for the current query, the subset
//! that contains it (case-insensitive). A small state machine decides whether a
//! key navigates the dropdown, accepts a suggestion, hides it, or is left for
//! the caller. Rendering returns ready-made [`Line`]s the host can append.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    style::{Modifier, Style},
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
    /// When set, an empty query lists every candidate instead of hiding the
    /// dropdown (a "press the trigger to see all options" affordance).
    open_on_empty: bool,
    /// When set, the selected row renders as a bright, full-width accent bar
    /// with a `▸` marker (for a menu-style popup); otherwise only its text is
    /// tinted (the inline-suggestion look).
    strong_highlight: bool,
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
            open_on_empty: false,
            strong_highlight: false,
        }
    }

    /// Makes the dropdown open on an empty query, listing every candidate (for a
    /// trigger-to-open menu). Chainable on [`new`](Self::new).
    #[must_use]
    pub fn open_on_empty(mut self) -> Self {
        self.open_on_empty = true;
        self
    }

    /// Renders the selected row as a bright, full-width accent bar with a `▸`
    /// marker (for a menu-style popup over a dark backdrop, where the plain
    /// text-tinted highlight is hard to see). Chainable on [`new`](Self::new).
    #[must_use]
    pub fn strong_highlight(mut self) -> Self {
        self.strong_highlight = true;
        self
    }

    /// Recomputes the matches for `query`: candidates that contain it
    /// (case-insensitive) but are not exactly it, newest first, capped. An empty
    /// query hides the dropdown unless [`open_on_empty`](Self::open_on_empty)
    /// was set, in which case every candidate is listed with the first row
    /// pre-highlighted (so `↑/↓` and `Enter` work as a menu straight away).
    pub fn refresh(&mut self, query: &str) {
        self.dismissed = false;
        let needle = query.trim().to_lowercase();
        let empty = needle.is_empty();
        self.matches.clear();
        self.overflow = 0;
        if !empty || self.open_on_empty {
            for (index, candidate) in self.candidates.iter().enumerate() {
                let lower = candidate.to_lowercase();
                if empty || (lower.contains(&needle) && lower != needle) {
                    if self.matches.len() < MAX_SUGGESTIONS {
                        self.matches.push(index);
                    } else {
                        self.overflow += 1;
                    }
                }
            }
        }
        self.selected = match self.selected {
            // Keep a still-valid highlight (so navigation sticks across a
            // refresh); a menu-style dropdown pre-selects the first row.
            Some(index) if index < self.matches.len() => Some(index),
            _ if self.open_on_empty && !self.matches.is_empty() => Some(0),
            _ => None,
        };
    }

    /// Whether the dropdown is currently shown.
    pub fn is_open(&self) -> bool {
        !self.dismissed && !self.matches.is_empty()
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
    /// highlighted row uses the selection style. Empty when closed. In
    /// [`strong_highlight`](Self::strong_highlight) mode `indent` is ignored and
    /// the rows render as a full-width menu (see [`menu_lines`](Self::menu_lines)).
    pub fn lines(
        &self,
        palette: &Palette,
        indent: usize,
        base: Style,
    ) -> Vec<Line<'static>> {
        if !self.is_open() {
            return Vec::new();
        }
        if self.strong_highlight {
            return self.menu_lines(palette, base);
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

    /// Full-width menu rows: a 2-col marker column (`▸ ` on the selected row,
    /// blanks otherwise) and each candidate padded to a common width, so the
    /// selected row fills as a bright accent bar with dark bold text.
    fn menu_lines(&self, palette: &Palette, base: Style) -> Vec<Line<'static>> {
        const MARKER_SELECTED: &str = "\u{25b8} ";
        const MARKER_NONE: &str = "  ";
        let overflow_label =
            (self.overflow > 0).then(|| format!("+{} more", self.overflow));
        let width = self
            .matches
            .iter()
            .map(|&candidate| self.candidates[candidate].chars().count())
            .chain(overflow_label.iter().map(|label| label.chars().count()))
            .max()
            .unwrap_or(0);
        let highlight = Style::default()
            .bg(style::to_ratatui(palette.accent))
            .fg(style::to_ratatui(palette.background))
            .add_modifier(Modifier::BOLD);
        let mut lines: Vec<Line<'static>> = self
            .matches
            .iter()
            .enumerate()
            .map(|(row, &candidate)| {
                let text = &self.candidates[candidate];
                let filler = " ".repeat(width - text.chars().count());
                if Some(row) == self.selected {
                    Line::from(Span::styled(
                        format!("{MARKER_SELECTED}{text}{filler}"),
                        highlight,
                    ))
                } else {
                    Line::from(vec![
                        Span::styled(MARKER_NONE, base),
                        Span::styled(format!("{text}{filler}"), base),
                    ])
                }
            })
            .collect();
        if let Some(label) = overflow_label {
            let filler = " ".repeat(width - label.chars().count());
            lines.push(Line::from(vec![
                Span::styled(MARKER_NONE, base),
                Span::styled(
                    format!("{label}{filler}"),
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
    fn empty_query_hides_by_default_but_lists_all_when_open_on_empty() {
        let mut plain = ac();
        plain.refresh("");
        assert!(!plain.is_open());

        let mut menu = ac().open_on_empty();
        menu.refresh("");
        assert!(menu.is_open());
        assert_eq!(matched(&menu).len(), 3);
        // The first row is pre-highlighted, so Enter confirms it right away and
        // arrows cycle from there.
        assert_eq!(menu.selected, Some(0));
        menu.on_key(key(KeyCode::Up)); // wraps 0 -> last
        assert_eq!(menu.selected, Some(2));
        assert!(matches!(
            menu.on_key(key(KeyCode::Enter)),
            AcOutcome::Accepted(_)
        ));
        // Typing still filters, and an exact match is still excluded.
        menu.refresh("edeka");
        assert!(matched(&menu).is_empty());
    }

    #[test]
    fn strong_highlight_marks_and_fills_the_selected_row() {
        use crate::theme::{ColorOverrides, ThemeRegistry};
        let palette = Palette::resolve(
            ThemeRegistry::builtin().resolve("default"),
            &ColorOverrides::default(),
        );
        let mut menu =
            Autocomplete::new(vec!["short".into(), "a longer entry".into()])
                .open_on_empty()
                .strong_highlight();
        menu.refresh(""); // opens with the first row pre-selected
        let lines = menu.lines(&palette, 0, Style::default());
        let text = |row: usize| {
            lines[row]
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        };
        // The selected row carries the ▸ marker; the others a blank gutter.
        assert!(text(0).starts_with("\u{25b8} "));
        assert!(text(1).starts_with("  "));
        // Rows are padded to a common width (a solid bar).
        assert_eq!(text(0).chars().count(), text(1).chars().count());
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
