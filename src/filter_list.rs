//! The shared state and key handling behind the filtered list overlays.
//!
//! `finder`, `help` and `command_palette` are the same widget wearing three
//! hats: a query buffer, a cursor into the rows that survive the filter, and a
//! scroll offset that follows it. They differ only in what a row *is* and how
//! it is drawn - not in how the keys move through it. That dispatch lives here
//! once, so a fix to it (the `Ctrl` guard, a page jump, `Home`/`End`) reaches
//! all three instead of two.
//!
//! The caller keeps the keys that are its own - `Esc`, a confirming `Enter`,
//! `?` closing the help - and hands the rest to [`FilterList::handle_key`],
//! exactly as a text field delegates to `input::apply_edit_key`.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent};

use crate::{input, nav};

/// Query, cursor and scroll state shared by the filtered list overlays.
pub(crate) struct FilterList {
    /// The filter text typed so far.
    pub(crate) query: String,
    /// Index into the currently selectable rows.
    pub(crate) cursor: usize,
    /// Persistent list scroll offset, so the view and scrollbar follow the
    /// cursor across frames.
    pub(crate) offset: Cell<usize>,
    /// The list viewport height captured at render, driving the page jumps.
    pub(crate) viewport: Cell<usize>,
}

impl FilterList {
    /// An empty query with the cursor on the first row.
    pub(crate) fn new() -> Self {
        Self {
            query: String::new(),
            cursor: 0,
            offset: Cell::new(0),
            viewport: Cell::new(1),
        }
    }

