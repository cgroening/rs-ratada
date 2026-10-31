//! The key state shared by the multi-select modals: cursor, checks, the
//! `Enter` rule and the hints that describe it.

use std::{cell::Cell, collections::HashSet};

use crossterm::event::{KeyCode, KeyEvent};

use super::picker::navigate_list;
use crate::{input, overlay::PopupFlow};

/// The hints while `Enter` takes the cursor row.
const PICK_HINTS: [(&str, &str); 3] =
    [("space", "tick"), ("enter", "pick"), ("esc", "cancel")];

/// The hints while `Enter` confirms the checks. Also the widest set, so the
/// box is sized for it and does not change width when the hints switch.
pub(super) const CONFIRM_HINTS: [(&str, &str); 3] =
    [("space", "tick"), ("enter", "confirm"), ("esc", "cancel")];

/// The cursor, the checked set and the list viewport height captured at
/// render (for page-wise navigation).
///
/// `Enter` with nothing checked takes the cursor row, so one item is picked
/// without ticking it first - unless the user already edited the checks: then
/// an empty set is what they asked for, and `Enter` confirms it.
pub(super) struct MultiSelect {
    pub(super) cursor: usize,
    pub(super) checked: HashSet<usize>,
    pub(super) viewport: Cell<usize>,
    /// Whether the user changed the checks (`Space`, or a mode switch that
    /// inverts them).
    has_edited: bool,
    /// Whether `Space` moves the cursor to the next row after toggling.
    advances_on_tick: bool,
}

impl MultiSelect {
    pub(super) fn new(initial: &[usize]) -> Self {
        Self {
            cursor: 0,
            checked: initial.iter().copied().collect(),
            viewport: Cell::new(1),
            has_edited: false,
            advances_on_tick: false,
        }
    }

    /// Lets `Space` move to the next row after toggling, so several rows are
    /// ticked one after another; on the last row the cursor stays.
    #[must_use]
    pub(super) fn advancing_on_tick(self) -> Self {
        Self {
            advances_on_tick: true,
            ..self
        }
    }

    /// Replaces the checks as a user edit (e.g. inverted by a mode switch).
    pub(super) fn replace_checks(&mut self, checks: HashSet<usize>) {
        self.checked = checks;
        self.has_edited = true;
    }

    /// The key hints for the current state: `enter` reads `pick` while it
    /// takes the cursor row, `confirm` otherwise.
    pub(super) fn hints(&self) -> [(&'static str, &'static str); 3] {
        if self.picks_cursor() {
            PICK_HINTS
        } else {
            CONFIRM_HINTS
        }
    }

    pub(super) fn handle_key(
        &mut self,
        key: KeyEvent,
        len: usize,
    ) -> PopupFlow<Vec<usize>> {
        if navigate_list(&mut self.cursor, key, len, self.viewport.get()) {
            return PopupFlow::Continue;
        }
        // This picker binds no Ctrl chord of its own, so one must not reach the
        // bare keys below (Ctrl+Space would toggle the checked row).
        if input::is_command(key) {
            return PopupFlow::Continue;
        }
        match key.code {
            KeyCode::Char(' ') => {
                self.toggle_cursor_row();
                if self.advances_on_tick {
                    self.cursor = (self.cursor + 1).min(len.saturating_sub(1));
                }
                PopupFlow::Continue
            }
            KeyCode::Enter => PopupFlow::Done(self.chosen()),
            KeyCode::Esc => PopupFlow::Cancelled,
            _ => PopupFlow::Continue,
        }
    }

    /// Whether `Enter` would take the cursor row instead of the checks.
    fn picks_cursor(&self) -> bool {
        self.checked.is_empty() && !self.has_edited
    }

    fn toggle_cursor_row(&mut self) {
        if !self.checked.insert(self.cursor) {
            self.checked.remove(&self.cursor);
        }
        self.has_edited = true;
    }

    /// What `Enter` confirms: the cursor row or the checks, ascending.
    fn chosen(&self) -> Vec<usize> {
        if self.picks_cursor() {
            return vec![self.cursor];
        }
        let mut chosen: Vec<usize> = self.checked.iter().copied().collect();
        chosen.sort_unstable();
        chosen
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    const LEN: usize = 4;

    fn bare(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Feeds `keys` to a state checked at `initial` and returns what the last
    /// key confirmed (`None` when it did not finish the modal).
    fn confirmed(initial: &[usize], keys: &[KeyCode]) -> Option<Vec<usize>> {
        run_keys(MultiSelect::new(initial), keys)
    }

    fn run_keys(
        mut state: MultiSelect,
        keys: &[KeyCode],
    ) -> Option<Vec<usize>> {
        let mut last = PopupFlow::Continue;
        for &code in keys {
            last = state.handle_key(bare(code), LEN);
        }
        match last {
            PopupFlow::Done(chosen) => Some(chosen),
            _ => None,
        }
    }

    #[test]
    fn enter_with_nothing_checked_takes_the_cursor_row() {
        let chosen = confirmed(&[], &[KeyCode::Down, KeyCode::Enter]);
        assert_eq!(chosen, Some(vec![1]));
    }

    #[test]
    fn space_keeps_the_cursor_by_default() {
        let keys = [KeyCode::Char(' '), KeyCode::Char(' '), KeyCode::Enter];
        // The second space unticks the same row again.
        assert_eq!(confirmed(&[], &keys), Some(vec![]));
    }

    #[test]
    fn space_moves_on_when_advancing() {
        let state = MultiSelect::new(&[]).advancing_on_tick();
        let keys = [KeyCode::Char(' '), KeyCode::Char(' '), KeyCode::Enter];
        assert_eq!(run_keys(state, &keys), Some(vec![0, 1]));
    }

    #[test]
    fn advancing_stops_on_the_last_row() {
        let state = MultiSelect::new(&[]).advancing_on_tick();
        let mut keys = vec![KeyCode::End, KeyCode::Char(' ')];
        // A second space on the last row unticks it instead of wrapping.
        keys.extend([KeyCode::Char(' '), KeyCode::Enter]);
        assert_eq!(run_keys(state, &keys), Some(vec![]));
    }

    #[test]
    fn enter_with_checks_confirms_exactly_the_checks() {
        let chosen = confirmed(&[3, 0], &[KeyCode::Down, KeyCode::Enter]);
        assert_eq!(chosen, Some(vec![0, 3]));
    }

    /// Unticking the last check and confirming must clear, not bring the
    /// cursor row straight back.
    #[test]
    fn enter_after_unticking_everything_confirms_nothing() {
        let chosen = confirmed(&[0], &[KeyCode::Char(' '), KeyCode::Enter]);
        assert_eq!(chosen, Some(vec![]));
    }

    #[test]
    fn replaced_checks_count_as_an_edit() {
        let mut state = MultiSelect::new(&[0]);
        state.replace_checks(HashSet::new());
        let chosen = state.handle_key(bare(KeyCode::Enter), LEN);
        assert!(matches!(chosen, PopupFlow::Done(rows) if rows.is_empty()));
    }

    #[test]
    fn the_enter_hint_names_what_enter_does() {
        let mut state = MultiSelect::new(&[]);
        assert_eq!(state.hints(), PICK_HINTS);
        state.handle_key(bare(KeyCode::Char(' ')), LEN);
        assert_eq!(state.hints(), CONFIRM_HINTS);
        assert_eq!(MultiSelect::new(&[2]).hints(), CONFIRM_HINTS);
    }
}
