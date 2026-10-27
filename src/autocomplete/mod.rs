//! Autocomplete suggestions for a text field.
//!
//! Holds the candidate list (in the order the host wants them ranked) and, for
//! the current query, every candidate that contains it (case-insensitive). The
//! keys follow blink.cmp: nothing is selected until the user navigates, `Tab`
//! takes the selection or else the first match, `Enter` takes only an active
//! selection, and `Esc` closes just the suggestions.
//!
//! Two renderings share this state: [`Autocomplete::render_popup`] draws a
//! floating menu over the view (`popup`), and [`Autocomplete::lines`] returns
//! rows a host appends to its own text (`inline`).

mod inline;
mod matching;
mod popup;

use std::ops::Range;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub use popup::PopupStyle;

use super::nav::{self, ScrollView};

/// Most suggestion rows visible at once; further matches scroll.
pub const MAX_VISIBLE: usize = 8;

/// What a key press means while the suggestions may be open.
pub enum AcOutcome {
    /// The user accepted this suggestion; the caller fills the field with it.
    Accepted(String),
    /// The highlight moved; the caller just redraws.
    Navigated,
    /// The suggestions were closed (`Esc`); the caller just redraws.
    Closed,
    /// Not an autocomplete key, or the suggestions are closed: the caller
    /// handles the key normally. `←`/`→` land here too, after closing the
    /// suggestions, so the caller still moves the caret.
    Ignored,
}

/// Suggestion state for one text field.
pub struct Autocomplete {
    candidates: Vec<String>,
    matches: Vec<Match>,
    /// The highlighted row (an index into `matches`), `None` until the user
    /// navigates.
    selected: Option<usize>,
    /// The first visible row for a menu of [`MAX_VISIBLE`] rows.
    scroll: usize,
    dismissed: bool,
    /// When set, an empty query lists every candidate instead of hiding the
    /// suggestions (a "type the trigger to see all options" affordance).
    open_on_empty: bool,
}

/// One candidate that matches the query.
struct Match {
    /// The candidate's index in `candidates`.
    index: usize,
    /// The matched part of the candidate, in chars (empty for an empty query).
    range: Range<usize>,
}

/// What a key does to an open suggestion list.
enum KeyAction {
    Next,
    Previous,
    /// `Tab`: the selection, or else the first match.
    AcceptOrFirst,
    /// `Enter`: only an active selection.
    AcceptSelected,
    Close,
    /// `←`/`→`: close, but leave the key to the caller.
    CloseAndPass,
}

impl Autocomplete {
    /// Builds the autocomplete over `candidates`, best first. It stays hidden
    /// until the first [`refresh`](Self::refresh).
    pub fn new(candidates: Vec<String>) -> Self {
        Self {
            candidates,
            matches: Vec::new(),
            selected: None,
            scroll: 0,
            dismissed: true,
            open_on_empty: false,
        }
    }

    /// Makes the suggestions open on an empty query, listing every candidate
    /// (for a trigger-to-open menu). Chainable on [`new`](Self::new).
    #[must_use]
    pub fn open_on_empty(mut self) -> Self {
        self.open_on_empty = true;
        self
    }

    /// Recomputes the matches for `query`: every candidate containing it
    /// (case-insensitive, an exact match included), in candidate order. An
    /// empty query hides the suggestions unless
    /// [`open_on_empty`](Self::open_on_empty) was set.
    ///
    /// The highlight survives only while the same candidate still matches, so
    /// narrowing the query never moves it onto a different entry.
    pub fn refresh(&mut self, query: &str) {
        let previous = self.selected_candidate();
        self.dismissed = false;
        self.matches = self.find_matches(query.trim());
        self.selected = previous.and_then(|candidate| {
            self.matches.iter().position(|hit| hit.index == candidate)
        });
        self.scroll = match self.selected {
            Some(row) => nav::keep_visible(self.menu_view(), row),
            None => 0,
        };
    }

    /// Whether the suggestions are currently shown.
    pub fn is_open(&self) -> bool {
        !self.dismissed && !self.matches.is_empty()
    }

