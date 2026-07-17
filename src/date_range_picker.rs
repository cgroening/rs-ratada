//! A calendar date-range picker modal: pick a start day, then an end day.
//!
//! Shares the month grid and navigation with [`super::date_picker`]. The first
//! `Enter` fixes the start; the range up to the cursor is highlighted; the
//! second `Enter` returns the ordered `(start, end)` pair.

use std::io;

use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyEvent};
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
    input,
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
        handle_key,
    )
}

/// Applies one key to the range `state`, or reports that the picker is done.
///
/// A named function rather than a closure inside [`popup`], so the guard below
/// is reachable from a test: everything in `popup` needs a live terminal.
fn handle_key(
    state: &mut Range,
    key: KeyEvent,
) -> PopupFlow<(NaiveDate, NaiveDate)> {
    // The grid moves on bare keys only: in raw mode crossterm reports Ctrl+H as
    // `Char('h') + CONTROL`, so without this guard a chord would silently walk
    // the day cursor.
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    match key.code {
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
    }
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
    use crossterm::event::KeyModifiers;

    use super::*;

    fn day(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, day).expect("a real date")
    }

    /// `Ctrl+H`/`Ctrl+J` arrive as plain characters in raw mode, so without the
    /// guard a chord would walk the day cursor.
    #[test]
    fn ctrl_chords_do_not_move_the_day_cursor() {
        let start = day(15);
        for code in [
            KeyCode::Char('h'),
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char('l'),
            KeyCode::Left,
            KeyCode::Down,
            KeyCode::PageUp,
            KeyCode::Home,
        ] {
            let mut state = Range {
                cursor: start,
                start: None,
            };
            let key = KeyEvent::new(code, KeyModifiers::CONTROL);
            assert!(matches!(handle_key(&mut state, key), PopupFlow::Continue));
            assert_eq!(state.cursor, start, "Ctrl+{code:?} moved the cursor");
            assert!(state.start.is_none(), "Ctrl+{code:?} fixed the start");
        }
    }

    #[test]
    fn bare_keys_still_move_and_pick_both_ends() {
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let mut state = Range {
            cursor: day(15),
            start: None,
        };

        handle_key(&mut state, press(KeyCode::Char('l')));
        assert_eq!(state.cursor, day(16));
        handle_key(&mut state, press(KeyCode::Char('j')));
        assert_eq!(state.cursor, day(23));

        // The first Enter fixes the start, the second returns the pair.
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Enter)),
            PopupFlow::Continue
        ));
        assert_eq!(state.start, Some(day(23)));
        handle_key(&mut state, press(KeyCode::Char('h')));
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Enter)),
            PopupFlow::Done(pair) if pair == (day(22), day(23)),
        ));
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Esc)),
            PopupFlow::Cancelled
        ));
    }

    #[test]
    fn ordered_sorts_the_pair() {
        let early = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let late = NaiveDate::from_ymd_opt(2026, 1, 20).unwrap();
        assert_eq!(ordered(late, early), (early, late));
        assert_eq!(ordered(early, late), (early, late));
    }
}
