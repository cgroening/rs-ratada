//! Scrollable, fuzzy-searchable help overlay listing all key bindings.
//!
//! A thin wrapper over [`overlay::popup`]: the dimmed backdrop, box and loop
//! come from there; this module only owns the search state and the body layout.

use std::io;

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
};

use super::{
    fuzzy,
    layout::centered_rect,
    modal::ModalSignal,
    nav,
    overlay::{self, PopupFlow, popup},
    style,
    terminal::Tui,
};
use crate::theme::Skin;

/// The search state of the help overlay.
struct Help {
    query: String,
    cursor: usize,
}

/// Shows the help overlay until the user closes it.
///
/// A query filters the bindings fuzzily; the arrow keys move the selection.
/// `Esc` or `?` close the overlay.
pub fn show<B: AsRef<str>>(
    tui: &mut Tui,
    skin: &Skin,
    bindings: &[(B, B)],
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<()>> {
    let mut state = Help {
        query: String::new(),
        cursor: 0,
    };
    popup(
        tui,
        &mut state,
        |area, _| {
            centered_rect(
                (area.width * 2 / 3).clamp(40, area.width),
                (area.height * 2 / 3).clamp(8, area.height),
                area,
            )
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &Help| {
            let inner = overlay::framed(frame, rect, skin, "Help");
            render_body(frame, inner, skin, bindings, state);
        },
        |state, key| match key.code {
            KeyCode::Esc | KeyCode::Char('?') => PopupFlow::Done(()),
            KeyCode::Up => {
                let len = filter(bindings, &state.query).len();
                state.cursor = nav::cycle(state.cursor, len, -1);
                PopupFlow::Continue
            }
            KeyCode::Down => {
                let len = filter(bindings, &state.query).len();
                state.cursor = nav::cycle(state.cursor, len, 1);
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

fn filter<B: AsRef<str>>(bindings: &[(B, B)], query: &str) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..bindings.len()).collect();
    }
    let mut scored: Vec<(u32, usize)> = bindings
        .iter()
        .enumerate()
        .filter_map(|(index, (key, description))| {
            let haystack = format!("{} {}", key.as_ref(), description.as_ref());
            fuzzy::score(&haystack, query).map(|score| (score, index))
        })
        .collect();
    scored.sort_by(|left, right| right.0.cmp(&left.0));
    scored.into_iter().map(|(_, index)| index).collect()
}

fn render_body<B: AsRef<str>>(
    frame: &mut Frame,
    inner: Rect,
    skin: &Skin,
    bindings: &[(B, B)],
    state: &Help,
) {
    let palette = &skin.palette;
    let filtered = filter(bindings, &state.query);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let search = Line::from(vec![
        Span::styled("search ", style::dim()),
        Span::raw(state.query.clone()),
        Span::styled(
            " ",
            Style::default().bg(style::to_ratatui(palette.cursor)),
        ),
    ]);
    frame.render_widget(Paragraph::new(search), rows[0]);

    let entries: Vec<ListItem> = filtered
        .iter()
        .map(|&index| {
            let key = bindings[index].0.as_ref();
            let description = bindings[index].1.as_ref();
            let mut spans = vec![Span::styled(
                format!("{key:<14}"),
                style::fg(palette.accent).add_modifier(Modifier::BOLD),
            )];
            spans.extend(fuzzy::highlight(
                description,
                &state.query,
                style::dim(),
                palette,
            ));
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut list_state = ListState::default();
    if !filtered.is_empty() {
        let cursor = state.cursor.min(filtered.len() - 1);
        list_state.select(Some(cursor));
    }
    let list =
        List::new(entries).highlight_style(style::bg(palette.selection_bg));
    frame.render_stateful_widget(list, rows[1], &mut list_state);
}
