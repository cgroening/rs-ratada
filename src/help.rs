//! Scrollable, fuzzy-searchable help overlay listing key bindings in sections.
//!
//! A thin wrapper over [`overlay::popup`]: the dimmed backdrop, box and loop
//! come from there; this module owns the search state and the sectioned body.
//! Bindings are grouped into [`HelpSection`]s; `Tab`/`BackTab` jump between
//! sections, the arrows (plus `PageUp`/`PageDown` and `Home`/`End`) move within
//! the flat list, and typing filters fuzzily while keeping the section headers
//! of any section that still has a match.

use std::{cell::Cell, io};

use crossterm::event::{KeyCode, KeyEvent};
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
    overlay::{self, PopupFlow, popup_with_paste},
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
    /// The list viewport height captured at render, driving page jumps.
    viewport: Cell<usize>,
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
/// that still matches); the arrows (plus `PageUp`/`PageDown` and `Home`/`End`)
/// move the selection, `Tab`/`BackTab` jump
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
        viewport: Cell::new(1),
    };
    popup_with_paste(
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
        |state, key| handle_key(state, key, sections),
        |state, text| {
            state
                .query
                .extend(text.chars().filter(|ch| !ch.is_control()));
            state.cursor = 0;
            PopupFlow::Continue
        },
    )
}

/// Applies one key to the overlay `state`, or reports that the help is done.
///
/// A named function rather than a closure inside [`popup`], so the guard below
/// is reachable from a test: everything in `popup` needs a live terminal.
fn handle_key<B: AsRef<str>>(
    state: &mut Help,
    key: KeyEvent,
    sections: &[HelpSection<'_, B>],
) -> PopupFlow<()> {
    // The overlay binds no chord of its own, so a Ctrl command is not ours to
    // act on: without this `Ctrl+U` types a `u` into the search instead of
    // clearing the line, and `Ctrl+?` would close the help. Alt alone and AltGr
    // (Ctrl+Alt) still type, as they do in every text field - see
    // `input::is_command`.
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('?') => PopupFlow::Done(()),
        KeyCode::Up => {
            let count = layout_rows(sections, &state.query).selectable.len();
            state.cursor = nav::cycle(state.cursor, count, -1);
            PopupFlow::Continue
        }
        KeyCode::Down => {
            let count = layout_rows(sections, &state.query).selectable.len();
            state.cursor = nav::cycle(state.cursor, count, 1);
            PopupFlow::Continue
        }
        KeyCode::PageUp => {
            let count = layout_rows(sections, &state.query).selectable.len();
            let page = state.viewport.get().max(1) as isize;
            state.cursor = nav::step_clamped(state.cursor, count, -page);
            PopupFlow::Continue
        }
        KeyCode::PageDown => {
            let count = layout_rows(sections, &state.query).selectable.len();
            let page = state.viewport.get().max(1) as isize;
            state.cursor = nav::step_clamped(state.cursor, count, page);
            PopupFlow::Continue
        }
        KeyCode::Home => {
            state.cursor = 0;
            PopupFlow::Continue
        }
        KeyCode::End => {
            let count = layout_rows(sections, &state.query).selectable.len();
            state.cursor = count.saturating_sub(1);
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
    }
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

    // The popup footer always reserves its row (popup hints ignore the global
    // F1 toggle, which governs only the main-app footer).
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
    use crossterm::event::KeyModifiers;

    use super::*;

    fn state() -> Help {
        Help {
            query: String::new(),
            cursor: 0,
            offset: Cell::new(0),
            viewport: Cell::new(1),
        }
    }

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
        let mut state = state();
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

    /// A Ctrl chord belongs to the host: without the guard `Ctrl+U` would type
    /// a `u` into the search, `Ctrl+J` walk the list and `Ctrl+?` close the
    /// overlay.
    #[test]
    fn ctrl_chords_do_not_navigate_or_type() {
        let secs = sections();
        for code in [
            KeyCode::Down,
            KeyCode::Up,
            KeyCode::End,
            KeyCode::PageDown,
            KeyCode::Tab,
            KeyCode::Char('u'),
            KeyCode::Char('?'),
        ] {
            let mut state = state();
            let key = KeyEvent::new(code, KeyModifiers::CONTROL);
            assert!(matches!(
                handle_key(&mut state, key, &secs),
                PopupFlow::Continue
            ));
            assert_eq!(state.cursor, 0, "Ctrl+{code:?} moved the cursor");
            assert!(state.query.is_empty(), "Ctrl+{code:?} typed a character");
        }
    }

    #[test]
    fn bare_keys_still_navigate_and_type() {
        let secs = sections();
        let mut state = state();
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
        handle_key(&mut state, press(KeyCode::Down), &secs);
        assert_eq!(state.cursor, 1);
        handle_key(&mut state, press(KeyCode::End), &secs);
        assert_eq!(state.cursor, 3);
        handle_key(&mut state, press(KeyCode::Char('u')), &secs);
        assert_eq!(state.query, "u");
        assert_eq!(state.cursor, 0, "typing restarts the selection");
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Esc), &secs),
            PopupFlow::Done(())
        ));
    }

    /// The other half of the rule: `AltGr` is reported as `Ctrl+Alt` yet types
    /// a real character, so the search must accept it. Guarding the `Char` arm
    /// with `is_bare_character` instead of `!is_command` would make `@`, `\`
    /// and `[` untypeable on a German keyboard.
    #[test]
    fn altgr_characters_still_reach_the_filter() {
        let secs = sections();
        let mut state = state();
        for ch in ['@', '\\', '['] {
            handle_key(
                &mut state,
                KeyEvent::new(
                    KeyCode::Char(ch),
                    KeyModifiers::CONTROL | KeyModifiers::ALT,
                ),
                &secs,
            );
        }
        assert_eq!(state.query, "@\\[");
    }
}
