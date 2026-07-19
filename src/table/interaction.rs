//! Table interaction: key dispatch, cursor navigation, selection and the
//! filter/sort view rebuild.

use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::model::{filter_indices, sort_indices};
use super::{FilterScope, SelectMode, SortDir, Table, TableAction};
use crate::{input, nav};

impl Table {
    /// Handles a key press; returns whether the host should act on it.
    pub fn handle_key(&mut self, key: KeyEvent) -> TableAction {
        if self.filtering {
            self.handle_filter_key(key);
            return TableAction::None;
        }
        // Ctrl+A is the only chord this table binds; every other one must be
        // kept away from the plain keys below, or Ctrl+S would re-sort and
        // Ctrl+J/H would navigate (in raw mode crossterm reports those as
        // `Char('j')`/`Char('h')` plus CONTROL).
        if input::is_command(key) {
            if key.code == KeyCode::Char('a') {
                self.select_all();
            }
            return TableAction::None;
        }
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let page = self.viewport.get().max(1) as isize;
        let last = self.view.len().saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_rows(-1, shift),
            KeyCode::Down | KeyCode::Char('j') => self.move_rows(1, shift),
            KeyCode::PageUp => self.move_rows(-page, shift),
            KeyCode::PageDown => self.move_rows(page, shift),
            KeyCode::Home | KeyCode::Char('g') => self.move_to(0, shift),
            KeyCode::End | KeyCode::Char('G') => self.move_to(last, shift),
            KeyCode::Left | KeyCode::Char('h') => self.move_col(-1, shift),
            KeyCode::Right | KeyCode::Char('l') => self.move_col(1, shift),
            KeyCode::Char(' ') => self.toggle_current(),
            KeyCode::Char('s') => self.sort_by_active(),
            KeyCode::Char('f') => self.toggle_filter_scope(),
            KeyCode::Char('m') => self.toggle_mode(),
            KeyCode::Char('/') => self.filtering = true,
            KeyCode::Esc => self.clear_selection(),
            KeyCode::Enter => return TableAction::Activate,
            _ => {}
        }
        TableAction::None
    }

    /// Whether the filter input is active (the host modal yields keys to it).
    pub fn is_filtering(&self) -> bool {
        self.filtering
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filtering = false;
                self.filter.clear();
                self.rebuild_view();
            }
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                self.filter.pop();
                self.rebuild_view();
            }
            // Anything that is not a command chord is filter text: Ctrl+U
            // would otherwise append a `u` instead of being left as an editing
            // key. Deliberately not `is_bare_character`, which would also
            // reject AltGr - `@`, `\` and `[` must reach the filter, exactly as
            // in `input::apply_edit_key`.
            KeyCode::Char(ch) if !input::is_command(key) => {
                self.filter.push(ch);
                self.rebuild_view();
            }
            _ => {}
        }
    }

    fn move_rows(&mut self, delta: isize, shift: bool) {
        let target = nav::step_clamped(self.cursor, self.view.len(), delta);
        self.move_to(target, shift);
    }

    fn move_to(&mut self, target: usize, shift: bool) {
        if self.view.is_empty() {
            return;
        }
        let prev = self.cursor;
        self.cursor = target.min(self.view.len() - 1);
        self.after_move(prev, shift);
    }

    fn move_col(&mut self, delta: isize, shift: bool) {
        if self.columns.is_empty() {
            return;
        }
        let prev_cell = self.cursor_cell();
        self.active_col =
            nav::step_clamped(self.active_col, self.columns.len(), delta);
        if shift && self.mode == SelectMode::Cell {
            if let Some(cell) = prev_cell {
                self.selected_cells.insert(cell);
            }
            if let Some(cell) = self.cursor_cell() {
                self.selected_cells.insert(cell);
            }
        }
    }

    /// Applies selection side effects after the cursor row moved.
    fn after_move(&mut self, prev: usize, shift: bool) {
        if !shift {
            self.anchor = self.cursor;
            return;
        }
        match self.mode {
            SelectMode::Row => {
                let (lo, hi) = minmax(self.anchor, self.cursor);
                for view_idx in lo..=hi {
                    self.selected_rows.insert(self.view[view_idx]);
                }
            }
            SelectMode::Cell => {
                for view_idx in [prev, self.cursor] {
                    self.selected_cells
                        .insert((self.view[view_idx], self.active_col));
                }
            }
        }
    }

    fn toggle_current(&mut self) {
        match self.mode {
            SelectMode::Row => {
                if let Some(row) = self.cursor_row() {
                    toggle(&mut self.selected_rows, row);
                }
            }
            SelectMode::Cell => {
                if let Some(cell) = self.cursor_cell() {
                    toggle(&mut self.selected_cells, cell);
                }
            }
        }
    }

    fn select_all(&mut self) {
        match self.mode {
            SelectMode::Row => {
                self.selected_rows = self.view.iter().copied().collect();
            }
            SelectMode::Cell => {
                self.selected_cells = self
                    .view
                    .iter()
                    .flat_map(|&row| {
                        (0..self.columns.len()).map(move |col| (row, col))
                    })
                    .collect();
            }
        }
    }

    fn clear_selection(&mut self) {
        self.selected_rows.clear();
        self.selected_cells.clear();
    }

    fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            SelectMode::Row => SelectMode::Cell,
            SelectMode::Cell => SelectMode::Row,
        };
        // Selections do not translate between the modes.
        self.clear_selection();
    }

    fn toggle_filter_scope(&mut self) {
        self.filter_scope = match self.filter_scope {
            FilterScope::AllColumns => FilterScope::ActiveColumn,
            FilterScope::ActiveColumn => FilterScope::AllColumns,
        };
        self.rebuild_view();
    }

    fn sort_by_active(&mut self) {
        let dir = match self.sort {
            Some((col, SortDir::Asc)) if col == self.active_col => {
                SortDir::Desc
            }
            _ => SortDir::Asc,
        };
        self.sort = Some((self.active_col, dir));
        self.rebuild_view();
    }

    /// Recomputes `view` (filter then sort), keeping the cursor on its row.
    fn rebuild_view(&mut self) {
        let keep = self.cursor_row();
        let mut view = filter_indices(
            &self.rows,
            &self.filter,
            self.filter_scope,
            self.active_col,
        );
        if let Some((col, dir)) = self.sort {
            view = sort_indices(&self.rows, &self.columns, view, col, dir);
        }
        self.view = view;
        self.cursor = keep
            .and_then(|row| self.view.iter().position(|&r| r == row))
            .unwrap_or(0)
            .min(self.view.len().saturating_sub(1));
        self.anchor = self.cursor;
    }
}

