//! A fuzzy picker modal: filter-as-you-type selection over arbitrary items.
//!
//! Generalises the pattern behind the help overlay into a reusable component.
//! A thin wrapper over [`overlay::popup`]; returns the original index of the
//! chosen item via a [`ModalSignal`].

use std::{cell::Cell, io};

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    chrome, fuzzy,
    layout::centered_fraction,
    list,
    modal::ModalSignal,
    nav,
    overlay::{self, PopupFlow, popup},
    style,
    terminal::Tui,
};
use crate::theme::Skin;

/// The state of the fuzzy finder: the query, the cursor into the filtered
/// results, and the persistent list scroll offset.
struct Finder {
    query: String,
    cursor: usize,
    offset: Cell<usize>,
}

/// Lets the user pick one entry from `items` with a live fuzzy filter. `Enter`
/// confirms the highlighted entry (returning its index into `items`), `Esc`
/// cancels. An empty `items` cancels immediately.
pub fn finder(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[String],
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<usize>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut state = Finder {
        query: String::new(),
        cursor: 0,
        offset: Cell::new(0),
    };
    popup(
        tui,
        &mut state,
        |area, _| centered_fraction(area, 2, 3, 40, 8),
        |frame, _| render_bg(frame),
        |frame, rect, state: &Finder| {
            let inner = overlay::framed(frame, rect, skin, title);
            render_body(frame, inner, skin, items, state);
            // The badge counts the matches, not the whole item list; the
            // cursor is clamped into them exactly as the body clamps it.
            let matches = filter(items, &state.query).len();
            let cursor = state.cursor.min(matches.saturating_sub(1));
            let badge = chrome::position_badge(cursor, matches);
            chrome::render_badge(frame, rect, skin, &badge);
        },
        |state, key| match key.code {
            KeyCode::Esc => PopupFlow::Cancelled,
            KeyCode::Enter => {
                match filter(items, &state.query).get(state.cursor) {
                    Some(&index) => PopupFlow::Done(index),
                    None => PopupFlow::Continue,
                }
            }
            KeyCode::Up => {
                let len = filter(items, &state.query).len();
                state.cursor = nav::cycle(state.cursor, len, -1);
                PopupFlow::Continue
            }
            KeyCode::Down => {
                let len = filter(items, &state.query).len();
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

/// The indices of `items` matching `query`, best score first. An empty query
/// keeps the original order.
pub fn filter(items: &[String], query: &str) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..items.len()).collect();
    }
    let mut scored: Vec<(u32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            fuzzy::score(item, query).map(|score| (score, index))
        })
        .collect();
    scored.sort_by(|left, right| right.0.cmp(&left.0));
    scored.into_iter().map(|(_, index)| index).collect()
}

fn render_body(
    frame: &mut Frame,
    inner: Rect,
    skin: &Skin,
    items: &[String],
    state: &Finder,
) {
    let palette = &skin.palette;
    let filtered = filter(items, &state.query);
    let cursor = state.cursor.min(filtered.len().saturating_sub(1));

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let search = Line::from(vec![
        Span::styled("search ", style::secondary(palette)),
        Span::raw(state.query.clone()),
        Span::styled(
            " ",
            Style::default().bg(style::to_ratatui(palette.cursor)),
        ),
    ]);
    frame.render_widget(Paragraph::new(search), rows[0]);

    let lines: Vec<Line<'static>> = filtered
        .iter()
        .map(|&index| {
            Line::from(fuzzy::highlight(
                &items[index],
                &state.query,
                style::primary(palette),
                palette,
            ))
        })
        .collect();
    list::render(
        frame,
        rows[1],
        skin,
        list::ListView {
            rows: lines,
            selected: cursor,
            offset: &state.offset,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<String> {
        ["Write report", "Reply to email", "Prepare slides"]
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    #[test]
    fn empty_query_keeps_original_order() {
        assert_eq!(filter(&items(), ""), vec![0, 1, 2]);
    }

    #[test]
    fn query_filters_and_ranks() {
        let result = filter(&items(), "rep");
        assert!(!result.is_empty());
        // Every returned item actually matches the query.
        assert!(
            result
                .iter()
                .all(|&i| fuzzy::score(&items()[i], "rep").is_some())
        );
    }

    #[test]
    fn non_matching_query_returns_nothing() {
        assert!(filter(&items(), "zzzzz").is_empty());
    }
}
