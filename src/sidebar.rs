//! A sidebar: a sectioned, keyboard-navigable menu column.
//!
//! Sections carry an optional title (a dim, non-selectable header) and a list of
//! selectable items. The sidebar owns the cursor, the vertical/horizontal scroll
//! offsets and an optional `/` fuzzy filter; the caller reads back the selected
//! item's `id` to drive its own view. Labels wider than the column are either
//! clipped with `…` ([`Overflow::Truncate`]) or kept and panned horizontally
//! with a scrollbar ([`Overflow::Scroll`]).

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::{chrome, fuzzy, input, nav, scroll, style, text};
use crate::theme::Skin;

/// Columns the [`Overflow::Scroll`] mode pans per left/right key press.
const H_STEP: usize = 4;

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
    fn matches(&self, item: &SidebarItem) -> bool {
        !self.filter_enabled
            || self.filter.is_empty()
            || fuzzy::score(&item.label, &self.filter).is_some()
    }

    /// The selectable items passing the filter, in section order.
    fn filtered_items(&self) -> impl Iterator<Item = &SidebarItem> {
        self.sections
            .iter()
            .flat_map(|section| section.items.iter())
            .filter(|item| self.matches(item))
    }

    /// The rows to render: section headers interleaved with matching items. A
    /// section contributes its header only when it has at least one match.
    fn rows(&self) -> Vec<Row<'_>> {
        let mut rows = Vec::new();
        for section in &self.sections {
            let mut header_shown = false;
            for item in section.items.iter().filter(|item| self.matches(item)) {
                if !header_shown {
                    if !section.title.is_empty() {
                        rows.push(Row::Header(&section.title));
                    }
                    header_shown = true;
                }
                rows.push(Row::Item(item));
            }
        }
        rows
    }

    /// Renders the panel/box, the filter line (when open) and the list.
    pub fn render(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let inner = self.render_frame(frame, area, skin);
        let list_area = self.render_filter_line(frame, inner, skin);
        self.render_list(frame, list_area, skin);
    }

    /// Draws the surrounding chrome (filled panel by default, or a box) and
    /// returns the inner content area.
    fn render_frame(&self, frame: &mut Frame, area: Rect, skin: &Skin) -> Rect {
        if let Some(decor) = &self.decor {
            let badge = self.filtered_items().count().to_string();
            return chrome::framed_decor(frame, area, skin, decor, &badge);
        }
        let block = chrome::menu_panel(skin);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    }

    /// When filtering, draws the `/query` line on the top row and returns the
    /// remaining list area; otherwise returns `inner` unchanged.
    fn render_filter_line(
        &self,
        frame: &mut Frame,
        inner: Rect,
        skin: &Skin,
    ) -> Rect {
        if !self.filtering {
            return inner;
        }
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(inner);
        let palette = &skin.palette;
        let mut query = vec![Span::styled("/", style::fg(palette.accent))];
        query.extend(input::query_spans(
            &self.filter,
            palette,
            (split[0].width as usize).saturating_sub(1),
        ));
        frame.render_widget(Paragraph::new(Line::from(query)), split[0]);
        split[1]
    }

    /// Draws the rows, the vertical scrollbar and (in scroll mode, on overflow)
    /// the horizontal scrollbar, keeping the cursor row visible.
    fn render_list(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let rows = self.rows();
        let max_width = rows.iter().map(Self::row_width).max().unwrap_or(0);
        // A vertical scrollbar claims the rightmost column when the rows
        // overflow the height, so reserve it: labels then clip before the bar
        // instead of underneath it. An hbar only shrinks the height further, so
        // measuring against the full area height keeps this decision simple.
        let has_scrollbar = rows.len() > area.height as usize;
        let content_width =
            (area.width as usize).saturating_sub(usize::from(has_scrollbar));

        let overflowing =
            self.overflow == Overflow::Scroll && max_width > content_width;
        let (body, hbar) = split_for_hbar(area, overflowing);

        let height = body.height as usize;
        self.viewport.set(height.max(1));
        let selected_row = self.selected_row(&rows);
        let offset = self.scroll_offset(&rows, selected_row, height);
        self.offset.set(offset);
        let h_offset = self.clamp_h_offset(max_width, content_width);

        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(height)
            .map(|(index, row)| {
                self.render_row(
                    row,
                    index == selected_row,
                    content_width,
                    h_offset,
                    skin,
                )
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), body);

        scroll::render_scrollbar(
            frame,
            body,
            skin,
            nav::ScrollView {
                total: rows.len(),
                offset,
                viewport: height,
            },
        );
        if let Some(hbar) = hbar {
            scroll::render_hscrollbar(
                frame,
                hbar,
                skin,
                nav::ScrollView {
                    total: max_width,
                    offset: h_offset,
                    viewport: content_width,
                },
            );
        }
    }

    /// Builds one styled line: a dim header, or an item with the selection
    /// pointer and (when selected) an accent, full-width tinted bar.
    fn render_row(
        &self,
        row: &Row,
        selected: bool,
        width: usize,
        h_offset: usize,
        skin: &Skin,
    ) -> Line<'static> {
        let palette = &skin.palette;
        match row {
            Row::Header(title) => Line::from(Span::styled(
                self.clip(title, h_offset, width),
                style::dim().add_modifier(Modifier::BOLD),
            )),
            Row::Item(item) => {
                let pointer = if selected { skin.glyphs.pointer } else { " " };
                let full = format!("{pointer} {}", item.label);
                let clipped = self.clip(&full, h_offset, width);
                if selected {
                    let bar = text::pad_end(&clipped, width);
                    let style = style::fg(palette.accent)
                        .add_modifier(Modifier::BOLD)
                        .bg(style::to_ratatui(palette.selection));
                    Line::from(Span::styled(bar, style))
                } else {
                    Line::from(Span::styled(clipped, Style::default()))
                }
            }
        }
    }

    /// Clips `text` to `width` columns per the overflow mode.
    fn clip(&self, text: &str, h_offset: usize, width: usize) -> String {
        match self.overflow {
            Overflow::Truncate => text::truncate(text, width),
            Overflow::Scroll => text::window(text, h_offset, width),
        }
    }

    /// The row index of the currently selected item (0 when there is none).
    fn selected_row(&self, rows: &[Row]) -> usize {
        let mut item_index = 0;
        for (row_index, row) in rows.iter().enumerate() {
            if let Row::Item(_) = row {
                if item_index == self.selected {
                    return row_index;
                }
                item_index += 1;
            }
        }
        0
    }

    /// The vertical scroll offset keeping the selected item visible. When
    /// scrolling up it reveals the item's section header too, so the first item
    /// sits at the very top (offset 0) and the header stays in view - otherwise
    /// `keep_visible` would stop at the item and clip the header above it.
    fn scroll_offset(
        &self,
        rows: &[Row],
        selected_row: usize,
        height: usize,
    ) -> usize {
        if height == 0 || rows.is_empty() {
            return 0;
        }
        let max_offset = rows.len().saturating_sub(height);
        let anchor = section_header_row(rows, selected_row);
        let mut offset = self.offset.get().min(max_offset);
        if anchor < offset {
            offset = anchor;
        }
        if selected_row >= offset + height {
            offset = selected_row + 1 - height;
        }
        offset.min(max_offset)
    }

    /// Clamps and stores the horizontal offset against the widest row, returning
    /// the value to render (always 0 outside scroll mode).
    fn clamp_h_offset(&self, max_width: usize, inner_width: usize) -> usize {
        if self.overflow != Overflow::Scroll {
            return 0;
        }
        let max_offset = max_width.saturating_sub(inner_width);
        let clamped = self.h_offset.get().min(max_offset);
        self.h_offset.set(clamped);
        clamped
    }

    /// The display width a row needs, including the two-column item prefix.
    fn row_width(row: &Row) -> usize {
        match row {
            Row::Header(title) => title.width(),
            Row::Item(item) => item.label.width() + 2,
        }
    }
}

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
    use crossterm::event::KeyModifiers;

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
