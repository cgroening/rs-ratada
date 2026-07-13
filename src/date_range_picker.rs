//! A calendar date-range picker modal: pick a start day, then an end day.
//!
//! Shares the month grid and navigation with [`super::date_picker`]. The first
//! `Enter` fixes the start; the range up to the cursor is highlighted; the
//! second `Enter` returns the ordered `(start, end)` pair.

use std::io;

use chrono::NaiveDate;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    date_picker::{
        add_months, day_grid, first_of_month, is_weekend, last_of_month, shift,
        today, weekday_header,
    },
    layout::centered_rect,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup},
    shortcut_hints, style,
    terminal::Tui,
};
use crate::theme::Skin;

const BOX_WIDTH: u16 = 24;
const INNER_WIDTH: usize = 22;

/// The two-phase state of the range picker: the cursor day plus the fixed start
/// once the first `Enter` was pressed.
struct Range {
    cursor: NaiveDate,
    start: Option<NaiveDate>,
}

/// Opens the range picker. `initial` pre-selects a range (resumes with the end
/// under the cursor). `Enter` fixes the start, then the end; `Esc` cancels.
pub fn date_range_picker(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: Option<(NaiveDate, NaiveDate)>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<(NaiveDate, NaiveDate)>> {
    let mut state = match initial {
        Some((begin, end)) => Range {
            cursor: end,
            start: Some(begin),
        },
        None => Range {
            cursor: today(),
            start: None,
        },
    };
    popup(
        tui,
        &mut state,
        |area, state: &Range| {
            let rows =
                body_lines(skin, state.cursor, state.start).len() as u16 + 2;
            centered_rect(BOX_WIDTH, rows, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &Range| {
            let inner = overlay::framed(frame, rect, skin, title);
            let lines = body_lines(skin, state.cursor, state.start);
            frame.render_widget(Paragraph::new(lines), inner);
        },
        |state, key| match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                state.cursor = shift(state.cursor, -1);
                PopupFlow::Continue
            }
            KeyCode::Right | KeyCode::Char('l') => {
                state.cursor = shift(state.cursor, 1);
                PopupFlow::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.cursor = shift(state.cursor, -7);
                PopupFlow::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.cursor = shift(state.cursor, 7);
                PopupFlow::Continue
            }
            KeyCode::PageUp => {
                state.cursor = add_months(state.cursor, -1);
                PopupFlow::Continue
            }
            KeyCode::PageDown => {
                state.cursor = add_months(state.cursor, 1);
                PopupFlow::Continue
            }
            KeyCode::Home => {
                state.cursor = first_of_month(state.cursor);
                PopupFlow::Continue
            }
            KeyCode::End => {
                state.cursor = last_of_month(state.cursor);
                PopupFlow::Continue
            }
            KeyCode::Enter => match state.start {
                None => {
                    state.start = Some(state.cursor);
                    PopupFlow::Continue
                }
                Some(begin) => PopupFlow::Done(ordered(begin, state.cursor)),
            },
            KeyCode::Esc => PopupFlow::Cancelled,
            _ => PopupFlow::Continue,
        },
    )
}

/// Orders two dates into `(earlier, later)`.
pub(crate) fn ordered(a: NaiveDate, b: NaiveDate) -> (NaiveDate, NaiveDate) {
    if a <= b { (a, b) } else { (b, a) }
}

fn body_lines(
    skin: &Skin,
    cursor: NaiveDate,
    start: Option<NaiveDate>,
) -> Vec<Line<'static>> {
    let palette = &skin.palette;
    let today = today();
    let mut lines: Vec<Line> = Vec::new();

    let header = match start {
        None => format!("pick start \u{b7} {}", cursor.format("%Y-%m-%d")),
        Some(begin) => {
            let (lo, hi) = ordered(begin, cursor);
            format!("{} \u{2192} {}", lo.format("%m-%d"), hi.format("%m-%d"))
        }
    };
    let pad = INNER_WIDTH.saturating_sub(header.len()) / 2;
    lines.push(Line::from(Span::styled(
        format!("{}{header}", " ".repeat(pad)),
        style::fg(palette.accent).add_modifier(Modifier::BOLD),
    )));

    lines.push(weekday_header(palette));

    lines.extend(day_grid(cursor, |day| {
        day_style(skin, day, cursor, start, today)
    }));

    lines.extend(shortcut_hints::lines(
        &[
            ("\u{2190}\u{2192}\u{2191}\u{2193}", "move"),
            ("enter", "start/end"),
        ],
        palette.accent_dim,
        INNER_WIDTH,
    ));
    lines
}

fn day_style(
    skin: &Skin,
    day: NaiveDate,
    cursor: NaiveDate,
    start: Option<NaiveDate>,
    today: NaiveDate,
) -> Style {
    let palette = &skin.palette;
    let in_range = start.is_some_and(|begin| {
        let (lo, hi) = ordered(begin, cursor);
        day >= lo && day <= hi
    });
    if day == cursor || start == Some(day) {
        style::bg(palette.selection)
            .fg(style::to_ratatui(palette.accent))
            .add_modifier(Modifier::BOLD)
    } else if in_range {
        style::bg(palette.selection)
    } else if day == today {
        style::fg(palette.accent_dim).add_modifier(Modifier::BOLD)
    } else if is_weekend(day) {
        style::secondary(palette)
    } else {
        Style::default().fg(Color::Reset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_sorts_the_pair() {
        let early = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let late = NaiveDate::from_ymd_opt(2026, 1, 20).unwrap();
        assert_eq!(ordered(late, early), (early, late));
        assert_eq!(ordered(early, late), (early, late));
    }
}
