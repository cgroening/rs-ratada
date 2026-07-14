//! Fuzzy command palette overlay: pick a command and run it.
//!
//! Like the [`help`](super::help) overlay it groups commands into titled
//! sections and filters them fuzzily, but it returns the chosen command so the
//! caller can execute it. With an empty query the commands stay grouped under
//! their section headers; as soon as the user types, the list flattens and
//! re-sorts by match score so the best hit is first. Commands whose `enabled`
//! flag is `false` render dimmed and cannot be selected.

use std::{cell::Cell, io};

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    chrome, fuzzy, input,
    layout::{centered_rect, fit},
    list,
    modal::ModalSignal,
    nav,
    overlay::{self, PopupFlow, popup_with_paste},
    shortcut_hints, style,
    terminal::Tui,
};
use crate::theme::{Palette, Skin};

/// The width of the category column shown while searching.
const CATEGORY_WIDTH: usize = 12;

/// The prefix of the query line; its width is taken off the caret line's.
const SEARCH_LABEL: &str = "search ";
/// The width of the command-label column before the key hint.
const LABEL_WIDTH: usize = 16;

/// One selectable command in the palette.
pub struct CommandItem<'a> {
    /// The command name shown to the user, e.g. `"add task"`.
    pub label: &'a str,
    /// The section this command is grouped under, e.g. `"Tasks"`.
    pub category: &'a str,
    /// The keys bound to the command, e.g. `"a"` (may be empty).
    pub key_hint: &'a str,
    /// Whether the command can run now; a disabled command renders dimmed and
    /// is not selectable.
    pub enabled: bool,
}

/// The search state of the palette.
struct PaletteState {
    query: String,
    /// Index into the currently selectable (enabled item) rows.
    cursor: usize,
    /// Persistent list scroll offset so the view and scrollbar follow the
    /// cursor across frames.
    offset: Cell<usize>,
    /// The list viewport height captured at render, driving page jumps.
    viewport: Cell<usize>,
}

/// One rendered row: a section header or a command.
enum Row<'a> {
    Header(&'a str),
    Item {
        item: &'a CommandItem<'a>,
        /// The command's original index into the caller's `items`.
        index: usize,
    },
}

/// The rows to render plus the navigation index maps for the current query.
struct RowLayout<'a> {
    rows: Vec<Row<'a>>,
    /// Row index of each selectable (enabled) item, in display order.
    selectable: Vec<usize>,
    /// Position within `selectable` of each section's first selectable item.
    /// Empty while searching (the flat list has no sections).
    section_starts: Vec<usize>,
}

