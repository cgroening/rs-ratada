//! Month picker modal: a `YYYY-MM` header over a 3x4 Jan-Dec grid.
//!
//! Arrows / `hjkl` move within the grid, `PageUp`/`PageDown` change the year,
//! `Home`/`End` jump to January/December, `Enter` picks, `Del`/`Backspace`
//! clears (when allowed) and `Esc` cancels.
//!
//! `i` turns the header into a text field prefilled with the current
//! `YYYY-MM` and fully selected, for a year too far away to page to. The grid
//! follows the typed value as soon as it parses; while it does not, the field
//! is coloured as an error and `Enter` does nothing. `Enter` returns the typed
//! month, `Esc` leaves the field and puts the grid back.

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
    date_entry::{self, DateEntry, EntryAction, OPEN_KEY, Validity},
    date_picker::{cursor_cell_style, today_cell_style},
    input,
    layout::centered_rect,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup_with_paste},
    style,
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
/// The separator between the year and the month in the typed entry.
const MONTH_SEPARATOR: char = '-';
/// The characters `YYYY-MM` takes, and with it the entry field's limit.
const ENTRY_MAX_LEN: usize = 7;

/// The picked year and month (1-12).
#[derive(Clone, Copy, PartialEq, Eq)]
struct Month {
    year: i32,
    month: u32,
}

/// The picker state: the grid position plus the typed-entry field while it is
/// open.
struct Picker {
    month: Month,
    entry: Option<DateEntry<Month>>,
}

/// Opens the month grid at `current` (or the local month). `i` opens a field
/// for typing the month instead of paging to it. Returns
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
    let mut state = Picker {
        month: Month { year, month },
        entry: None,
    };
    popup_with_paste(
        tui,
        &mut state,
        |area, state: &Picker| {
            let rows = body_lines(&skin.palette, state, today, allow_clear)
                .len() as u16
                + 2;
            centered_rect(BOX_WIDTH, rows, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &Picker| {
            let inner = overlay::framed(frame, rect, skin, prompt);
            let lines = body_lines(&skin.palette, state, today, allow_clear);
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
) -> PopupFlow<Option<(i32, u32)>> {
    match state.entry {
        Some(_) => handle_entry_key(state, key),
        None => handle_grid_key(state, key, allow_clear),
    }
}

/// Applies one key while the entry field is open: `Enter` returns the typed
/// month once it parses, `Esc` closes the field and puts the grid back, and
/// everything else edits the field - including the `Ctrl` chords the grid
/// branch refuses.
fn handle_entry_key(
    state: &mut Picker,
    key: KeyEvent,
) -> PopupFlow<Option<(i32, u32)>> {
    let Some(entry) = state.entry.as_mut() else {
        return PopupFlow::Continue;
    };
    match entry.handle_key(key) {
        EntryAction::Committed => match parse_entry(entry.text()) {
            Some(month) => PopupFlow::Done(Some((month.year, month.month))),
            None => PopupFlow::Continue,
        },
        EntryAction::Aborted => {
            state.month = entry.snapshot();
            state.entry = None;
            PopupFlow::Continue
        }
        EntryAction::Edited => {
            preview(state);
            PopupFlow::Continue
        }
    }
}

/// Applies one key while the month grid has the keyboard.
fn handle_grid_key(
    state: &mut Picker,
    key: KeyEvent,
    allow_clear: bool,
) -> PopupFlow<Option<(i32, u32)>> {
    // The grid moves on bare keys only: in raw mode crossterm reports Ctrl+H as
    // `Char('h') + CONTROL`, so without this guard a chord would silently walk
    // the month cursor. The entry field needs those chords and is handled
    // before this branch, never behind the guard.
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            state.month.month = step(state.month.month, -1);
            PopupFlow::Continue
        }
        KeyCode::Right | KeyCode::Char('l') => {
            state.month.month = step(state.month.month, 1);
            PopupFlow::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.month.month = step(state.month.month, -3);
            PopupFlow::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.month.month = step(state.month.month, 3);
            PopupFlow::Continue
        }
        KeyCode::PageUp => {
            state.month.year -= 1;
            PopupFlow::Continue
        }
        KeyCode::PageDown => {
            state.month.year += 1;
            PopupFlow::Continue
        }
        KeyCode::Home => {
            state.month.month = 1;
            PopupFlow::Continue
        }
        KeyCode::End => {
            state.month.month = 12;
            PopupFlow::Continue
        }
        KeyCode::Char(OPEN_KEY) => {
            state.entry = Some(DateEntry::open(
                &entry_text(state.month),
                ENTRY_MAX_LEN,
                state.month,
            ));
            PopupFlow::Continue
        }
        KeyCode::Enter => {
            PopupFlow::Done(Some((state.month.year, state.month.month)))
        }
        KeyCode::Delete | KeyCode::Backspace if allow_clear => {
            PopupFlow::Done(None)
        }
        KeyCode::Esc => PopupFlow::Cancelled,
        _ => PopupFlow::Continue,
    }
}

/// Routes a bracketed paste into the entry field; the grid has nothing to paste
/// into and ignores it.
fn handle_paste(
    state: &mut Picker,
    text: &str,
) -> PopupFlow<Option<(i32, u32)>> {
    if let Some(entry) = state.entry.as_mut() {
        entry.paste(text);
        preview(state);
    }
    PopupFlow::Continue
}

/// The header's `YYYY-MM`, which is also what the field opens with.
fn entry_text(month: Month) -> String {
    format!("{:04}{MONTH_SEPARATOR}{:02}", month.year, month.month)
}

/// The `YYYY-MM` parse behind the typed entry, yielding `None` while the text
/// is not a real month - an empty field included.
///
/// The month must be 1-12, so `2026-13` stays invalid instead of silently
/// clamping the way the grid keys do.
fn parse_entry(text: &str) -> Option<Month> {
    let (year, month) = text.split_once(MONTH_SEPARATOR)?;
    let year = year.parse().ok()?;
    let month = month.parse().ok()?;
    (1..=12).contains(&month).then_some(Month { year, month })
}

/// Moves the grid onto the typed month as soon as the text parses, so year and
/// grid follow the value being typed.
fn preview(state: &mut Picker) {
    let Some(entry) = state.entry.as_ref() else {
        return;
    };
    if let Some(month) = parse_entry(entry.text()) {
        state.month = month;
    }
}

fn step(month: u32, delta: i32) -> u32 {
    (month as i32 + delta).clamp(1, 12) as u32
}

fn body_lines(
    palette: &Palette,
    state: &Picker,
    today: chrono::NaiveDate,
    allow_clear: bool,
) -> Vec<Line<'static>> {
    let Month { year, month } = state.month;
    let mut lines = vec![header_line(palette, state)];

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

    lines.extend(hint_lines(palette, state, allow_clear));
    lines
}

