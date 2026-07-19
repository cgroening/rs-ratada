//! Sidebar key dispatch: moving the selection, driving the filter and the
//! horizontal panning of the scroll overflow mode.

use crossterm::event::{KeyCode, KeyEvent};

use super::{H_STEP, Overflow, Sidebar, SidebarItem, SidebarOutcome};
use crate::{fuzzy, input, nav};

impl Sidebar {
    /// Handles a key, reporting whether it activated an item, was consumed or is
    /// irrelevant. Navigation, panning and the `/` filter are consumed; `Enter`
    /// on an item activates it; everything else is ignored so the caller can act.
    ///
    /// The bindings are bare keys only: a Ctrl chord is reported `Ignored`, so
    /// a caller stays free to bind `Ctrl+<key>` itself.
    pub fn handle_key(&mut self, key: KeyEvent) -> SidebarOutcome {
        if self.filtering {
            self.handle_filter_key(key);
            return SidebarOutcome::Consumed;
        }
        // The sidebar binds no chord of its own, and crossterm reports Ctrl+J
        // as `Char('j')` and Ctrl+H as `Char('h')` - without this they would
        // move the cursor and pan the labels.
        if input::is_command(key) {
            return SidebarOutcome::Ignored;
        }
        let len = self.filtered_items().count();
        let page = self.viewport.get().max(1) as isize;
        let scroll = self.overflow == Overflow::Scroll;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = nav::cycle(self.selected, len, -1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = nav::cycle(self.selected, len, 1);
            }
            KeyCode::PageUp => {
                self.selected = nav::step_clamped(self.selected, len, -page);
            }
            KeyCode::PageDown => {
                self.selected = nav::step_clamped(self.selected, len, page);
            }
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => {
                self.selected = len.saturating_sub(1);
            }
            KeyCode::Left | KeyCode::Char('h') if scroll => {
                self.h_offset
                    .set(self.h_offset.get().saturating_sub(H_STEP));
            }
            KeyCode::Right | KeyCode::Char('l') if scroll => {
                self.h_offset.set(self.h_offset.get() + H_STEP);
            }
            KeyCode::Char('/') if self.filter_enabled => self.filtering = true,
            KeyCode::Enter => return SidebarOutcome::Activated,
            _ => return SidebarOutcome::Ignored,
        }
        SidebarOutcome::Consumed
    }

    /// Edits the filter query; any change resets the cursor to the first match.
    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filtering = false;
                self.filter.clear();
                self.selected = 0;
            }
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                self.filter.pop();
                self.selected = 0;
            }
            // Anything that is not a command chord is filter text: Ctrl+U
            // would otherwise append a `u` instead of being left as an editing
            // key. Deliberately not `is_bare_character`, which would also
            // reject AltGr - `@`, `\` and `[` must reach the filter, exactly as
            // in `input::apply_edit_key`.
            KeyCode::Char(ch) if !input::is_command(key) => {
                self.filter.push(ch);
                self.selected = 0;
            }
            _ => {}
        }
    }

    /// Whether `item` passes the active filter (always true without a filter).
    pub(super) fn matches(&self, item: &SidebarItem) -> bool {
        !self.filter_enabled
            || self.filter.is_empty()
            || fuzzy::score(&item.label, &self.filter).is_some()
    }

    /// The selectable items passing the filter, in section order.
    pub(super) fn filtered_items(&self) -> impl Iterator<Item = &SidebarItem> {
        self.sections
            .iter()
            .flat_map(|section| section.items.iter())
            .filter(|item| self.matches(item))
    }
}
