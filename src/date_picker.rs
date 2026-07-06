//! Calendar date picker modal.
//!
//! A centred ISO date over a Monday-first month grid with today, the cursor day
//! and weekends coloured. Arrows / `hjkl` move by day/week, `PageUp`/`PageDown`
//! jump whole months, `Enter` picks, `Del`/`Backspace` clears (when allowed)
//! and `Esc` cancels.

use std::io;

use chrono::{Datelike, Duration, Local, NaiveDate};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    layout::centered_rect,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup},
    shortcut_hints, style,
    terminal::Tui,
};
use crate::theme::{Palette, Skin};

const BOX_WIDTH: u16 = 24;
const INNER_WIDTH: usize = 22;

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
            centered_rect(overlay::box_width(BOX_WIDTH, skin), rows, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, &cursor| {
            let inner = overlay::framed(frame, rect, skin, prompt);
            let lines = body_lines(&skin.palette, cursor, allow_clear);
            frame.render_widget(Paragraph::new(lines), inner);
        },
        |cursor, key| match key.code {
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
            KeyCode::Enter => PopupFlow::Done(Some(*cursor)),
            KeyCode::Delete | KeyCode::Backspace if allow_clear => {
                PopupFlow::Done(None)
            }
            KeyCode::Esc => PopupFlow::Cancelled,
            _ => PopupFlow::Continue,
        },
    )
}

pub(super) fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub(super) fn shift(date: NaiveDate, days: i64) -> NaiveDate {
    date + Duration::days(days)
}

pub(super) fn add_months(date: NaiveDate, months: i32) -> NaiveDate {
    let step = chrono::Months::new(months.unsigned_abs());
    let shifted = if months >= 0 {
        date.checked_add_months(step)
    } else {
        date.checked_sub_months(step)
    };
    shifted.unwrap_or(date)
}

/// The Monday-first cells for the month containing `cursor`: leading `None`s for
/// the offset to the first weekday, one `Some(day)` per day, trailing `None`s to
/// fill the last week. Shared with the date-range picker.
pub(super) fn month_cells(cursor: NaiveDate) -> Vec<Option<NaiveDate>> {
    let first = cursor.with_day(1).unwrap_or(cursor);
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

    let mut header: Vec<Span> = vec![Span::raw(" ")];
    for (index, name) in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
        .into_iter()
        .enumerate()
    {
        if index > 0 {
            header.push(Span::raw(" "));
        }
        header.push(Span::styled(name, style::dim()));
    }
    lines.push(Line::from(header));

    lines.extend(day_grid(palette, cursor, today));

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

fn day_grid(
    palette: &Palette,
    cursor: NaiveDate,
    today: NaiveDate,
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
                        day_style(palette, *day, cursor, today),
                    )),
                    None => spans.push(Span::raw("  ")),
                }
            }
            Line::from(spans)
        })
        .collect()
}

fn day_style(
    palette: &Palette,
    day: NaiveDate,
    cursor: NaiveDate,
    today: NaiveDate,
) -> Style {
    if day == cursor {
        style::bg(palette.selection_bg)
            .fg(style::to_ratatui(palette.accent))
            .add_modifier(Modifier::BOLD)
    } else if day == today {
        style::fg(palette.accent_dim).add_modifier(Modifier::BOLD)
    } else if day.weekday().num_days_from_monday() >= 5 {
        style::dim()
    } else {
        Style::default().fg(Color::Reset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
