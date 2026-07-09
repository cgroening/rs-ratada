//! Scrollable, fuzzy-searchable help overlay listing key bindings in sections.
//!
//! A thin wrapper over [`overlay::popup`]: the dimmed backdrop, box and loop
//! come from there; this module owns the search state and the sectioned body.
//! Bindings are grouped into [`HelpSection`]s; `Tab`/`BackTab` jump between
//! sections, the arrows move within the flat list, and typing filters fuzzily
//! while keeping the section headers of any section that still has a match.

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
    layout::centered_fraction,
    list,
    modal::ModalSignal,
    nav,
    overlay::{self, PopupFlow, popup},
    shortcut_hints, style,
    terminal::Tui,
};
use crate::theme::Skin;

/// The prefix of the query line; its width is taken off the caret line's.
const SEARCH_LABEL: &str = "search ";

/// A titled group of key bindings shown under one header in the overlay.
pub struct HelpSection<'a, B: AsRef<str>> {
    /// The section header.
    pub title: &'a str,
    /// The `(key, description)` bindings listed under the header.
    pub bindings: &'a [(B, B)],
}

/// The search state of the help overlay.
struct Help {
    query: String,
    /// Index into the currently selectable (item) rows.
    cursor: usize,
    /// Persistent list scroll offset so the view and scrollbar follow the
    /// cursor across frames.
    offset: Cell<usize>,
}

/// One rendered row: a section header or a selectable binding.
enum Row<'a> {
    Header(&'a str),
    Item { key: &'a str, description: &'a str },
}

/// The rows to render plus the navigation index maps for the current query.
struct RowLayout<'a> {
    rows: Vec<Row<'a>>,
    /// Row index of each selectable item, in order.
    selectable: Vec<usize>,
    /// Position within `selectable` of each visible section's first item.
    section_starts: Vec<usize>,
}

