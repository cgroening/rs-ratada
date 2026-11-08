//! A calendar date-range picker modal: pick a start day, then an end day.
//!
//! Shares the month grid and navigation with [`super::date_picker`]. The first
//! `Enter` fixes the start; the range up to the cursor is highlighted; the
//! second `Enter` returns the ordered `(start, end)` pair.
//!
//! `e` turns the header into a text field holding the whole range as
//! `YYYY-MM-DD..YYYY-MM-DD`, prefilled and fully selected. Each end may also be
//! typed as `DD.MM.YYYY`, `DD.MM.YY` or `DD.MM.` without a year, which borrows
//! the other end's year (`20.12..5.1` runs into the next one). Both ends light
//! up as soon as the text parses; while it does not, the field is coloured as
//! an error and `Enter` does nothing. A reversed pair is normalised, exactly as
//! the calendar's second `Enter` does. `Enter` returns the range, `Esc` leaves
//! the field and puts the calendar back.
//!
//! The ISO calendar-week column of [`super::date_picker`] leads each row here
//! too, its emphasis following the cursor. Its switch,
//! [`set_week_column`](super::date_picker::set_week_column), covers both.

use std::io;

use chrono::{Datelike, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    chrome::{MODAL_CHROME_HEIGHT, MODAL_PADDING_WIDTH},
    date_entry::{self, DateEntry, EntryAction, OPEN_KEY, Validity},
    date_picker::{
        ISO_FORMAT, WeekColumn, add_months, cursor_cell_style, day_grid,
        first_of_month, is_weekend, last_of_month, shift, today,
        today_cell_style, week_column, weekday_header,
    },
    date_text, input,
    layout::centered_rect,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup_with_paste},
    style,
    terminal::Tui,
};
use crate::theme::Skin;

// One column wider than the other calendar pickers: the typed range fills the
// inner width by itself, and the block caret past its last character needs a
// cell of its own.
const BOX_WIDTH: u16 = 25;
const INNER_WIDTH: usize = 23;
/// The characters the longest typed range takes (`YYYY-MM-DD..YYYY-MM-DD`, or
/// the same with `DD.MM.YYYY` ends), and with it the field's limit.
const ENTRY_MAX_LEN: usize = 22;

/// The two-phase calendar state: the cursor day plus the fixed start once the
/// first `Enter` was pressed.
#[derive(Clone, Copy)]
struct Range {
    cursor: NaiveDate,
    start: Option<NaiveDate>,
}

/// The picker state: the calendar, the typed-entry field while it is open, and
/// the week-column setting the picker opened with.
struct Picker {
    range: Range,
    entry: Option<DateEntry<Range>>,
    week_column: WeekColumn,
}

