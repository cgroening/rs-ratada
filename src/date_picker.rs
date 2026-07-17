//! Calendar date picker modal.
//!
//! A centred ISO date over a Monday-first month grid with today, the cursor day
//! and weekends coloured. Arrows / `hjkl` move by day/week, `PageUp`/`PageDown`
//! jump whole months, `Home`/`End` jump to the first/last day of the visible
//! month, `Enter` picks, `Del`/`Backspace` clears (when allowed) and `Esc`
//! cancels.

use std::io;

use chrono::{Datelike, Duration, Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    input,
    layout::centered_rect,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup},
    shortcut_hints, style,
    terminal::Tui,
};
use crate::theme::{Palette, Skin};

const BOX_WIDTH: u16 = 24;
const INNER_WIDTH: usize = 22;
/// The Monday-first weekday headers, shared by the calendar pickers.
const WEEKDAY_NAMES: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
/// The weekday index (Monday = 0) at which the weekend begins (Saturday).
const WEEKEND_START: u32 = 5;

/// Opens the calendar at `current` (or today). `allow_clear` lets `Del` return
/// an empty date. Returns `Value(Some(date))` when picked, `Value(None)` when
/// cleared, `Cancelled` on `Esc`, `Quit` on the global quit chord.
pub fn date_picker(
    tui: &mut Tui,
    skin: &Skin,
    prompt: &str,
    current: Option<NaiveDate>,
    allow_clear: bool,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Option<NaiveDate>>> {
    let mut cursor = current.unwrap_or_else(today);
    popup(
        tui,
        &mut cursor,
        |area, &cursor| {
            let rows =
                body_lines(&skin.palette, cursor, allow_clear).len() as u16 + 2;
            centered_rect(BOX_WIDTH, rows, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, &cursor| {
            let inner = overlay::framed(frame, rect, skin, prompt);
            let lines = body_lines(&skin.palette, cursor, allow_clear);
            frame.render_widget(Paragraph::new(lines), inner);
        },
        |cursor, key| handle_key(cursor, key, allow_clear),
    )
}

/// Applies one key to the day `cursor`, or reports that the picker is done.
///
/// A named function rather than a closure inside [`popup`], so the guard below
/// is reachable from a test: everything in `popup` needs a live terminal.
fn handle_key(
    cursor: &mut NaiveDate,
    key: KeyEvent,
    allow_clear: bool,
) -> PopupFlow<Option<NaiveDate>> {
    // The grid moves on bare keys only: in raw mode crossterm reports Ctrl+H as
    // `Char('h') + CONTROL`, so without this guard a chord would silently walk
    // the day cursor.
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            *cursor = shift(*cursor, -1);
            PopupFlow::Continue
        }
        KeyCode::Right | KeyCode::Char('l') => {
            *cursor = shift(*cursor, 1);
            PopupFlow::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            *cursor = shift(*cursor, -7);
            PopupFlow::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            *cursor = shift(*cursor, 7);
            PopupFlow::Continue
        }
        KeyCode::PageUp => {
            *cursor = add_months(*cursor, -1);
            PopupFlow::Continue
        }
        KeyCode::PageDown => {
            *cursor = add_months(*cursor, 1);
            PopupFlow::Continue
        }
        KeyCode::Home => {
            *cursor = first_of_month(*cursor);
            PopupFlow::Continue
        }
        KeyCode::End => {
            *cursor = last_of_month(*cursor);
            PopupFlow::Continue
        }
        KeyCode::Enter => PopupFlow::Done(Some(*cursor)),
        KeyCode::Delete | KeyCode::Backspace if allow_clear => {
            PopupFlow::Done(None)
        }
        KeyCode::Esc => PopupFlow::Cancelled,
        _ => PopupFlow::Continue,
    }
}

/// The local date today.
pub(super) fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Moves `date` by `days` (may be negative).
pub(super) fn shift(date: NaiveDate, days: i64) -> NaiveDate {
    date + Duration::days(days)
}

/// Moves `date` by `months` (may be negative), clamping to a valid day.
pub(super) fn add_months(date: NaiveDate, months: i32) -> NaiveDate {
    let step = chrono::Months::new(months.unsigned_abs());
    let shifted = if months >= 0 {
        date.checked_add_months(step)
    } else {
        date.checked_sub_months(step)
    };
    shifted.unwrap_or(date)
}

/// The first day of the month containing `date`.
pub(super) fn first_of_month(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
}

/// The last day of the month containing `date`.
pub(super) fn last_of_month(date: NaiveDate) -> NaiveDate {
    // First of next month, stepped back one day.
    shift(add_months(first_of_month(date), 1), -1)
}

/// The Monday-first cells for the month containing `cursor`: leading `None`s for
/// the offset to the first weekday, one `Some(day)` per day, trailing `None`s to
/// fill the last week. Shared with the date-range picker.
pub(super) fn month_cells(cursor: NaiveDate) -> Vec<Option<NaiveDate>> {
    let first = first_of_month(cursor);
    let lead = first.weekday().num_days_from_monday() as usize;
    let mut cells: Vec<Option<NaiveDate>> = vec![None; lead];
    let mut day = first;
    while day.month() == cursor.month() {
        cells.push(Some(day));
        day = shift(day, 1);
    }
    while !cells.len().is_multiple_of(7) {
        cells.push(None);
    }
    cells
}

fn body_lines(
    palette: &Palette,
    cursor: NaiveDate,
    allow_clear: bool,
) -> Vec<Line<'static>> {
    let today = today();
    let mut lines: Vec<Line> = Vec::new();

    let iso = cursor.format("%Y-%m-%d").to_string();
    let pad = INNER_WIDTH.saturating_sub(iso.len()) / 2;
    lines.push(Line::from(Span::styled(
        format!("{}{iso}", " ".repeat(pad)),
        style::fg(palette.accent).add_modifier(Modifier::BOLD),
    )));

    lines.push(weekday_header(palette));

    lines.extend(day_grid(cursor, |day| {
        day_style(palette, day, cursor, today)
    }));

    let mut hints: Vec<(&str, &str)> = vec![
        ("\u{2190}\u{2192}\u{2191}\u{2193}", "move"),
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

/// The Monday-first weekday header row (`Mo Tu We Th Fr Sa Su`), shared by the
/// calendar pickers.
pub(super) fn weekday_header(palette: &Palette) -> Line<'static> {
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    for (index, name) in WEEKDAY_NAMES.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(name, style::secondary(palette)));
    }
    Line::from(spans)
}