/// Shows the command palette until the user runs a command or cancels.
///
/// `Enter` runs the highlighted command and returns its index into `items`;
/// `Esc` cancels. Typing filters the commands fuzzily and re-sorts them by
/// score; with an empty query they stay grouped and `Tab`/`BackTab` jump
/// between sections. An empty `items` cancels immediately.
pub fn command_palette(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[CommandItem<'_>],
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<usize>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut state = PaletteState {
        query: String::new(),
        cursor: 0,
        offset: Cell::new(0),
        viewport: Cell::new(1),
    };
    popup_with_paste(
        tui,
        &mut state,
        |area, _| {
            centered_rect(
                fit(area.width * 2 / 3, 40, area.width),
                fit(area.height * 2 / 3, 8, area.height),
                area,
            )
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &PaletteState| {
            let inner = overlay::framed(frame, rect, skin, title);
            render_body(frame, inner, skin, items, state);
            // Section headers are rows but not positions: the badge counts the
            // selectable commands.
            let count = layout_rows(items, &state.query).selectable.len();
            let cursor = state.cursor.min(count.saturating_sub(1));
            let badge = chrome::position_badge(cursor, count);
            chrome::render_badge(frame, rect, skin, &badge);
        },
        |state, key| match key.code {
            KeyCode::Esc => PopupFlow::Cancelled,
            KeyCode::Enter => {
                let layout = layout_rows(items, &state.query);
                match selected_index(&layout, state.cursor) {
                    Some(index) => PopupFlow::Done(index),
                    None => PopupFlow::Continue,
                }
            }
            KeyCode::Up => {
                let count = layout_rows(items, &state.query).selectable.len();
                state.cursor = nav::cycle(state.cursor, count, -1);
                PopupFlow::Continue
            }
            KeyCode::Down => {
                let count = layout_rows(items, &state.query).selectable.len();
                state.cursor = nav::cycle(state.cursor, count, 1);
                PopupFlow::Continue
            }
            KeyCode::PageUp => {
                let count = layout_rows(items, &state.query).selectable.len();
                let page = state.viewport.get().max(1) as isize;
                state.cursor = nav::step_clamped(state.cursor, count, -page);
                PopupFlow::Continue
            }
            KeyCode::PageDown => {
                let count = layout_rows(items, &state.query).selectable.len();
                let page = state.viewport.get().max(1) as isize;
                state.cursor = nav::step_clamped(state.cursor, count, page);
                PopupFlow::Continue
            }
            KeyCode::Home => {
                state.cursor = 0;
                PopupFlow::Continue
            }
            KeyCode::End => {
                let count = layout_rows(items, &state.query).selectable.len();
                state.cursor = count.saturating_sub(1);
                PopupFlow::Continue
            }
            KeyCode::Tab => {
                jump_section(state, items, 1);
                PopupFlow::Continue
            }
            KeyCode::BackTab => {
                jump_section(state, items, -1);
                PopupFlow::Continue
            }
            KeyCode::Backspace => {
                state.query.pop();
                state.cursor = 0;
                PopupFlow::Continue
            }
            KeyCode::Char(ch) => {
                state.query.push(ch);
                state.cursor = 0;
                PopupFlow::Continue
            }
            _ => PopupFlow::Continue,
        },
        |state, text| {
            state
                .query
                .extend(text.chars().filter(|ch| !ch.is_control()));
            state.cursor = 0;
            PopupFlow::Continue
        },
    )
}

/// The original index of the command the cursor sits on, if any.
fn selected_index(layout: &RowLayout, cursor: usize) -> Option<usize> {
    let row = *layout.selectable.get(cursor)?;
    match layout.rows[row] {
        Row::Item { index, .. } => Some(index),
        Row::Header(_) => None,
    }
}

/// Moves the cursor to the first item of the next (`+1`) or previous (`-1`)
/// section, wrapping around. A no-op while searching (no sections).
fn jump_section(
    state: &mut PaletteState,
    items: &[CommandItem<'_>],
    direction: isize,
) {
    let starts = layout_rows(items, &state.query).section_starts;
    if starts.is_empty() {
        return;
    }
    // The section the cursor is currently in: the last start at or before it.
    let current = starts
        .iter()
        .rposition(|&start| start <= state.cursor)
        .unwrap_or(0);
    let next = nav::cycle(current, starts.len(), direction);
    state.cursor = starts[next];
}

/// Builds the rows for `items`: grouped under section headers when `query` is
/// empty, otherwise a flat list ranked by fuzzy score.
fn layout_rows<'a>(items: &'a [CommandItem<'a>], query: &str) -> RowLayout<'a> {
    if query.trim().is_empty() {
        grouped_rows(items)
    } else {
        ranked_rows(items, query.trim())
    }
}

/// Groups `items` under a header per category, preserving their given order.
/// Only enabled items are selectable; disabled ones still render (dimmed).
fn grouped_rows<'a>(items: &'a [CommandItem<'a>]) -> RowLayout<'a> {
    let mut rows: Vec<Row<'a>> = Vec::new();
    let mut selectable: Vec<usize> = Vec::new();
    let mut section_starts: Vec<usize> = Vec::new();
    let mut current_category: Option<&str> = None;
    let mut section_has_selectable = false;

    for (index, item) in items.iter().enumerate() {
        if current_category != Some(item.category) {
            rows.push(Row::Header(item.category));
            current_category = Some(item.category);
            section_has_selectable = false;
        }
        if item.enabled {
            if !section_has_selectable {
                section_starts.push(selectable.len());
                section_has_selectable = true;
            }
            selectable.push(rows.len());
        }
        rows.push(Row::Item { item, index });
    }
    RowLayout {
        rows,
        selectable,
        section_starts,
    }
}

/// Filters `items` to those matching `query` and orders them by score, best
/// first. Only enabled items are selectable; disabled matches still render.
fn ranked_rows<'a>(items: &'a [CommandItem<'a>], query: &str) -> RowLayout<'a> {
    let mut scored: Vec<(u32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let haystack = format!("{} {}", item.category, item.label);
            fuzzy::score(&haystack, query).map(|score| (score, index))
        })
        .collect();
    scored.sort_by(|left, right| right.0.cmp(&left.0));

    let mut rows: Vec<Row<'a>> = Vec::new();
    let mut selectable: Vec<usize> = Vec::new();
    for (_, index) in scored {
        let item = &items[index];
        if item.enabled {
            selectable.push(rows.len());
        }
        rows.push(Row::Item { item, index });
    }
    RowLayout {
        rows,
        selectable,
        section_starts: Vec::new(),
    }
}

fn render_body(
    frame: &mut Frame,
    inner: Rect,
    skin: &Skin,
    items: &[CommandItem<'_>],
    state: &PaletteState,
) {
    let palette = &skin.palette;
    let grouped = state.query.trim().is_empty();
    let layout = layout_rows(items, &state.query);

    // The footer collapses to nothing while the hints are hidden, handing its
    // row to the list.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(shortcut_hints::footer_height(1)),
        ])
        .split(inner);

    let mut search =
        vec![Span::styled(SEARCH_LABEL, style::secondary(palette))];
    search.extend(input::query_spans(
        &state.query,
        palette,
        (rows[0].width as usize).saturating_sub(SEARCH_LABEL.len()),
    ));
    frame.render_widget(Paragraph::new(Line::from(search)), rows[0]);

    let header_style =
        style::fg(palette.accent_dim).add_modifier(Modifier::BOLD);
    let entries: Vec<Line<'static>> = layout
        .rows
        .iter()
        .map(|row| match row {
            Row::Header(title) => {
                Line::from(Span::styled(title.to_uppercase(), header_style))
            }
            Row::Item { item, .. } => {
                item_line(item, &state.query, palette, grouped)
            }
        })
        .collect();

    let selected = layout
        .selectable
        .get(state.cursor.min(layout.selectable.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0);
    let viewport = list::render(
        frame,
        rows[1],
        skin,
        list::ListView {
            rows: entries,
            selected,
            offset: &state.offset,
        },
    );
    state.viewport.set(viewport);

    let hint = footer_hint(skin, rows[2].width as usize, grouped);
    frame.render_widget(Paragraph::new(hint), rows[2]);
}

/// Renders one command row. Enabled commands show the label (with fuzzy match
/// highlights) and an accented key hint; disabled ones are wholly dimmed.
fn item_line(
    item: &CommandItem<'_>,
    query: &str,
    palette: &Palette,
    grouped: bool,
) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    if !grouped {
        spans.push(Span::styled(
            format!("{:<CATEGORY_WIDTH$}", item.category),
            style::secondary(palette),
        ));
    }
    let label = format!("{:<LABEL_WIDTH$}", item.label);
    if item.enabled {
        spans.extend(fuzzy::highlight(
            &label,
            query,
            style::secondary(palette),
            palette,
        ));
        spans.push(Span::styled(
            item.key_hint.to_string(),
            style::fg(palette.accent).add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(label, style::secondary(palette)));
        spans.push(Span::styled(
            item.key_hint.to_string(),
            style::secondary(palette),
        ));
    }
    Line::from(spans)
}

/// The footer hint line: adds `tab section` only while grouped.
fn footer_hint(skin: &Skin, width: usize, grouped: bool) -> Line<'static> {
    let mut hints: Vec<(&str, &str)> =
        vec![("\u{2191}\u{2193}", "move"), ("enter", "run")];
    if grouped {
        hints.push(("tab", "section"));
    }
    hints.push(("esc", "close"));
    shortcut_hints::lines(&hints, skin.palette.accent_dim, width)
        .into_iter()
        .next()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<CommandItem<'static>> {
        vec![
            CommandItem {
                label: "add task",
                category: "Tasks",
                key_hint: "a",
                enabled: true,
            },
            CommandItem {
                label: "delete",
                category: "Tasks",
                key_hint: "d",
                enabled: false,
            },
            CommandItem {
                label: "view summary",
                category: "Views",
                key_hint: "2",
                enabled: true,
            },
        ]
    }

    #[test]
    fn empty_query_groups_and_lists_only_enabled_as_selectable() {
        let items = items();
        let layout = layout_rows(&items, "");
        // Header(Tasks), add, delete, Header(Views), view summary = 5 rows.
        assert_eq!(layout.rows.len(), 5);
        // Only the two enabled items are selectable (delete is disabled).
        assert_eq!(layout.selectable, vec![1, 4]);
        // One section start per section that has a selectable item.
        assert_eq!(layout.section_starts, vec![0, 1]);
    }

    #[test]
    fn disabled_item_is_rendered_but_not_selectable() {
        let items = items();
        let layout = layout_rows(&items, "");
        // The disabled "delete" sits at row 2 but never in `selectable`.
        assert!(matches!(layout.rows[2], Row::Item { index: 1, .. }));
        assert!(!layout.selectable.contains(&2));
    }

    #[test]
    fn query_flattens_ranks_and_drops_headers() {
        let items = items();
        let layout = layout_rows(&items, "task");
        // No headers in the flat list, no sections to jump between.
        assert!(layout.rows.iter().all(|r| matches!(r, Row::Item { .. })));
        assert!(layout.section_starts.is_empty());
        // "add task" is the only enabled match, so it is the sole selection.
        assert_eq!(layout.selectable.len(), 1);
        assert_eq!(selected_index(&layout, 0), Some(0));
    }

    #[test]
    fn non_matching_query_leaves_nothing_selectable() {
        let items = items();
        let layout = layout_rows(&items, "zzzzz");
        assert!(layout.selectable.is_empty());
        assert_eq!(selected_index(&layout, 0), None);
    }
}