    /// Applies one navigation or typing key, reporting whether it was used.
    ///
    /// `count` is the number of selectable rows *after* filtering, and
    /// `section_starts` the cursor index each section begins at (empty for an
    /// unsectioned list, which makes `Tab`/`BackTab` no-ops). Both are passed
    /// per call rather than stored: they are recomputed from the query on
    /// every keystroke anyway, and caching them here would be a second source
    /// of truth to keep in step.
    ///
    /// A `Ctrl` chord is always reported as unused. The overlays bind none of
    /// their own, and without that guard `Ctrl+U` would type a `u` into the
    /// query instead of being left for the caller. `Alt` alone and `AltGr`
    /// (which crossterm reports as `Ctrl+Alt`) still type, as they must in any
    /// text field - see [`input::is_command`].
    pub(crate) fn handle_key(
        &mut self,
        key: KeyEvent,
        count: usize,
        section_starts: &[usize],
    ) -> bool {
        if input::is_command(key) {
            return false;
        }
        let page = self.viewport.get().max(1) as isize;
        match key.code {
            KeyCode::Up => self.cursor = nav::cycle(self.cursor, count, -1),
            KeyCode::Down => self.cursor = nav::cycle(self.cursor, count, 1),
            KeyCode::PageUp => {
                self.cursor = nav::step_clamped(self.cursor, count, -page);
            }
            KeyCode::PageDown => {
                self.cursor = nav::step_clamped(self.cursor, count, page);
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = count.saturating_sub(1),
            KeyCode::Tab => self.jump_section(section_starts, 1),
            KeyCode::BackTab => self.jump_section(section_starts, -1),
            KeyCode::Backspace => {
                self.query.pop();
                self.cursor = 0;
            }
            KeyCode::Char(ch) => {
                self.query.push(ch);
                self.cursor = 0;
            }
            _ => return false,
        }
        true
    }

    /// Appends pasted `text` to the query, dropping control characters.
    pub(crate) fn paste(&mut self, text: &str) {
        self.query
            .extend(text.chars().filter(|ch| !ch.is_control()));
        self.cursor = 0;
    }

    /// Moves the cursor to the first row of the next (`+1`) or previous (`-1`)
    /// section, wrapping around.
    fn jump_section(&mut self, section_starts: &[usize], direction: isize) {
        if section_starts.is_empty() {
            return;
        }
        // The section the cursor is in: the last start at or before it.
        let current = section_starts
            .iter()
            .rposition(|&start| start <= self.cursor)
            .unwrap_or(0);
        let next = nav::cycle(current, section_starts.len(), direction);
        self.cursor = section_starts[next];
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    fn press(list: &mut FilterList, code: KeyCode, count: usize) -> bool {
        list.handle_key(KeyEvent::new(code, KeyModifiers::NONE), count, &[])
    }

    #[test]
    fn up_and_down_cycle_through_the_rows() {
        let mut list = FilterList::new();
        press(&mut list, KeyCode::Down, 3);
        assert_eq!(list.cursor, 1);
        press(&mut list, KeyCode::Up, 3);
        assert_eq!(list.cursor, 0);
        // Past the first entry wraps to the last.
        press(&mut list, KeyCode::Up, 3);
        assert_eq!(list.cursor, 2);
    }

    #[test]
    fn home_and_end_clamp_instead_of_wrapping() {
        let mut list = FilterList::new();
        press(&mut list, KeyCode::End, 4);
        assert_eq!(list.cursor, 3);
        press(&mut list, KeyCode::End, 4);
        assert_eq!(list.cursor, 3);
        press(&mut list, KeyCode::Home, 4);
        assert_eq!(list.cursor, 0);
    }

    #[test]
    fn a_page_jump_moves_by_the_viewport_and_clamps() {
        let mut list = FilterList::new();
        list.viewport.set(5);
        press(&mut list, KeyCode::PageDown, 20);
        assert_eq!(list.cursor, 5);
        press(&mut list, KeyCode::PageUp, 20);
        assert_eq!(list.cursor, 0);
        press(&mut list, KeyCode::PageUp, 20);
        assert_eq!(list.cursor, 0, "the page jump must not wrap");
    }

    #[test]
    fn typing_fills_the_query_and_resets_the_cursor() {
        let mut list = FilterList::new();
        press(&mut list, KeyCode::Down, 5);
        press(&mut list, KeyCode::Char('a'), 5);
        assert_eq!(list.query, "a");
        assert_eq!(list.cursor, 0, "a narrower list invalidates the cursor");
        press(&mut list, KeyCode::Backspace, 5);
        assert!(list.query.is_empty());
    }

    /// The guard that keeps `Ctrl+U` out of the query buffer.
    #[test]
    fn a_ctrl_chord_is_left_to_the_caller() {
        let mut list = FilterList::new();
        for code in [
            KeyCode::Char('u'),
            KeyCode::Char('a'),
            KeyCode::Down,
            KeyCode::Home,
        ] {
            let used = list.handle_key(
                KeyEvent::new(code, KeyModifiers::CONTROL),
                5,
                &[],
            );
            assert!(!used, "Ctrl+{code:?} was consumed");
        }
        assert!(list.query.is_empty(), "a chord typed into the query");
        assert_eq!(list.cursor, 0, "a chord moved the cursor");
    }

    /// `AltGr` arrives as `Ctrl+Alt` and must type, or a German layout could
    /// not enter `@`, `\` or `[` into the filter.
    #[test]
    fn altgr_still_types_into_the_query() {
        let mut list = FilterList::new();
        let used = list.handle_key(
            KeyEvent::new(
                KeyCode::Char('@'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            5,
            &[],
        );
        assert!(used);
        assert_eq!(list.query, "@");
    }

    #[test]
    fn tab_jumps_between_section_starts_and_wraps() {
        let mut list = FilterList::new();
        let starts = [0, 3, 7];
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        list.handle_key(tab, 10, &starts);
        assert_eq!(list.cursor, 3);
        list.handle_key(tab, 10, &starts);
        assert_eq!(list.cursor, 7);
        list.handle_key(tab, 10, &starts);
        assert_eq!(list.cursor, 0, "past the last section wraps to the first");
    }

    #[test]
    fn tab_does_nothing_without_sections() {
        let mut list = FilterList::new();
        list.cursor = 2;
        press(&mut list, KeyCode::Tab, 5);
        assert_eq!(list.cursor, 2);
    }

    #[test]
    fn an_empty_list_leaves_the_cursor_at_zero() {
        let mut list = FilterList::new();
        for code in [KeyCode::Down, KeyCode::Up, KeyCode::End] {
            press(&mut list, code, 0);
            assert_eq!(list.cursor, 0, "{code:?} on an empty list");
        }
    }

    #[test]
    fn a_paste_appends_without_its_control_characters() {
        let mut list = FilterList::new();
        list.paste("ab\ncd\t");
        assert_eq!(list.query, "abcd");
    }
}