/// Opens the range picker. `initial` pre-selects a range (resumes with the end
/// under the cursor). `Enter` fixes the start, then the end; `e` opens a field
/// for typing the whole range instead; `Esc` cancels.
pub fn date_range_picker(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: Option<(NaiveDate, NaiveDate)>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<(NaiveDate, NaiveDate)>> {
    let range = match initial {
        Some((begin, end)) => Range {
            cursor: end,
            start: Some(begin),
        },
        None => Range {
            cursor: today(),
            start: None,
        },
    };
    let mut state = Picker {
        range,
        entry: None,
        week_column: week_column(),
    };
    popup_with_paste(
        tui,
        &mut state,
        |area, state: &Picker| {
            let rows =
                body_lines(skin, state).len() as u16 + MODAL_CHROME_HEIGHT;
            let width = BOX_WIDTH
                + MODAL_PADDING_WIDTH
                + state.week_column.extra_width();
            centered_rect(width, rows, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &Picker| {
            let inner = overlay::framed(frame, rect, skin, title);
            let lines = body_lines(skin, state);
            frame.render_widget(Paragraph::new(lines), inner);
        },
        handle_key,
        |state, text| handle_paste(state, &text),
    )
}

/// Applies one key in whichever mode the picker is in, or reports that it is
/// done.
///
/// A named function rather than a closure inside `popup_with_paste`, so the
/// guards below are reachable from a test: everything in the popup loop needs a
/// live terminal.
fn handle_key(
    state: &mut Picker,
    key: KeyEvent,
) -> PopupFlow<(NaiveDate, NaiveDate)> {
    match state.entry {
        Some(_) => handle_entry_key(state, key),
        None => handle_calendar_key(state, key),
    }
}

/// Applies one key while the entry field is open: `Enter` returns the typed
/// range once it parses, `Esc` closes the field and puts the calendar back, and
/// everything else edits the field - including the `Ctrl` chords the calendar
/// branch refuses.
fn handle_entry_key(
    state: &mut Picker,
    key: KeyEvent,
) -> PopupFlow<(NaiveDate, NaiveDate)> {
    let Some(entry) = state.entry.as_mut() else {
        return PopupFlow::Continue;
    };
    match entry.handle_key(key) {
        EntryAction::Committed => match parse_entry(entry) {
            Some(pair) => PopupFlow::Done(pair),
            None => PopupFlow::Continue,
        },
        EntryAction::Aborted => {
            state.range = entry.snapshot();
            state.entry = None;
            PopupFlow::Continue
        }
        EntryAction::Edited => {
            preview(state);
            PopupFlow::Continue
        }
    }
}

/// Applies one key while the calendar has the keyboard.
fn handle_calendar_key(
    state: &mut Picker,
    key: KeyEvent,
) -> PopupFlow<(NaiveDate, NaiveDate)> {
    // The grid moves on bare keys only: in raw mode crossterm reports Ctrl+H as
    // `Char('h') + CONTROL`, so without this guard a chord would silently walk
    // the day cursor. The entry field needs those chords and is handled before
    // this branch, never behind the guard.
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            state.range.cursor = shift(state.range.cursor, -1);
            PopupFlow::Continue
        }
        KeyCode::Right | KeyCode::Char('l') => {
            state.range.cursor = shift(state.range.cursor, 1);
            PopupFlow::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.range.cursor = shift(state.range.cursor, -7);
            PopupFlow::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.range.cursor = shift(state.range.cursor, 7);
            PopupFlow::Continue
        }
        KeyCode::PageUp => {
            state.range.cursor = add_months(state.range.cursor, -1);
            PopupFlow::Continue
        }
        KeyCode::PageDown => {
            state.range.cursor = add_months(state.range.cursor, 1);
            PopupFlow::Continue
        }
        KeyCode::Home => {
            state.range.cursor = first_of_month(state.range.cursor);
            PopupFlow::Continue
        }
        KeyCode::End => {
            state.range.cursor = last_of_month(state.range.cursor);
            PopupFlow::Continue
        }
        KeyCode::Char(OPEN_KEY) => {
            state.entry = Some(DateEntry::open(
                &entry_text(state.range),
                ENTRY_MAX_LEN,
                state.range,
            ));
            PopupFlow::Continue
        }
        KeyCode::Enter => match state.range.start {
            None => {
                state.range.start = Some(state.range.cursor);
                PopupFlow::Continue
            }
            Some(begin) => PopupFlow::Done(ordered(begin, state.range.cursor)),
        },
        KeyCode::Esc => PopupFlow::Cancelled,
        _ => PopupFlow::Continue,
    }
}

/// Routes a bracketed paste into the entry field; the calendar has nothing to
/// paste into and ignores it.
fn handle_paste(
    state: &mut Picker,
    text: &str,
) -> PopupFlow<(NaiveDate, NaiveDate)> {
    if let Some(entry) = state.entry.as_mut() {
        entry.paste(text);
        preview(state);
    }
    PopupFlow::Continue
}

/// The text the field opens with: the fixed start (or the cursor, while none is
/// fixed) through the cursor.
fn entry_text(range: Range) -> String {
    let begin = range.start.unwrap_or(range.cursor);
    format!(
        "{}{}{}",
        begin.format(ISO_FORMAT),
        date_text::RANGE_SEPARATOR,
        range.cursor.format(ISO_FORMAT),
    )
}

/// The parse behind the typed entry, yielding `None` while either end is not a
/// real date. The forms are those of [`date_text`]; an end typed without a year
/// borrows the other end's, or else the cursor year the field opened over.
///
/// A reversed pair is accepted and normalised through [`ordered`], exactly as
/// the calendar's second `Enter` does.
fn parse_entry(entry: &DateEntry<Range>) -> Option<(NaiveDate, NaiveDate)> {
    let default_year = entry.snapshot().cursor.year();
    let (begin, end) = date_text::parse_range(entry.text(), default_year)?;
    Some(ordered(begin, end))
}

/// Highlights both typed ends as soon as the text parses: the earlier day
/// becomes the fixed start, the later one the cursor, which also picks the
/// visible month.
fn preview(state: &mut Picker) {
    let Some(entry) = state.entry.as_ref() else {
        return;
    };
    if let Some((begin, end)) = parse_entry(entry) {
        state.range = Range {
            cursor: end,
            start: Some(begin),
        };
    }
}

/// Orders two dates into `(earlier, later)`.
pub(crate) fn ordered(a: NaiveDate, b: NaiveDate) -> (NaiveDate, NaiveDate) {
    if a <= b { (a, b) } else { (b, a) }
}

fn body_lines(skin: &Skin, state: &Picker) -> Vec<Line<'static>> {
    let palette = &skin.palette;
    let today = today();
    let range = state.range;
    let width = INNER_WIDTH + usize::from(state.week_column.extra_width());
    let gutter = state.week_column.gutter(skin, today);
    let mut lines = vec![
        header_line(skin, state, width),
        weekday_header(palette, gutter.as_ref()),
    ];

    lines.extend(day_grid(range.cursor, gutter.as_ref(), |day| {
        day_style(skin, day, range.cursor, range.start, today)
    }));

    lines.extend(hint_lines(skin, state, width));
    lines
}