/// The centred `YYYY-MM`, or the entry field drawn in its place.
fn header_line(palette: &Palette, state: &Picker) -> Line<'static> {
    let Some(entry) = &state.entry else {
        let header = entry_text(state.month);
        let pad = INNER_WIDTH.saturating_sub(header.len()) / 2;
        return Line::from(Span::styled(
            format!("{}{header}", " ".repeat(pad)),
            style::fg(palette.accent).add_modifier(Modifier::BOLD),
        ));
    };
    let validity = Validity::of(parse_entry(entry.text()).is_some());
    entry.header_line(palette, INNER_WIDTH, validity)
}

/// The hint footer for the current mode, at a height the mode does not change.
fn hint_lines(
    palette: &Palette,
    state: &Picker,
    allow_clear: bool,
) -> Vec<Line<'static>> {
    let grid = grid_hints(allow_clear);
    match state.entry {
        Some(_) => date_entry::hint_lines(
            &date_entry::ENTRY_HINTS,
            &grid,
            palette,
            INNER_WIDTH,
        ),
        None => date_entry::hint_lines(
            &grid,
            &date_entry::ENTRY_HINTS,
            palette,
            INNER_WIDTH,
        ),
    }
}

/// The grid-mode hints; `del clear` only where clearing is allowed.
fn grid_hints(allow_clear: bool) -> Vec<(&'static str, &'static str)> {
    let mut hints = vec![
        ("\u{2191}\u{2193}\u{2190}\u{2192}", "move"),
        date_entry::EDIT_HINT,
        ("pgup/dn", "year"),
        ("enter", "pick"),
    ];
    if allow_clear {
        hints.push(("del", "clear"));
    }
    hints
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

    fn picker() -> Picker {
        Picker {
            month: Month {
                year: 2026,
                month: 6,
            },
            entry: None,
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

    /// Replaces the open field's content with `text`.
    fn retype(state: &mut Picker, text: &str) {
        handle_key(state, ctrl(KeyCode::Char('a')), true);
        handle_key(state, press(KeyCode::Delete), true);
        type_text(state, text);
    }

    /// `Ctrl+H`/`Ctrl+J` arrive as plain characters in raw mode, so without the
    /// guard a chord would walk the month cursor - or, for `Ctrl+I`, open the
    /// entry field.
    #[test]
    fn ctrl_chords_do_not_move_the_month_cursor() {
        for code in [
            KeyCode::Char('h'),
            KeyCode::Char('i'),
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
            let mut state = picker();
            assert!(matches!(
                handle_key(&mut state, ctrl(code), true),
                PopupFlow::Continue
            ));
            assert_eq!(state.month.month, 6, "Ctrl+{code:?} moved the month");
            assert_eq!(state.month.year, 2026, "Ctrl+{code:?} moved the year");
            assert!(state.entry.is_none(), "Ctrl+{code:?} opened the field");
        }
    }

    #[test]
    fn bare_keys_still_move_pick_and_clear() {
        let mut state = picker();

        handle_key(&mut state, press(KeyCode::Char('l')), true);
        assert_eq!(state.month.month, 7);
        handle_key(&mut state, press(KeyCode::Char('k')), true);
        assert_eq!(state.month.month, 4);
        handle_key(&mut state, press(KeyCode::PageUp), true);
        assert_eq!(state.month.year, 2025);
        handle_key(&mut state, press(KeyCode::End), true);
        assert_eq!(state.month.month, 12);

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
    fn i_prefills_the_year_and_month() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('i')), true);
        let entry = state.entry.as_ref().expect("the field is open");
        assert_eq!(entry.text(), "2026-06");
    }

    /// The grid follows the typed value, so the month is confirmed by sight
    /// before `Enter` ever returns it.
    #[test]
    fn a_typed_month_moves_the_grid_live() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('i')), true);
        retype(&mut state, "2025-01");
        assert_eq!(state.month.year, 2025);
        assert_eq!(state.month.month, 1);
    }

    /// The grid keys clamp into the year; the typed entry must not, or a
    /// mistyped `2026-13` would silently become December.
    #[test]
    fn a_month_outside_one_to_twelve_is_invalid() {
        for text in ["", "2026", "2026-13", "2026-00"] {
            let mut state = picker();
            handle_key(&mut state, press(KeyCode::Char('i')), true);
            retype(&mut state, text);
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
    fn enter_returns_the_typed_month() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('i')), true);
        retype(&mut state, "2025-01");
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Enter), true),
            PopupFlow::Done(Some((2025, 1)))
        ));
    }

    #[test]
    fn esc_restores_the_year_and_month() {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('i')), true);
        retype(&mut state, "2025-01");
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Esc), true),
            PopupFlow::Continue
        ));
        assert!(state.entry.is_none());
        assert_eq!(state.month.year, 2026);
        assert_eq!(state.month.month, 6);
        assert!(matches!(
            handle_key(&mut state, press(KeyCode::Esc), true),
            PopupFlow::Cancelled
        ));
    }

    /// The `Ctrl` guard belongs to the grid branch alone: in the field the same
    /// chords must edit, or `Ctrl+U`/`Ctrl+V` would be dead there.
    #[test]
    fn ctrl_chords_edit_in_entry_mode_but_never_in_the_grid() {
        let mut state = picker();
        handle_key(&mut state, ctrl(KeyCode::Char('u')), true);
        assert_eq!(state.month.month, 6);

        handle_key(&mut state, press(KeyCode::Char('i')), true);
        handle_key(&mut state, press(KeyCode::End), true);
        handle_key(&mut state, ctrl(KeyCode::Char('u')), true);
        let entry = state.entry.as_ref().expect("the field is open");
        assert_eq!(entry.text(), "");
        assert_eq!(state.month.month, 6);
    }

    #[test]
    fn a_paste_reaches_the_field_only_while_it_is_open() {
        let mut state = picker();
        handle_paste(&mut state, "2025-01");
        assert_eq!(state.month.year, 2026);

        handle_key(&mut state, press(KeyCode::Char('i')), true);
        handle_paste(&mut state, "2025-01");
        assert_eq!(state.month.year, 2025);
        assert_eq!(state.month.month, 1);
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
