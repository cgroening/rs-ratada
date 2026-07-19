//! A sidebar: a sectioned, keyboard-navigable menu column.
//!
//! Sections carry an optional title (a dim, non-selectable header) and a list of
//! selectable items. The sidebar owns the cursor, the vertical/horizontal scroll
//! offsets and an optional `/` fuzzy filter; the caller reads back the selected
//! item's `id` to drive its own view. Labels wider than the column are either
//! clipped with `…` ([`Overflow::Truncate`]) or kept and panned horizontally
//! with a scrollbar ([`Overflow::Scroll`]).

mod interaction;
mod layout;
mod render;

use std::cell::Cell;

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use super::chrome;

/// A selectable item: a display `label` and an opaque `id` the caller uses to map
/// the selection back to its own data.
#[derive(Debug, Clone)]
pub struct SidebarItem {
    /// The text shown for the item.
    pub label: String,
    /// The caller's identifier, reported back on selection.
    pub id: usize,
}

impl SidebarItem {
    /// An item showing `label`, tagged with the caller's `id`.
    pub fn new(label: impl Into<String>, id: usize) -> Self {
        Self {
            label: label.into(),
            id,
        }
    }
}

/// A named group of items. An empty `title` renders no header row.
#[derive(Debug, Clone)]
pub struct SidebarSection {
    /// The header text; an empty title renders no header row.
    pub title: String,
    /// The items in this section.
    pub items: Vec<SidebarItem>,
}

impl SidebarSection {
    /// A section titled `title` holding `items`.
    pub fn new(title: impl Into<String>, items: Vec<SidebarItem>) -> Self {
        Self {
            title: title.into(),
            items,
        }
    }
}

/// How labels wider than the column are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow {
    /// Clip with a trailing `…`.
    #[default]
    Truncate,
    /// Keep the full label; pan horizontally with left/right and show a
    /// horizontal scrollbar when content overflows.
    Scroll,
}

/// What a key press meant to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarOutcome {
    /// The selected item was activated (Enter).
    Activated,
    /// The key drove the sidebar (navigation, filtering, panning).
    Consumed,
    /// The key was not relevant to the sidebar.
    Ignored,
}

/// One rendered row: a section header or a selectable item.
enum Row<'a> {
    Header(&'a str),
    Item(&'a SidebarItem),
}

/// A sectioned menu column over owned [`SidebarSection`]s.
pub struct Sidebar {
    sections: Vec<SidebarSection>,
    selected: usize,
    offset: Cell<usize>,
    viewport: Cell<usize>,
    h_offset: Cell<usize>,
    overflow: Overflow,
    filter_enabled: bool,
    filtering: bool,
    filter: String,
    decor: Option<chrome::BoxDecor>,
}

impl Sidebar {
    /// Builds a sidebar over `sections`, cursor on the first item, no filter,
    /// [`Overflow::Truncate`].
    pub fn new(sections: Vec<SidebarSection>) -> Self {
        Self {
            sections,
            selected: 0,
            offset: Cell::new(0),
            viewport: Cell::new(1),
            h_offset: Cell::new(0),
            overflow: Overflow::Truncate,
            filter_enabled: false,
            filtering: false,
            filter: String::new(),
            decor: None,
        }
    }

    /// Sets how over-wide labels are handled.
    #[must_use]
    pub fn overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = overflow;
        self
    }

    /// Enables the built-in `/` fuzzy filter over item labels.
    #[must_use]
    pub fn filterable(mut self) -> Self {
        self.filter_enabled = true;
        self
    }

    /// Draws the sidebar inside a rounded box (see [`chrome::BoxDecor`]) instead
    /// of the default filled panel column.
    #[must_use]
    pub fn boxed(mut self, decor: chrome::BoxDecor) -> Self {
        self.decor = Some(decor);
        self
    }

    /// Whether the filter input is currently open.
    pub fn is_filtering(&self) -> bool {
        self.filtering
    }

    /// The `id` of the selected item, if any (none when the filter excludes all).
    pub fn selected_id(&self) -> Option<usize> {
        self.filtered_items().nth(self.selected).map(|item| item.id)
    }

    /// The label of the selected item, if any.
    pub fn selected_label(&self) -> Option<&str> {
        self.filtered_items()
            .nth(self.selected)
            .map(|item| item.label.as_str())
    }
}

