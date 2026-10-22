//! Calendar date picker modal.
//!
//! A centred ISO date over a Monday-first month grid with today, the cursor day
//! and weekends coloured. Arrows / `hjkl` move by day/week, `PageUp`/`PageDown`
//! jump whole months, `Home`/`End` jump to the first/last day of the visible
//! month, `Enter` picks, `Del`/`Backspace` clears (when allowed) and `Esc`
//! cancels.
//!
//! `i` turns the header into a text field prefilled with the current date and
//! fully selected, for a date too far away to walk to. The calendar follows the
//! typed `%Y-%m-%d` value as soon as it parses; while it does not, the field is
//! coloured as an error and `Enter` does nothing. `Enter` returns the typed
//! date, `Esc` leaves the field and puts the cursor back.
//!
//! A column left of the grid shows the ISO calendar week of each row, the
//! cursor's week in the accent and today's in the muted accent. It is on by
//! default; [`set_week_column`] hides it for this and the range picker.

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
    date_entry::{self, DateEntry, EntryAction, OPEN_KEY, Validity},
    input,
    layout::centered_rect,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup_with_paste},
    style,
    terminal::Tui,
};
use crate::{
    theme::{Palette, Skin},
    week_column::WeekGutter,
};

pub use crate::week_column::{WeekColumn, set_week_column, week_column};

const BOX_WIDTH: u16 = 24;
const INNER_WIDTH: usize = 22;
/// The one format the header prints and the typed entry accepts, shared with
/// the range picker so both spell a day the same way.
pub(super) const ISO_FORMAT: &str = "%Y-%m-%d";
/// The characters an ISO date takes, and with it the entry field's limit.
const ENTRY_MAX_LEN: usize = 10;
/// The Monday-first weekday headers, shared by the calendar pickers.
const WEEKDAY_NAMES: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
/// The weekday index (Monday = 0) at which the weekend begins (Saturday).
const WEEKEND_START: u32 = 5;

/// The picker state: the day under the cursor, the typed-entry field while it
/// is open, and the week-column setting the picker opened with.
struct Picker {
    cursor: NaiveDate,
    entry: Option<DateEntry<NaiveDate>>,
    week_column: WeekColumn,
}