    /// Interprets `key` against the suggestion state.
    pub fn on_key(&mut self, key: KeyEvent) -> AcOutcome {
        if !self.is_open() {
            return AcOutcome::Ignored;
        }
        let Some(action) = key_action(key) else {
            return AcOutcome::Ignored;
        };
        match action {
            KeyAction::Next => self.move_selection(1),
            KeyAction::Previous => self.move_selection(-1),
            KeyAction::AcceptOrFirst => {
                return self.accept(self.selected.unwrap_or(0));
            }
            KeyAction::AcceptSelected => {
                return match self.selected {
                    Some(row) => self.accept(row),
                    None => AcOutcome::Ignored,
                };
            }
            KeyAction::Close => {
                self.close();
                return AcOutcome::Closed;
            }
            KeyAction::CloseAndPass => {
                self.close();
                return AcOutcome::Ignored;
            }
        }
        AcOutcome::Navigated
    }

    /// The candidates matching `needle`, in candidate order.
    fn find_matches(&self, needle: &str) -> Vec<Match> {
        if needle.is_empty() {
            if !self.open_on_empty {
                return Vec::new();
            }
            return (0..self.candidates.len())
                .map(|index| Match { index, range: 0..0 })
                .collect();
        }
        self.candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| {
                matching::find_ci(candidate, needle)
                    .map(|range| Match { index, range })
            })
            .collect()
    }

    /// The candidate index behind the highlighted row.
    fn selected_candidate(&self) -> Option<usize> {
        self.selected
            .and_then(|row| self.matches.get(row))
            .map(|hit| hit.index)
    }

    /// Moves the highlight by `delta`, wrapping around both ends. With nothing
    /// highlighted yet, a step down enters at the top and a step up at the
    /// bottom.
    fn move_selection(&mut self, delta: isize) {
        let len = self.matches.len();
        let row = match self.selected {
            None if delta > 0 => 0,
            None => len.saturating_sub(1),
            Some(row) => nav::cycle(row, len, delta.signum()),
        };
        self.selected = Some(row);
        self.scroll = nav::keep_visible(self.menu_view(), row);
    }

    /// Closes the suggestions on `row` and hands its candidate back.
    fn accept(&mut self, row: usize) -> AcOutcome {
        let Some(value) = self
            .matches
            .get(row)
            .map(|hit| self.candidates[hit.index].clone())
        else {
            return AcOutcome::Ignored;
        };
        self.close();
        AcOutcome::Accepted(value)
    }

    fn close(&mut self) {
        self.dismissed = true;
        self.selected = None;
    }

    /// The scroll window of a full-height menu.
    fn menu_view(&self) -> ScrollView {
        ScrollView {
            total: self.matches.len(),
            offset: self.scroll,
            viewport: MAX_VISIBLE,
        }
    }

    /// The first visible row for a window of `viewport` rows, keeping the
    /// highlight in view. A host may grant fewer than [`MAX_VISIBLE`] rows, so
    /// the window is fitted at render time rather than stored.
    fn window_offset(&self, viewport: usize) -> usize {
        let view = ScrollView {
            total: self.matches.len(),
            offset: self.scroll,
            viewport,
        };
        if let Some(row) = self.selected {
            return nav::keep_visible(view, row);
        }
        self.scroll.min(view.total.saturating_sub(viewport))
    }
}

/// Maps `key` onto what it does to an open suggestion list, or `None` when the
/// key is not one of its keys. `Ctrl`/`Alt` variants of the plain keys stay
/// with the caller (a form binds `Ctrl+Enter`, `Ctrl+↑` and the like).
fn key_action(key: KeyEvent) -> Option<KeyAction> {
    let command = key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    if command {
        return command_action(key);
    }
    match key.code {
        KeyCode::Down => Some(KeyAction::Next),
        KeyCode::Up | KeyCode::BackTab => Some(KeyAction::Previous),
        KeyCode::Tab => Some(KeyAction::AcceptOrFirst),
        KeyCode::Enter => Some(KeyAction::AcceptSelected),
        KeyCode::Esc => Some(KeyAction::Close),
        KeyCode::Left | KeyCode::Right => Some(KeyAction::CloseAndPass),
        _ => None,
    }
}

