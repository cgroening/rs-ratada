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
            let rows = body_lines(skin, state).len() as u16 + 2;
            let width = BOX_WIDTH + state.week_column.extra_width();
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
        ("enter", "start/end"),
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
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    fn day(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, day).expect("a real date")
    }

    fn picker() -> Picker {
        Picker {
            range: Range {
                cursor: day(15),
                start: None,
            },
            entry: None,
            week_column: WeekColumn::Shown,
        }
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    /// Types `text` into the open entry field, one key at a time.
    fn type_text(state: &mut Picker, text: &str) {
        for character in text.chars() {
            handle_key(state, press(KeyCode::Char(character)));
        }
    }

    /// Replaces the open field's content with `text`.
    fn retype(state: &mut Picker, text: &str) {
        handle_key(state, ctrl(KeyCode::Char('a')));
        handle_key(state, press(KeyCode::Delete));
        type_text(state, text);
    }

    /// `Ctrl+H`/`Ctrl+J` arrive as plain characters in raw mode, so without the
    /// guard a chord would walk the day cursor - or, for `Ctrl+E`, open the
    /// entry field.
    #[test]
    fn ctrl_chords_do_not_move_the_day_cursor() {
        for code in [
            KeyCode::Char('h'),
            KeyCode::Char('e'),
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char('l'),
            KeyCode::Left,
            KeyCode::Down,
            KeyCode::PageUp,
            KeyCode::Home,
        ] {
            let mut state = picker();
            assert!(matches!(
                handle_key(&mut state, ctrl(code)),
                PopupFlow::Continue
            ));
            assert_eq!(
                state.range.cursor,
                day(15),
                "Ctrl+{code:?} moved the cursor",
            );
            assert!(
                state.range.start.is_none(),
                "Ctrl+{code:?} fixed the start",
            );
            assert!(state.entry.is_none(), "Ctrl+{code:?} opened the field");
        }
    }

    #[test]
    fn bare_keys_still_move_and_pick_both_ends() {
        let mut state = picker();

        handle_key(&mut state, press(KeyCode::Char('l')));
        assert_eq!(state.range.cursor, day(16));
        handle_key(&mut state, press(KeyCode::Char('j')));
        assert_eq!(state.range.cursor, day(23));

        // The first Enter fixes the start, the second returns the pair.
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Enter)),
            PopupFlow::Continue
        ));
        assert_eq!(state.range.start, Some(day(23)));
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
    fn e_prefills_the_cursor_twice_without_a_fixed_start() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('e')));
        let entry = state.entry.as_ref().expect("the field is open");
        assert_eq!(entry.text(), "2026-06-15..2026-06-15");
    }

    #[test]
    fn e_prefills_the_fixed_start_through_the_cursor() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Enter));
        handle_key(&mut state, press(KeyCode::Char('l')));
        handle_key(&mut state, press(KeyCode::Char('e')));
        let entry = state.entry.as_ref().expect("the field is open");
        assert_eq!(entry.text(), "2026-06-15..2026-06-16");
    }

    /// Both ends light up while typing, so the range is confirmed by sight
    /// before `Enter` ever returns it.
    #[test]
    fn a_typed_range_highlights_both_ends_live() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('e')));
        retype(&mut state, "2026-06-10..2026-06-20");
        assert_eq!(state.range.start, Some(day(10)));
        assert_eq!(state.range.cursor, day(20));
    }

    /// Both ends lack a year, so both fall in the cursor's year (2026).
    #[test]
    fn a_typed_german_range_highlights_both_ends_live() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('e')));
        retype(&mut state, "10.6...20.6.");
        assert_eq!(state.range.start, Some(day(10)));
        assert_eq!(state.range.cursor, day(20));
    }

    #[test]
    fn a_reversed_typed_range_is_normalised() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('e')));
        retype(&mut state, "2026-06-20..2026-06-10");
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Enter)),
            PopupFlow::Done(pair) if pair == (day(10), day(20)),
        ));
    }

    #[test]
    fn an_incomplete_range_blocks_enter() {
        for text in ["", "2026-06-20", "2026-06-20..", "2026-06-20..2026-13-01"]
        {
            let mut state = picker();
            handle_key(&mut state, press(KeyCode::Char('e')));
            retype(&mut state, text);
            assert!(
                matches!(
                    handle_key(&mut state, press(KeyCode::Enter)),
                    PopupFlow::Continue
                ),
                "{text:?} was accepted",
            );
            assert!(state.entry.is_some(), "{text:?} closed the field");
        }
    }

    /// The preview fixes a start that may not have existed, so restoring only
    /// the cursor would leave a phantom range highlighted.
    #[test]
    fn esc_restores_both_the_start_and_the_cursor() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('e')));
        retype(&mut state, "2026-06-10..2026-06-20");
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Esc)),
            PopupFlow::Continue
        ));
        assert!(state.entry.is_none());
        assert_eq!(state.range.cursor, day(15));
        assert_eq!(state.range.start, None);
    }

    /// The `Ctrl` guard belongs to the calendar branch alone: in the field the
    /// same chords must edit, or `Ctrl+U`/`Ctrl+V` would be dead there.
    #[test]
    fn ctrl_chords_edit_in_entry_mode_but_never_in_the_calendar() {
        let mut state = picker();
        handle_key(&mut state, ctrl(KeyCode::Char('u')));
        assert_eq!(state.range.cursor, day(15));

        handle_key(&mut state, press(KeyCode::Char('e')));
        handle_key(&mut state, press(KeyCode::End));
        handle_key(&mut state, ctrl(KeyCode::Char('u')));
        let entry = state.entry.as_ref().expect("the field is open");
        assert_eq!(entry.text(), "");
        assert_eq!(state.range.cursor, day(15));
    }

    #[test]
    fn a_paste_reaches_the_field_only_while_it_is_open() {
        let mut state = picker();
        handle_paste(&mut state, "2026-06-10..2026-06-20");
        assert_eq!(state.range.cursor, day(15));

        handle_key(&mut state, press(KeyCode::Char('e')));
        handle_paste(&mut state, "2026-06-10..2026-06-20");
        assert_eq!(state.range.start, Some(day(10)));
        assert_eq!(state.range.cursor, day(20));
    }

    #[test]
    fn ordered_sorts_the_pair() {
        let early = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let late = NaiveDate::from_ymd_opt(2026, 1, 20).unwrap();
        assert_eq!(ordered(late, early), (early, late));
        assert_eq!(ordered(early, late), (early, late));
    }
}