/// Columns the [`Overflow::Scroll`] mode pans per left/right key press.
const H_STEP: usize = 4;

/// The row index of the header for the section containing `selected_row`, or
/// `selected_row` itself when no header sits above it (e.g. an untitled section).
fn section_header_row(rows: &[Row], selected_row: usize) -> usize {
    let last = selected_row.min(rows.len().saturating_sub(1));
    (0..=last)
        .rev()
        .find(|&index| matches!(rows[index], Row::Header(_)))
        .unwrap_or(selected_row)
}

/// Splits `area` into a body and an optional bottom row for the horizontal
/// scrollbar.
fn split_for_hbar(area: Rect, overflowing: bool) -> (Rect, Option<Rect>) {
    if !overflowing {
        return (area, None);
    }
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    (split[0], Some(split[1]))
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn sample() -> Sidebar {
        Sidebar::new(vec![
            SidebarSection::new(
                "FRUIT",
                vec![
                    SidebarItem::new("Apple", 10),
                    SidebarItem::new("Banana", 11),
                ],
            ),
            SidebarSection::new("VEG", vec![SidebarItem::new("Carrot", 20)]),
        ])
    }

    #[test]
    fn starts_on_the_first_item() {
        let sidebar = sample();
        assert_eq!(sidebar.selected_id(), Some(10));
        assert_eq!(sidebar.selected_label(), Some("Apple"));
    }

    #[test]
    fn selecting_the_first_item_scrolls_to_the_top() {
        // Rows: [Header FRUIT, Apple, Banana, Header VEG, Carrot] = 5 rows.
        let sidebar = sample();
        let rows = sidebar.rows();
        // Pretend we had scrolled down before returning to the first item.
        sidebar.offset.set(1);
        let selected_row = sidebar.selected_row(&rows); // first item -> row 1
        let offset = sidebar.scroll_offset(&rows, selected_row, 4);
        // The header (row 0) is revealed, so the offset is the very top.
        assert_eq!(offset, 0);
    }

    #[test]
    fn section_header_row_finds_the_header_above() {
        let sidebar = sample();
        let rows = sidebar.rows();
        assert_eq!(section_header_row(&rows, 1), 0); // Apple -> FRUIT header
        assert_eq!(section_header_row(&rows, 4), 3); // Carrot -> VEG header
    }

    #[test]
    fn down_advances_across_sections_skipping_headers() {
        let mut sidebar = sample();
        sidebar.handle_key(key(KeyCode::Down));
        sidebar.handle_key(key(KeyCode::Down));
        // Third item lives under the second section; headers are not selectable.
        assert_eq!(sidebar.selected_id(), Some(20));
    }

    #[test]
    fn up_from_the_top_wraps_to_the_last_item() {
        let mut sidebar = sample();
        sidebar.handle_key(key(KeyCode::Up));
        assert_eq!(sidebar.selected_id(), Some(20));
    }

    #[test]
    fn enter_activates_the_selected_item() {
        let mut sidebar = sample();
        assert_eq!(
            sidebar.handle_key(key(KeyCode::Enter)),
            SidebarOutcome::Activated,
        );
    }

    #[test]
    fn unrelated_keys_are_ignored() {
        let mut sidebar = sample();
        assert_eq!(
            sidebar.handle_key(key(KeyCode::Char('x'))),
            SidebarOutcome::Ignored,
        );
    }

    #[test]
    fn filter_narrows_matches_and_resets_the_cursor() {
        let mut sidebar = sample().filterable();
        sidebar.handle_key(key(KeyCode::Down)); // move off the first item
        sidebar.handle_key(key(KeyCode::Char('/')));
        assert!(sidebar.is_filtering());
        for ch in "carr".chars() {
            sidebar.handle_key(key(KeyCode::Char(ch)));
        }
        assert_eq!(sidebar.selected_id(), Some(20));
    }

    #[test]
    fn filter_excluding_everything_yields_no_selection() {
        let mut sidebar = sample().filterable();
        sidebar.handle_key(key(KeyCode::Char('/')));
        for ch in "zzzz".chars() {
            sidebar.handle_key(key(KeyCode::Char(ch)));
        }
        assert_eq!(sidebar.selected_id(), None);
    }

    #[test]
    fn escape_clears_the_filter() {
        let mut sidebar = sample().filterable();
        sidebar.handle_key(key(KeyCode::Char('/')));
        sidebar.handle_key(key(KeyCode::Char('z')));
        sidebar.handle_key(key(KeyCode::Esc));
        assert!(!sidebar.is_filtering());
        assert_eq!(sidebar.selected_id(), Some(10));
    }

    #[test]
    fn slash_does_nothing_without_a_filter() {
        let mut sidebar = sample();
        assert_eq!(
            sidebar.handle_key(key(KeyCode::Char('/'))),
            SidebarOutcome::Ignored,
        );
        assert!(!sidebar.is_filtering());
    }

    #[test]
    fn ctrl_chords_do_not_navigate() {
        let mut sidebar = sample().overflow(Overflow::Scroll);
        // crossterm reports Ctrl+J as `Char('j')`; reporting the chord Ignored
        // leaves the caller free to bind it.
        assert_eq!(
            sidebar.handle_key(ctrl(KeyCode::Char('j'))),
            SidebarOutcome::Ignored,
        );
        assert_eq!(sidebar.selected_id(), Some(10));
        // Ctrl+L must not pan the labels either.
        sidebar.handle_key(ctrl(KeyCode::Char('l')));
        assert_eq!(sidebar.h_offset.get(), 0);
    }

    #[test]
    fn ctrl_chords_are_not_typed_into_the_filter() {
        let mut sidebar = sample().filterable();
        sidebar.handle_key(key(KeyCode::Char('/')));
        sidebar.handle_key(ctrl(KeyCode::Char('u')));
        assert!(sidebar.filter.is_empty());
        // A bare character still types.
        sidebar.handle_key(key(KeyCode::Char('u')));
        assert_eq!(sidebar.filter, "u");
    }

    /// The other half of the rule: `AltGr` is reported as `Ctrl+Alt` yet types
    /// a real character, so the filter must accept it. Guarding that arm with
    /// `is_bare_character` instead of `!is_command` would make `@`, `\` and `[`
    /// untypeable on a German keyboard.
    #[test]
    fn altgr_characters_still_reach_the_filter() {
        let mut sidebar = sample().filterable();
        sidebar.handle_key(key(KeyCode::Char('/')));
        for ch in ['@', '\\', '['] {
            sidebar.handle_key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ));
        }
        assert_eq!(sidebar.filter, "@\\[");
    }

    #[test]
    fn panning_only_applies_in_scroll_mode() {
        let mut truncate = sample();
        assert_eq!(
            truncate.handle_key(key(KeyCode::Right)),
            SidebarOutcome::Ignored,
        );

        let mut scroll = sample().overflow(Overflow::Scroll);
        assert_eq!(
            scroll.handle_key(key(KeyCode::Right)),
            SidebarOutcome::Consumed,
        );
        assert_eq!(scroll.h_offset.get(), H_STEP);
        // Left never pans past the origin.
        scroll.handle_key(key(KeyCode::Left));
        scroll.handle_key(key(KeyCode::Left));
        assert_eq!(scroll.h_offset.get(), 0);
    }
}