/// The range header centred in `width`, or the entry field drawn in its place.
fn header_line(skin: &Skin, state: &Picker, width: usize) -> Line<'static> {
    let palette = &skin.palette;
    let Some(entry) = &state.entry else {
        let range = state.range;
        let header = match range.start {
            None => {
                format!("pick start \u{b7} {}", range.cursor.format(ISO_FORMAT))
            }
            Some(begin) => {
                let (lo, hi) = ordered(begin, range.cursor);
                format!(
                    "{} \u{2192} {}",
                    lo.format("%m-%d"),
                    hi.format("%m-%d")
                )
            }
        };
        let pad = width.saturating_sub(header.len()) / 2;
        return Line::from(Span::styled(
            format!("{}{header}", " ".repeat(pad)),
            style::fg(palette.accent).add_modifier(Modifier::BOLD),
        ));
    };
    let validity = Validity::of(parse_entry(entry).is_some());
    entry.header_line(palette, width, validity)
}

/// The hint footer for the current mode, wrapped to `width`, at a height the
/// mode does not change.
///
/// The edit token sits second on purpose: it shares the first row with the
/// short `move` token there, which keeps the footer two rows tall.
fn hint_lines(skin: &Skin, state: &Picker, width: usize) -> Vec<Line<'static>> {
    let calendar = [
        ("\u{2190}\u{2192}\u{2191}\u{2193}", "move"),
        date_entry::EDIT_HINT,
        ("Enter", "start/end"),
    ];
    let palette = &skin.palette;
    match state.entry {
        Some(_) => date_entry::hint_lines(
            &date_entry::ENTRY_HINTS,
            &calendar,
            palette,
            width,
        ),
        None => date_entry::hint_lines(
            &calendar,
            &date_entry::ENTRY_HINTS,
            palette,
            width,
        ),
    }
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
        cursor_cell_style(palette)
    } else if in_range {
        style::bg(palette.selection)
    } else if day == today {
        today_cell_style(palette)
    } else if is_weekend(day) {
        style::secondary(palette)
    } else {
        Style::default().fg(Color::Reset)
    }
}

#[cfg(test)]
mod tests;
