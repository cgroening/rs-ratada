//! Month picker modal: a `YYYY-MM` header over a 3x4 Jan-Dec grid.
//!
//! Arrows / `hjkl` move within the grid, `PageUp`/`PageDown` change the year,
//! `Home`/`End` jump to January/December, `Enter` picks, `Del`/`Backspace`
//! clears (when allowed) and `Esc` cancels.

use std::io;

use chrono::{Datelike, Local};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    date_picker::{cursor_cell_style, today_cell_style},
    input,
    layout::centered_rect,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup},
    shortcut_hints, style,
    terminal::Tui,
};
use crate::theme::{Palette, Skin};

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct",
    "Nov", "Dec",
];
const BOX_WIDTH: u16 = 18;
const INNER_WIDTH: usize = 16;
/// The month grid is laid out as `MONTH_GRID_ROWS` x `MONTH_GRID_COLS` (12).
const MONTH_GRID_ROWS: u32 = 4;
const MONTH_GRID_COLS: u32 = 3;

/// The picked year and month (1-12).
struct Month {
    year: i32,
    month: u32,
}

/// Opens the month grid at `current` (or the local month). Returns
/// `Value(Some((year, month)))` when picked (`month` is 1-12), `Value(None)`
/// when cleared, `Cancelled` on `Esc`, `Quit` on the global quit chord.
pub fn month_picker(
    tui: &mut Tui,
    skin: &Skin,
    prompt: &str,
    current: Option<(i32, u32)>,
    allow_clear: bool,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Option<(i32, u32)>>> {
    let today = Local::now().date_naive();
    let (year, month) = current.unwrap_or((today.year(), today.month()));
    let mut state = Month { year, month };
    popup(
        tui,
        &mut state,
        |area, state: &Month| {
            let rows = body_lines(
                &skin.palette,
                state.year,
                state.month,
                today,
                allow_clear,
            )
            .len() as u16
                + 2;
            centered_rect(BOX_WIDTH, rows, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &Month| {
            let inner = overlay::framed(frame, rect, skin, prompt);
            let lines = body_lines(
                &skin.palette,
                state.year,
                state.month,
                today,
                allow_clear,
            );
            frame.render_widget(Paragraph::new(lines), inner);
        },
        |state, key| handle_key(state, key, allow_clear),
    )
}

/// Applies one key to the month `state`, or reports that the picker is done.
///
/// A named function rather than a closure inside [`popup`], so the guard below
/// is reachable from a test: everything in `popup` needs a live terminal.
fn handle_key(
    state: &mut Month,
    key: KeyEvent,
    allow_clear: bool,
) -> PopupFlow<Option<(i32, u32)>> {
    // The grid moves on bare keys only: in raw mode crossterm reports Ctrl+H as
    // `Char('h') + CONTROL`, so without this guard a chord would silently walk
    // the month cursor.
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            state.month = step(state.month, -1);
            PopupFlow::Continue
        }
        KeyCode::Right | KeyCode::Char('l') => {
            state.month = step(state.month, 1);
            PopupFlow::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.month = step(state.month, -3);
            PopupFlow::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.month = step(state.month, 3);
            PopupFlow::Continue
        }
        KeyCode::PageUp => {
            state.year -= 1;
            PopupFlow::Continue
        }
        KeyCode::PageDown => {
            state.year += 1;
            PopupFlow::Continue
        }
        KeyCode::Home => {
            state.month = 1;
            PopupFlow::Continue
        }
        KeyCode::End => {
            state.month = 12;
            PopupFlow::Continue
        }
        KeyCode::Enter => PopupFlow::Done(Some((state.year, state.month))),
        KeyCode::Delete | KeyCode::Backspace if allow_clear => {
            PopupFlow::Done(None)
        }
        KeyCode::Esc => PopupFlow::Cancelled,
        _ => PopupFlow::Continue,
    }
}

fn step(month: u32, delta: i32) -> u32 {
    (month as i32 + delta).clamp(1, 12) as u32
}

fn body_lines(
    palette: &Palette,
    year: i32,
    month: u32,
    today: chrono::NaiveDate,
    allow_clear: bool,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();

    let header = format!("{year:04}-{month:02}");
    let pad = INNER_WIDTH.saturating_sub(header.len()) / 2;
    lines.push(Line::from(Span::styled(
        format!("{}{header}", " ".repeat(pad)),
        style::fg(palette.accent).add_modifier(Modifier::BOLD),
    )));

    for row in 0..MONTH_GRID_ROWS {
        let mut spans: Vec<Span> = vec![Span::raw(" ")];
        for col in 0..MONTH_GRID_COLS {
            let candidate = row * MONTH_GRID_COLS + col + 1;
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                MONTHS[(candidate - 1) as usize],
                month_style(palette, year, candidate, month, today),
            ));
        }
        lines.push(Line::from(spans));
    }

    let mut hints: Vec<(&str, &str)> = vec![
        ("\u{2191}\u{2193}\u{2190}\u{2192}", "move"),
        ("pgup/dn", "year"),
        ("enter", "pick"),
    ];
    if allow_clear {
        hints.push(("del", "clear"));
    }
    lines.extend(shortcut_hints::lines(
        &hints,
        palette.accent_dim,
        INNER_WIDTH,
    ));
    lines
}

fn month_style(
    palette: &Palette,
    year: i32,
    month: u32,
    cursor: u32,
    today: chrono::NaiveDate,
) -> Style {
    if month == cursor {
        cursor_cell_style(palette)
    } else if year == today.year() && month == today.month() {
        today_cell_style(palette)
    } else {
        Style::default()
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    /// `Ctrl+H`/`Ctrl+J` arrive as plain characters in raw mode, so without the
    /// guard a chord would walk the month cursor.
    #[test]
    fn ctrl_chords_do_not_move_the_month_cursor() {
        for code in [
            KeyCode::Char('h'),
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char('l'),
            KeyCode::Left,
            KeyCode::Down,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Home,
            KeyCode::End,
        ] {
            let mut state = Month {
                year: 2026,
                month: 6,
            };
            let key = KeyEvent::new(code, KeyModifiers::CONTROL);
            assert!(matches!(
                handle_key(&mut state, key, true),
                PopupFlow::Continue
            ));
            assert_eq!(state.month, 6, "Ctrl+{code:?} moved the month");
            assert_eq!(state.year, 2026, "Ctrl+{code:?} moved the year");
        }
    }

    #[test]
    fn bare_keys_still_move_pick_and_clear() {
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let mut state = Month {
            year: 2026,
            month: 6,
        };

        handle_key(&mut state, press(KeyCode::Char('l')), true);
        assert_eq!(state.month, 7);
        handle_key(&mut state, press(KeyCode::Char('k')), true);
        assert_eq!(state.month, 4);
        handle_key(&mut state, press(KeyCode::PageUp), true);
        assert_eq!(state.year, 2025);
        handle_key(&mut state, press(KeyCode::End), true);
        assert_eq!(state.month, 12);

        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Enter), true),
            PopupFlow::Done(Some((2025, 12)))
        ));
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Delete), true),
            PopupFlow::Done(None)
        ));
        // Without `allow_clear` the same key does nothing.
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Delete), false),
            PopupFlow::Continue
        ));
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Esc), true),
            PopupFlow::Cancelled
        ));
    }

    #[test]
    fn step_clamps_within_the_year() {
        assert_eq!(step(6, 1), 7);
        assert_eq!(step(6, -3), 3);
        assert_eq!(step(1, -1), 1);
        assert_eq!(step(12, 3), 12);
        assert_eq!(step(2, 3), 5);
    }
}