/// The month around `cursor` as week rows, each day styled by `style_of`. Shared
/// by the calendar pickers, which differ only in how a day is styled.
pub(super) fn day_grid(
    cursor: NaiveDate,
    style_of: impl Fn(NaiveDate) -> Style,
) -> Vec<Line<'static>> {
    month_cells(cursor)
        .chunks(7)
        .map(|week| {
            let mut spans: Vec<Span> = vec![Span::raw(" ")];
            for (index, cell) in week.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::raw(" "));
                }
                match cell {
                    Some(day) => spans.push(Span::styled(
                        format!("{:>2}", day.day()),
                        style_of(*day),
                    )),
                    None => spans.push(Span::raw("  ")),
                }
            }
            Line::from(spans)
        })
        .collect()
}

/// Whether `day` falls on the weekend (Saturday or Sunday).
pub(super) fn is_weekend(day: NaiveDate) -> bool {
    day.weekday().num_days_from_monday() >= WEEKEND_START
}

fn day_style(
    palette: &Palette,
    day: NaiveDate,
    cursor: NaiveDate,
    today: NaiveDate,
) -> Style {
    if day == cursor {
        style::bg(palette.selection)
            .fg(style::to_ratatui(palette.accent))
            .add_modifier(Modifier::BOLD)
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

    /// `Ctrl+H`/`Ctrl+J` arrive as plain characters in raw mode, so without the
    /// guard a chord would walk the day cursor.
    #[test]
    fn ctrl_chords_do_not_move_the_day_cursor() {
        let start = NaiveDate::from_ymd_opt(2026, 6, 15).expect("a real date");
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
            let mut cursor = start;
            let key = KeyEvent::new(code, KeyModifiers::CONTROL);
            assert!(matches!(
                handle_key(&mut cursor, key, true),
                PopupFlow::Continue
            ));
            assert_eq!(cursor, start, "Ctrl+{code:?} moved the cursor");
        }
    }

    #[test]
    fn bare_keys_still_move_pick_and_clear() {
        let start = NaiveDate::from_ymd_opt(2026, 6, 15).expect("a real date");
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let mut cursor = start;

        handle_key(&mut cursor, press(KeyCode::Char('l')), true);
        assert_eq!(cursor, shift(start, 1));
        handle_key(&mut cursor, press(KeyCode::Char('j')), true);
        assert_eq!(cursor, shift(start, 8));

        assert!(matches!(
            handle_key(&mut cursor, press(KeyCode::Enter), true),
            PopupFlow::Done(Some(_))
        ));
        assert!(matches!(
            handle_key(&mut cursor, press(KeyCode::Delete), true),
            PopupFlow::Done(None)
        ));
        // Without `allow_clear` the same key does nothing.
        assert!(matches!(
            handle_key(&mut cursor, press(KeyCode::Delete), false),
            PopupFlow::Continue
        ));
    }

    #[test]
    fn month_cells_cover_the_month_padded_to_full_weeks() {
        let date = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let cells = month_cells(date);
        // June has 30 days; the grid is padded to whole weeks.
        assert_eq!(cells.iter().filter(|c| c.is_some()).count(), 30);
        assert_eq!(cells.len() % 7, 0);
    }

    #[test]
    fn add_months_clamps_to_a_valid_day() {
        let jan31 = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        assert_eq!(
            add_months(jan31, 1),
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap(),
        );
    }

    #[test]
    fn shift_moves_by_days() {
        let day = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        assert_eq!(
            shift(day, 7),
            NaiveDate::from_ymd_opt(2026, 6, 22).unwrap()
        );
    }

    #[test]
    fn first_and_last_of_month_bound_the_visible_month() {
        let mid = NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        assert_eq!(
            first_of_month(mid),
            NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
        );
        // February 2026 has 28 days.
        assert_eq!(
            last_of_month(mid),
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap(),
        );
    }
}