/// Shows the help overlay until the user closes it.
///
/// A query filters the bindings fuzzily (keeping the header of every section
/// that still matches); the arrow keys move the selection, `Tab`/`BackTab` jump
/// to the next/previous section, and `Esc` or `?` close the overlay.
pub fn show<B: AsRef<str>>(
    tui: &mut Tui,
    skin: &Skin,
    sections: &[HelpSection<'_, B>],
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<()>> {
    let mut state = Help {
        query: String::new(),
        cursor: 0,
        offset: Cell::new(0),
    };
    popup(
        tui,
        &mut state,
        |area, _| centered_fraction(area, 2, 3, 40, 8),
        |frame, _| render_bg(frame),
        |frame, rect, state: &Help| {
            let inner = overlay::framed(frame, rect, skin, "Help");
            render_body(frame, inner, skin, sections, state);
            // Section headers are rows but not positions: the badge counts the
            // selectable bindings.
            let count = layout_rows(sections, &state.query).selectable.len();
            let cursor = state.cursor.min(count.saturating_sub(1));
            let badge = chrome::position_badge(cursor, count);
            chrome::render_badge(frame, rect, skin, &badge);
        },
        |state, key| match key.code {
            KeyCode::Esc | KeyCode::Char('?') => PopupFlow::Done(()),
            KeyCode::Up => {
                let count =
                    layout_rows(sections, &state.query).selectable.len();
                state.cursor = nav::cycle(state.cursor, count, -1);
                PopupFlow::Continue
            }
            KeyCode::Down => {
                let count =
                    layout_rows(sections, &state.query).selectable.len();
                state.cursor = nav::cycle(state.cursor, count, 1);
                PopupFlow::Continue
            }
            KeyCode::Tab => {
                jump_section(state, sections, 1);
                PopupFlow::Continue
            }
            KeyCode::BackTab => {
                jump_section(state, sections, -1);
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
    )
}

/// Moves the cursor to the first item of the next (`+1`) or previous (`-1`)
/// visible section, wrapping around.
fn jump_section<B: AsRef<str>>(
    state: &mut Help,
    sections: &[HelpSection<'_, B>],
    direction: isize,
) {
    let starts = layout_rows(sections, &state.query).section_starts;
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

/// Builds the rows and navigation index maps for `sections` filtered by `query`.
/// A section is included only if it keeps at least one matching binding; the
/// bindings stay in their original order (no score re-sorting).
fn layout_rows<'a, B: AsRef<str>>(
    sections: &'a [HelpSection<'a, B>],
    query: &str,
) -> RowLayout<'a> {
    let query = query.trim();
    let mut rows: Vec<Row<'a>> = Vec::new();
    let mut selectable: Vec<usize> = Vec::new();
    let mut section_starts: Vec<usize> = Vec::new();

    for section in sections {
        let matches: Vec<&(B, B)> = section
            .bindings
            .iter()
            .filter(|(key, description)| {
                is_match(key.as_ref(), description.as_ref(), query)
            })
            .collect();
        if matches.is_empty() {
            continue;
        }
        section_starts.push(selectable.len());
        rows.push(Row::Header(section.title));
        for (key, description) in matches {
            selectable.push(rows.len());
            rows.push(Row::Item {
                key: key.as_ref(),
                description: description.as_ref(),
            });
        }
    }
    RowLayout {
        rows,
        selectable,
        section_starts,
    }
}

/// Whether a binding matches `query` (everything matches an empty query).
fn is_match(key: &str, description: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    fuzzy::score(&format!("{key} {description}"), query).is_some()
}

fn render_body<B: AsRef<str>>(
    frame: &mut Frame,
    inner: Rect,
    skin: &Skin,
    sections: &[HelpSection<'_, B>],
    state: &Help,
) {
    let palette = &skin.palette;
    let layout = layout_rows(sections, &state.query);

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
            Row::Item { key, description } => {
                let mut spans = vec![Span::styled(
                    format!("  {key:<12}"),
                    style::fg(palette.accent).add_modifier(Modifier::BOLD),
                )];
                spans.extend(fuzzy::highlight(
                    description,
                    &state.query,
                    style::secondary(palette),
                    palette,
                ));
                Line::from(spans)
            }
        })
        .collect();

    // Delegate to the shared list widget so the cursor highlight, scroll and
    // scrollbar-on-overflow are consistent with every other list. The selected
    // row is the current item's flat row index; headers are never selected.
    let selected = layout
        .selectable
        .get(state.cursor.min(layout.selectable.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0);
    list::render(
        frame,
        rows[1],
        skin,
        list::ListView {
            rows: entries,
            selected,
            offset: &state.offset,
        },
    );

    let hint = footer_hint(skin, rows[2].width as usize);
    frame.render_widget(Paragraph::new(hint), rows[2]);
}

/// The footer hint line for the overlay.
fn footer_hint(skin: &Skin, width: usize) -> Line<'static> {
    shortcut_hints::lines(
        &[
            ("\u{2191}\u{2193}", "move"),
            ("tab", "section"),
            ("esc", "close"),
        ],
        skin.palette.accent_dim,
        width,
    )
    .into_iter()
    .next()
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sections() -> Vec<HelpSection<'static, &'static str>> {
        vec![
            HelpSection {
                title: "Navigation",
                bindings: &[("k", "up"), ("j", "down")],
            },
            HelpSection {
                title: "Tasks",
                bindings: &[("a", "add task"), ("d", "delete")],
            },
        ]
    }

    #[test]
    fn empty_query_keeps_every_section_and_item() {
        let secs = sections();
        let layout = layout_rows(&secs, "");
        // 2 headers + 4 items = 6 rows; 4 selectable; 2 section starts.
        assert_eq!(layout.rows.len(), 6);
        assert_eq!(layout.selectable.len(), 4);
        assert_eq!(layout.section_starts, vec![0, 2]);
    }

    #[test]
    fn query_filters_items_and_drops_empty_sections() {
        let secs = sections();
        let layout = layout_rows(&secs, "add");
        // Only the Tasks section keeps a match ("add task").
        assert_eq!(layout.selectable.len(), 1);
        assert_eq!(layout.section_starts, vec![0]);
        assert!(matches!(layout.rows[0], Row::Header("Tasks")));
    }

    #[test]
    fn section_jump_lands_on_the_first_item_of_the_target() {
        let secs = sections();
        let mut state = Help {
            query: String::new(),
            cursor: 0,
            offset: Cell::new(0),
        };
        // From the first section, Tab moves to the Tasks section start (index 2
        // in the selectable list).
        jump_section(&mut state, &secs, 1);
        assert_eq!(state.cursor, 2);
        // Wraps back to the first section.
        jump_section(&mut state, &secs, 1);
        assert_eq!(state.cursor, 0);
        // BackTab from the first section wraps to the last.
        jump_section(&mut state, &secs, -1);
        assert_eq!(state.cursor, 2);
    }
}