/// Opens the calendar at `current` (or today). `allow_clear` lets `Del` return
/// an empty date, and `i` opens a field for typing the date instead of walking
/// to it. Returns `Value(Some(date))` when picked, `Value(None)` when cleared,
/// `Cancelled` on `Esc`, `Quit` on the global quit chord.
pub fn date_picker(
    tui: &mut Tui,
    skin: &Skin,
    prompt: &str,
    current: Option<NaiveDate>,
    allow_clear: bool,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Option<NaiveDate>>> {
    let mut state = Picker {
        cursor: current.unwrap_or_else(today),
        entry: None,
        week_column: week_column(),
    };
    popup_with_paste(
        tui,
        &mut state,
        |area, state: &Picker| {
            let rows = body_lines(skin, state, allow_clear).len() as u16 + 2;
            let width = BOX_WIDTH + state.week_column.extra_width();
            centered_rect(width, rows, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &Picker| {
            let inner = overlay::framed(frame, rect, skin, prompt);
            let lines = body_lines(skin, state, allow_clear);
            frame.render_widget(Paragraph::new(lines), inner);
        },
        |state, key| handle_key(state, key, allow_clear),
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
    allow_clear: bool,
) -> PopupFlow<Option<NaiveDate>> {
    match state.entry {
        Some(_) => handle_entry_key(state, key),
        None => handle_calendar_key(state, key, allow_clear),
    }
}

/// Applies one key while the entry field is open: `Enter` returns the typed
/// date once it parses, `Esc` closes the field and puts the cursor back, and
/// everything else edits the field - including the `Ctrl` chords the calendar
/// branch refuses.
fn handle_entry_key(
    state: &mut Picker,
    key: KeyEvent,
) -> PopupFlow<Option<NaiveDate>> {
    let Some(entry) = state.entry.as_mut() else {
        return PopupFlow::Continue;
    };
    match entry.handle_key(key) {
        EntryAction::Committed => match parse_entry(entry.text()) {
            Some(date) => PopupFlow::Done(Some(date)),
            None => PopupFlow::Continue,
        },
        EntryAction::Aborted => {
            state.cursor = entry.snapshot();
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
    allow_clear: bool,
) -> PopupFlow<Option<NaiveDate>> {
    // The grid moves on bare keys only: in raw mode crossterm reports Ctrl+H as
    // `Char('h') + CONTROL`, so without this guard a chord would silently walk
    // the day cursor. The entry field needs those chords and is handled before
    // this branch, never behind the guard.
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
        KeyCode::Char(OPEN_KEY) => {
            state.entry = Some(DateEntry::open(
                &state.cursor.format(ISO_FORMAT).to_string(),
                ENTRY_MAX_LEN,
                state.cursor,
            ));
            PopupFlow::Continue
        }
        KeyCode::Enter => PopupFlow::Done(Some(state.cursor)),
        KeyCode::Delete | KeyCode::Backspace if allow_clear => {
            PopupFlow::Done(None)
        }
        KeyCode::Esc => PopupFlow::Cancelled,
        _ => PopupFlow::Continue,
    }
}

/// Routes a bracketed paste into the entry field; the calendar has nothing to
/// paste into and ignores it.
fn handle_paste(
    state: &mut Picker,
    text: &str,
) -> PopupFlow<Option<NaiveDate>> {
    if let Some(entry) = state.entry.as_mut() {
        entry.paste(text);
        preview(state);
    }
    PopupFlow::Continue
}

/// Moves the calendar onto the typed date as soon as the text parses, so the
/// grid and the visible month follow the value being typed.
fn preview(state: &mut Picker) {
    let Some(entry) = state.entry.as_ref() else {
        return;
    };
    if let Some(date) = parse_entry(entry.text()) {
        state.cursor = date;
    }
}

/// The [`ISO_FORMAT`] parse behind the typed entry, yielding `None` while the
/// text is not a real date - an empty field included.
///
/// Unpadded components (`2026-6-5`) are accepted, as chrono parses them; the
/// format is what is strict here, not the digit count.
fn parse_entry(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, ISO_FORMAT).ok()
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
    skin: &Skin,
    state: &Picker,
    allow_clear: bool,
) -> Vec<Line<'static>> {
    let palette = &skin.palette;
    let today = today();
    let width = INNER_WIDTH + usize::from(state.week_column.extra_width());
    let gutter = state.week_column.gutter(skin, today);
    let mut lines = vec![
        header_line(palette, state, width),
        weekday_header(palette, gutter.as_ref()),
    ];

    lines.extend(day_grid(state.cursor, gutter.as_ref(), |day| {
        day_style(palette, day, state.cursor, today)
    }));

    lines.extend(hint_lines(palette, state, allow_clear, width));
    lines
}

/// The ISO date centred in `width`, or the entry field drawn in its place.
fn header_line(
    palette: &Palette,
    state: &Picker,
    width: usize,
) -> Line<'static> {
    let Some(entry) = &state.entry else {
        let iso = state.cursor.format(ISO_FORMAT).to_string();
        let pad = width.saturating_sub(iso.len()) / 2;
        return Line::from(Span::styled(
            format!("{}{iso}", " ".repeat(pad)),
            style::fg(palette.accent).add_modifier(Modifier::BOLD),
        ));
    };
    let validity = Validity::of(parse_entry(entry.text()).is_some());
    entry.header_line(palette, width, validity)
}

/// The hint footer for the current mode, wrapped to `width`, at a height the
/// mode does not change.
fn hint_lines(
    palette: &Palette,
    state: &Picker,
    allow_clear: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let calendar = calendar_hints(allow_clear);
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

/// The calendar-mode hints; `del clear` only where clearing is allowed.
///
/// The edit token sits second on purpose: it shares the first row with the
/// short `move` token there, which keeps the footer two rows tall.
fn calendar_hints(allow_clear: bool) -> Vec<(&'static str, &'static str)> {
    let mut hints = vec![
        ("\u{2190}\u{2192}\u{2191}\u{2193}", "move"),
        date_entry::EDIT_HINT,
        ("enter", "pick"),
    ];
    if allow_clear {
        hints.push(("del", "clear"));
    }
    hints
}

/// The Monday-first weekday header row (`Mo Tu We Th Fr Sa Su`), shared by the
/// calendar pickers; with a `gutter`, the week column's label leads it.
pub(super) fn weekday_header(
    palette: &Palette,
    gutter: Option<&WeekGutter>,
) -> Line<'static> {
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    if let Some(gutter) = gutter {
        spans.extend(gutter.header());
        spans.push(Span::raw(" "));
    }
    for (index, name) in WEEKDAY_NAMES.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(name, style::secondary(palette)));
    }
    Line::from(spans)
}