/// The `Ctrl` chords of an open suggestion list: `Ctrl+J`/`Ctrl+K` step like
/// `↓`/`↑` (as in blink.cmp), and a `Ctrl`/`Alt` arrow still closes it while
/// the caller moves the caret.
fn command_action(key: KeyEvent) -> Option<KeyAction> {
    let only_control = key.modifiers == KeyModifiers::CONTROL;
    match key.code {
        KeyCode::Char('j') if only_control => Some(KeyAction::Next),
        KeyCode::Char('k') if only_control => Some(KeyAction::Previous),
        KeyCode::Left | KeyCode::Right => Some(KeyAction::CloseAndPass),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
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

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn matched(ac: &Autocomplete) -> Vec<String> {
        ac.matches
            .iter()
            .map(|hit| ac.candidates[hit.index].clone())
            .collect()
    }

    fn accepted(outcome: AcOutcome) -> Option<String> {
        match outcome {
            AcOutcome::Accepted(value) => Some(value),
            _ => None,
        }
    }

    fn numbered(count: usize) -> Autocomplete {
        Autocomplete::new((0..count).map(|n| format!("Store {n}")).collect())
    }

    #[test]
    fn matches_contain_query_in_candidate_order() {
        let mut ac = ac();
        ac.refresh("rewe");
        assert_eq!(matched(&ac), vec!["Rewe big shop", "Rewe small"]);
        assert!(ac.is_open());
    }

    #[test]
    fn exact_match_is_listed() {
        let mut ac = ac();
        ac.refresh("edeka");
        assert_eq!(matched(&ac), vec!["Edeka"]);
        assert!(ac.is_open());
    }

    #[test]
    fn match_records_the_matched_char_range() {
        let mut ac = ac();
        ac.refresh("small");
        assert_eq!(ac.matches[0].range, 5..10);
    }

    #[test]
    fn empty_query_hides_by_default() {
        let mut ac = ac();
        ac.refresh("  ");
        assert!(!ac.is_open());
    }

    #[test]
    fn open_on_empty_lists_everything_without_a_preselection() {
        let mut menu = ac().open_on_empty();
        menu.refresh("");
        assert!(menu.is_open());
        assert_eq!(matched(&menu).len(), 3);
        assert_eq!(menu.selected, None);
    }

    #[test]
    fn all_matches_are_kept_beyond_the_visible_rows() {
        let mut ac = numbered(20);
        ac.refresh("store");
        assert_eq!(ac.matches.len(), 20);
    }

    #[test]
    fn nothing_is_selected_until_the_user_navigates() {
        let mut ac = ac();
        ac.refresh("rewe");
        assert_eq!(ac.selected, None);
    }

    #[test]
    fn down_enters_at_the_top_and_up_at_the_bottom() {
        let mut down = ac();
        down.refresh("rewe");
        down.on_key(key(KeyCode::Down));
        assert_eq!(down.selected, Some(0));

        let mut up = ac();
        up.refresh("rewe");
        up.on_key(key(KeyCode::Up));
        assert_eq!(up.selected, Some(1));
    }

    #[test]
    fn navigation_wraps_around_both_ends() {
        let mut ac = ac();
        ac.refresh("rewe");
        ac.on_key(key(KeyCode::Down)); // -> 0
        ac.on_key(key(KeyCode::Up)); // wraps 0 -> last
        assert_eq!(ac.selected, Some(1));
        ac.on_key(key(KeyCode::Down)); // wraps last -> 0
        assert_eq!(ac.selected, Some(0));
    }

    #[test]
    fn ctrl_j_and_ctrl_k_step_like_the_arrows() {
        let mut ac = ac();
        ac.refresh("rewe");
        assert!(matches!(ac.on_key(ctrl('j')), AcOutcome::Navigated));
        assert_eq!(ac.selected, Some(0));
        assert!(matches!(ac.on_key(ctrl('k')), AcOutcome::Navigated));
        assert_eq!(ac.selected, Some(1));
    }

    #[test]
    fn back_tab_steps_up() {
        let mut ac = ac();
        ac.refresh("rewe");
        ac.on_key(key(KeyCode::BackTab));
        assert_eq!(ac.selected, Some(1));
    }

    #[test]
    fn tab_without_a_selection_accepts_the_first_match() {
        let mut ac = ac();
        ac.refresh("rewe");
        let value = accepted(ac.on_key(key(KeyCode::Tab)));
        assert_eq!(value.as_deref(), Some("Rewe big shop"));
        assert!(!ac.is_open());
    }

    #[test]
    fn tab_accepts_the_selection() {
        let mut ac = ac();
        ac.refresh("rewe");
        ac.on_key(key(KeyCode::Up)); // -> "Rewe small"
        let value = accepted(ac.on_key(key(KeyCode::Tab)));
        assert_eq!(value.as_deref(), Some("Rewe small"));
    }

    #[test]
    fn enter_without_a_selection_is_left_to_the_caller() {
        let mut ac = ac();
        ac.refresh("rewe");
        assert!(matches!(ac.on_key(key(KeyCode::Enter)), AcOutcome::Ignored));
        assert!(ac.is_open());
    }

    #[test]
    fn enter_accepts_the_selection() {
        let mut ac = ac();
        ac.refresh("rewe");
        ac.on_key(key(KeyCode::Down));
        let value = accepted(ac.on_key(key(KeyCode::Enter)));
        assert_eq!(value.as_deref(), Some("Rewe big shop"));
        assert!(!ac.is_open());
    }

    #[test]
    fn ctrl_enter_is_left_to_the_caller() {
        let mut ac = ac();
        ac.refresh("rewe");
        ac.on_key(key(KeyCode::Down));
        let chord = KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL);
        assert!(matches!(ac.on_key(chord), AcOutcome::Ignored));
        assert!(ac.is_open());
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
    fn left_and_right_close_but_pass_the_key_on() {
        for code in [KeyCode::Left, KeyCode::Right] {
            let mut ac = ac();
            ac.refresh("rewe");
            assert!(matches!(ac.on_key(key(code)), AcOutcome::Ignored));
            assert!(!ac.is_open());
        }
    }

    #[test]
    fn other_ctrl_chords_are_left_alone() {
        let mut ac = ac();
        ac.refresh("rewe");
        assert!(matches!(ac.on_key(ctrl('a')), AcOutcome::Ignored));
        assert!(ac.is_open());
    }

    #[test]
    fn keys_are_ignored_while_closed() {
        let mut ac = ac();
        assert!(matches!(ac.on_key(ctrl('k')), AcOutcome::Ignored));
        assert!(matches!(ac.on_key(key(KeyCode::Tab)), AcOutcome::Ignored));
    }

    #[test]
    fn selection_follows_the_candidate_across_a_refresh() {
        let mut ac = ac();
        ac.refresh("e");
        // "Rewe big shop", "Edeka", "Rewe small": select "Rewe small".
        ac.on_key(key(KeyCode::Up));
        ac.refresh("small");
        assert_eq!(ac.selected, Some(0));
        assert_eq!(matched(&ac), vec!["Rewe small"]);
    }

    #[test]
    fn selection_is_dropped_when_its_candidate_stops_matching() {
        let mut ac = ac();
        ac.refresh("rewe");
        ac.on_key(key(KeyCode::Down)); // "Rewe big shop"
        ac.refresh("rewe s");
        assert_eq!(ac.selected, None);
    }

    #[test]
    fn scroll_follows_the_selection_and_wraps() {
        let mut ac = numbered(12);
        ac.refresh("store");
        ac.on_key(key(KeyCode::Up)); // last row
        assert_eq!(ac.selected, Some(11));
        assert_eq!(ac.scroll, 12 - MAX_VISIBLE);
        ac.on_key(key(KeyCode::Down)); // wraps to the top
        assert_eq!(ac.scroll, 0);
    }

    #[test]
    fn window_fits_a_shorter_viewport_around_the_selection() {
        let mut ac = numbered(12);
        ac.refresh("store");
        for _ in 0..6 {
            ac.on_key(key(KeyCode::Down));
        }
        assert_eq!(ac.selected, Some(5));
        // A host that grants only three rows still shows row 5.
        assert_eq!(ac.window_offset(3), 3);
    }
}