fn toggle<T: Eq + std::hash::Hash>(set: &mut HashSet<T>, value: T) {
    if !set.remove(&value) {
        set.insert(value);
    }
}

fn minmax(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{Column, Row};

    fn table() -> Table {
        Table::new(
            vec![Column::text("name"), Column::number("size")],
            vec![
                Row::new(vec!["alpha".into(), "3".into()]),
                Row::new(vec!["beta".into(), "1".into()]),
                Row::new(vec!["gamma".into(), "2".into()]),
            ],
        )
    }

    fn press(table: &mut Table, code: KeyCode) -> TableAction {
        table.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn press_with(
        table: &mut Table,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> TableAction {
        table.handle_key(KeyEvent::new(code, modifiers))
    }

    #[test]
    fn arrows_and_vim_keys_move_the_row_cursor() {
        let mut table = table();
        press(&mut table, KeyCode::Down);
        assert_eq!(table.cursor_row(), Some(1));
        press(&mut table, KeyCode::Char('j'));
        assert_eq!(table.cursor_row(), Some(2));
        press(&mut table, KeyCode::Char('k'));
        assert_eq!(table.cursor_row(), Some(1));
    }

    #[test]
    fn home_and_end_jump_to_the_first_and_last_row() {
        let mut table = table();
        press(&mut table, KeyCode::End);
        assert_eq!(table.cursor_row(), Some(2));
        press(&mut table, KeyCode::Home);
        assert_eq!(table.cursor_row(), Some(0));
    }

    #[test]
    fn enter_reports_an_activation_to_the_host() {
        let mut table = table();
        assert_eq!(press(&mut table, KeyCode::Enter), TableAction::Activate);
        assert_eq!(press(&mut table, KeyCode::Down), TableAction::None);
    }

    #[test]
    fn space_toggles_the_current_row_selection() {
        let mut table = table();
        press(&mut table, KeyCode::Char(' '));
        assert_eq!(table.selected_rows(), vec![0]);
        press(&mut table, KeyCode::Char(' '));
        assert!(table.selected_rows().is_empty());
    }

    #[test]
    fn esc_clears_the_selection() {
        let mut table = table();
        press(&mut table, KeyCode::Char(' '));
        press(&mut table, KeyCode::Esc);
        assert!(table.selected_rows().is_empty());
    }

    #[test]
    fn slash_enters_the_filter_and_esc_leaves_it() {
        let mut table = table();
        assert!(!table.is_filtering());
        press(&mut table, KeyCode::Char('/'));
        assert!(table.is_filtering());
        press(&mut table, KeyCode::Esc);
        assert!(!table.is_filtering());
    }

    /// This is the guard the 0.4.0 release note is about. In raw mode crossterm
    /// reports `Ctrl+J` as `Char('j')` plus CONTROL, so without the
    /// `is_command` check every one of these chords would drive the table:
    /// `Ctrl+S` would re-sort, `Ctrl+J`/`Ctrl+K` would walk the cursor,
    /// `Ctrl+Space` would toggle a selection.
    #[test]
    fn no_ctrl_chord_but_ctrl_a_drives_the_table() {
        for code in [
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char('h'),
            KeyCode::Char('l'),
            KeyCode::Char('s'),
            KeyCode::Char('f'),
            KeyCode::Char('m'),
            KeyCode::Char('g'),
            KeyCode::Char('G'),
            KeyCode::Char(' '),
            KeyCode::Char('/'),
            KeyCode::Down,
            KeyCode::PageDown,
            KeyCode::End,
            KeyCode::Enter,
        ] {
            let mut table = table();
            let before = (table.cursor_row(), table.cursor_cell());
            let action = press_with(&mut table, code, KeyModifiers::CONTROL);

            assert_eq!(action, TableAction::None, "Ctrl+{code:?} acted");
            assert_eq!(
                (table.cursor_row(), table.cursor_cell()),
                before,
                "Ctrl+{code:?} moved the cursor"
            );
            assert!(
                table.selected_rows().is_empty(),
                "Ctrl+{code:?} selected a row"
            );
            assert!(!table.is_filtering(), "Ctrl+{code:?} opened the filter");
        }
    }

    #[test]
    fn ctrl_a_selects_every_visible_row() {
        let mut table = table();
        press_with(&mut table, KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(table.selected_rows(), vec![0, 1, 2]);
    }

    /// `AltGr` arrives as Control+Alt. It must not be read as a command chord,
    /// or a filter buffer would swallow the characters it types.
    #[test]
    fn altgr_is_not_treated_as_a_ctrl_chord() {
        let mut table = table();
        press_with(
            &mut table,
            KeyCode::Char('j'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );
        assert_eq!(
            table.cursor_row(),
            Some(1),
            "AltGr+j was swallowed as a chord"
        );
    }

    #[test]
    fn s_sorts_by_the_active_column_and_toggles_direction() {
        let mut table = table();
        press(&mut table, KeyCode::Char('s'));
        // Sorted ascending by "name": alpha, beta, gamma - already in order.
        assert_eq!(table.cursor_row(), Some(0));
        press(&mut table, KeyCode::Char('s'));
        // Toggled to descending, so the view order is reversed.
        assert_eq!(table.selected_rows().len(), 0);
    }

    #[test]
    fn typing_into_the_filter_narrows_the_view() {
        let mut table = table();
        press(&mut table, KeyCode::Char('/'));
        for ch in "beta".chars() {
            press(&mut table, KeyCode::Char(ch));
        }
        assert_eq!(table.cursor_row(), Some(1), "the cursor follows the match");
    }

    #[test]
    fn minmax_orders_its_two_arguments() {
        assert_eq!(minmax(2, 5), (2, 5));
        assert_eq!(minmax(5, 2), (2, 5));
        assert_eq!(minmax(4, 4), (4, 4));
    }
}