/// The month around `cursor` as week rows, each day styled by `style_of`. Shared
/// by the calendar pickers, which differ only in how a day is styled. With a
/// `gutter`, each row is led by its ISO week number.
pub(super) fn day_grid(
    cursor: NaiveDate,
    gutter: Option<&WeekGutter>,
    style_of: impl Fn(NaiveDate) -> Style,
) -> Vec<Line<'static>> {
    month_cells(cursor)
        .chunks(7)
        .map(|week| {
            let mut spans: Vec<Span> = vec![Span::raw(" ")];
            if let Some(gutter) = gutter {
                spans.extend(week_cell(gutter, week, cursor));
                spans.push(Span::raw(" "));
            }
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

/// The week column's cell for one grid row, taken from the row's first day:
/// every day of a Monday-first row shares one ISO week.
fn week_cell(
    gutter: &WeekGutter,
    week: &[Option<NaiveDate>],
    cursor: NaiveDate,
) -> [Span<'static>; 2] {
    match week.iter().flatten().next() {
        Some(day) => gutter.row(*day, cursor),
        None => gutter.blank(),
    }
}

/// Whether `day` falls on the weekend (Saturday or Sunday).
pub(super) fn is_weekend(day: NaiveDate) -> bool {
    day.weekday().num_days_from_monday() >= WEEKEND_START
}

/// The style of the calendar cell under the cursor (or a selected range end).
///
/// Shared by every calendar-style picker so the highlight looks the same in
/// all of them; the pickers differ in *when* they apply it, not in how it looks.
pub(super) fn cursor_cell_style(palette: &Palette) -> Style {
    style::bg(palette.selection)
        .fg(style::to_ratatui(palette.accent))
        .add_modifier(Modifier::BOLD)
}

/// The style marking today's cell, when it is not under the cursor.
pub(super) fn today_cell_style(palette: &Palette) -> Style {
    style::fg(palette.accent_dim).add_modifier(Modifier::BOLD)
}

fn day_style(
    palette: &Palette,
    day: NaiveDate,
    cursor: NaiveDate,
    today: NaiveDate,
) -> Style {
    if day == cursor {
        cursor_cell_style(palette)
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
    use crate::theme::{ColorOverrides, GlyphVariant, Glyphs, ThemeRegistry};

    /// The reference day every test starts from.
    fn start() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 15).expect("a real date")
    }

    fn picker() -> Picker {
        Picker {
            cursor: start(),
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
            handle_key(state, press(KeyCode::Char(character)), true);
        }
    }

    /// `Ctrl+H`/`Ctrl+J` arrive as plain characters in raw mode, so without the
    /// guard a chord would walk the day cursor - or, for `Ctrl+I`, open the
    /// entry field.
    #[test]
    fn ctrl_chords_do_not_move_the_day_cursor() {
        for code in [
            KeyCode::Char('h'),
            KeyCode::Char('i'),
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
                handle_key(&mut state, ctrl(code), true),
                PopupFlow::Continue
            ));
            assert_eq!(state.cursor, start(), "Ctrl+{code:?} moved the cursor");
            assert!(state.entry.is_none(), "Ctrl+{code:?} opened the field");
        }
    }

    #[test]
    fn bare_keys_still_move_pick_and_clear() {
        let mut state = picker();

        handle_key(&mut state, press(KeyCode::Char('l')), true);
        assert_eq!(state.cursor, shift(start(), 1));
        handle_key(&mut state, press(KeyCode::Char('j')), true);
        assert_eq!(state.cursor, shift(start(), 8));

        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Enter), true),
            PopupFlow::Done(Some(_))
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
    }

    #[test]
    fn i_opens_the_entry_field_prefilled_with_the_cursor() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('i')), true);
        let entry = state.entry.as_ref().expect("the field is open");
        assert_eq!(entry.text(), "2026-06-15");
    }

    /// The grid follows the typed value, so the day is confirmed by sight
    /// before `Enter` ever returns it.
    #[test]
    fn a_typed_date_moves_the_cursor_live() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('i')), true);
        type_text(&mut state, "2026-01-02");
        assert!(state.entry.is_some());
        assert_eq!(
            state.cursor,
            NaiveDate::from_ymd_opt(2026, 1, 2).expect("a real date"),
        );
    }

    #[test]
    fn an_unparseable_entry_blocks_enter() {
        for text in ["", "2026-13-40", "2026-06"] {
            let mut state = picker();
            handle_key(&mut state, press(KeyCode::Char('i')), true);
            handle_key(&mut state, ctrl(KeyCode::Char('a')), true);
            handle_key(&mut state, press(KeyCode::Delete), true);
            type_text(&mut state, text);
            assert!(
                matches!(
                    handle_key(&mut state, press(KeyCode::Enter), true),
                    PopupFlow::Continue
                ),
                "{text:?} was accepted",
            );
            assert!(state.entry.is_some(), "{text:?} closed the field");
        }
    }

    #[test]
    fn enter_returns_the_typed_date() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('i')), true);
        type_text(&mut state, "2026-01-02");
        let expected =
            NaiveDate::from_ymd_opt(2026, 1, 2).expect("a real date");
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Enter), true),
            PopupFlow::Done(Some(date)) if date == expected
        ));
    }

    /// `Esc` unwinds one level at a time: the live preview is taken back before
    /// the modal itself closes.
    #[test]
    fn esc_leaves_entry_mode_and_restores_the_cursor() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('i')), true);
        type_text(&mut state, "2026-01-02");
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Esc), true),
            PopupFlow::Continue
        ));
        assert!(state.entry.is_none());
        assert_eq!(state.cursor, start());
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Esc), true),
            PopupFlow::Cancelled
        ));
    }

    /// The `Ctrl` guard belongs to the calendar branch alone: in the field the
    /// same chords must edit, or `Ctrl+U`/`Ctrl+V` would be dead there.
    #[test]
    fn ctrl_chords_edit_in_entry_mode_but_never_in_the_calendar() {
        let mut state = picker();
        handle_key(&mut state, ctrl(KeyCode::Char('u')), true);
        assert_eq!(state.cursor, start());

        handle_key(&mut state, press(KeyCode::Char('i')), true);
        handle_key(&mut state, press(KeyCode::End), true);
        handle_key(&mut state, ctrl(KeyCode::Char('u')), true);
        let entry = state.entry.as_ref().expect("the field is open");
        assert_eq!(entry.text(), "");
        // Editing the field never walks the day cursor.
        assert_eq!(state.cursor, start());
    }

    #[test]
    fn a_paste_reaches_the_field_only_while_it_is_open() {
        let mut state = picker();
        handle_paste(&mut state, "2026-01-02");
        assert_eq!(state.cursor, start());

        handle_key(&mut state, press(KeyCode::Char('i')), true);
        handle_paste(&mut state, "2026-01-02");
        assert_eq!(
            state.cursor,
            NaiveDate::from_ymd_opt(2026, 1, 2).expect("a real date"),
        );
    }

    #[test]
    fn delete_clears_only_in_calendar_mode() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('i')), true);
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Delete), true),
            PopupFlow::Continue
        ));
        let entry = state.entry.as_ref().expect("the field is open");
        assert_eq!(entry.text(), "");
    }

    fn skin() -> Skin {
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
        )
    }

    fn text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    /// The weekday header and the six week rows of June 2026.
    fn calendar_rows(state: &Picker) -> Vec<String> {
        body_lines(&skin(), state, true)[1..7]
            .iter()
            .map(text)
            .collect()
    }

    #[test]
    fn a_shown_week_column_leads_every_row_with_its_iso_week() {
        let rows = calendar_rows(&picker());
        assert_eq!(rows[0], " Wk \u{2502} Mo Tu We Th Fr Sa Su");
        assert_eq!(rows[1], " 23 \u{2502}  1  2  3  4  5  6  7");
        assert_eq!(rows[5], format!(" 27 \u{2502} 29 30{}", " ".repeat(15)));
        let width = INNER_WIDTH + usize::from(WeekColumn::Shown.extra_width());
        for row in &rows {
            assert!(row.chars().count() < width, "{row:?} fills the box");
        }
    }

    /// Hidden, the calendar is the one the pickers drew before the column.
    #[test]
    fn a_hidden_week_column_keeps_the_previous_calendar() {
        let state = Picker {
            week_column: WeekColumn::Hidden,
            ..picker()
        };
        let rows = calendar_rows(&state);
        assert_eq!(rows[0], " Mo Tu We Th Fr Sa Su");
        assert_eq!(rows[1], "  1  2  3  4  5  6  7");
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
