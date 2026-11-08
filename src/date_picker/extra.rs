//! The date picker with keys of the caller's own.
//!
//! An [`ExtraKey`] is a bare letter the calendar does not use itself, shown in
//! the hint footer after the picker's own keys. Pressing it closes the picker
//! with [`DatePick::Extra`], so a caller can offer a choice that is no date
//! ("E inbox") without a second dialog. While the typed entry is open, every
//! letter types into it.

use std::io;

use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, widgets::Paragraph};

use super::{
    BOX_WIDTH, MODAL_CHROME_HEIGHT, MODAL_PADDING_WIDTH, ModalSignal, OPEN_KEY,
    Picker, PopupFlow, Tui, body_lines, centered_rect, handle_key,
    handle_paste, input, overlay, popup_with_paste, today, week_column,
};
use crate::theme::Skin;

/// The keys the calendar uses itself; an extra key equal to one is ignored.
const RESERVED_KEYS: [char; 5] = ['h', 'j', 'k', 'l', OPEN_KEY];

/// A key the caller adds to the date picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtraKey {
    /// The key as typed and as the hint shows it: one character (`"E"`).
    pub key: &'static str,
    /// What it does, for the hint footer (`"inbox"`).
    pub label: &'static str,
}

impl ExtraKey {
    /// Returns the key's character, when `key` is exactly one character the
    /// calendar does not use itself.
    fn usable_char(&self) -> Option<char> {
        let mut chars = self.key.chars();
        let character = chars.next()?;
        let is_single = chars.next().is_none();
        (is_single && !RESERVED_KEYS.contains(&character)).then_some(character)
    }
}

/// What the picker returned: a date (or `None` when cleared), or the extra
/// key that closed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePick {
    /// A picked date, or `None` when the date was cleared.
    Date(Option<NaiveDate>),
    /// The character of the [`ExtraKey`] that closed the picker.
    Extra(char),
}

/// What the picker shows and allows.
#[derive(Debug, Clone, Copy)]
pub struct DatePickerOptions<'a> {
    /// The title of the box.
    pub prompt: &'a str,
    /// The day the cursor starts on; today when `None`.
    pub current: Option<NaiveDate>,
    /// Whether `Del`/`Backspace` returns an empty date.
    pub allow_clear: bool,
    /// The caller's own keys, in hint order. A key that is not one character,
    /// or that the calendar uses itself (`h`, `j`, `k`, `l`, `e`), is left out.
    pub extra: &'a [ExtraKey],
}

/// Opens the calendar like [`date_picker`], with the extra keys of `options`.
/// Returns `Value(DatePick::Date(..))` when a date is picked or cleared,
/// `Value(DatePick::Extra(key))` when an extra key closed it, `Cancelled` on
/// `Esc` and `Quit` on the global quit chord.
///
/// # Errors
///
/// Returns an error when drawing to or reading from the terminal fails.
pub fn date_picker_with(
    tui: &mut Tui,
    skin: &Skin,
    options: &DatePickerOptions<'_>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<DatePick>> {
    let allow_clear = options.allow_clear;
    let mut state = Picker {
        cursor: options.current.unwrap_or_else(today),
        entry: None,
        week_column: week_column(),
        extra: usable_keys(options.extra),
    };
    popup_with_paste(
        tui,
        &mut state,
        |area, state: &Picker| {
            let rows = body_lines(skin, state, allow_clear).len() as u16
                + MODAL_CHROME_HEIGHT;
            let width = BOX_WIDTH
                + MODAL_PADDING_WIDTH
                + state.week_column.extra_width();
            centered_rect(width, rows, area)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &Picker| {
            let inner = overlay::framed(frame, rect, skin, options.prompt);
            let lines = body_lines(skin, state, allow_clear);
            frame.render_widget(Paragraph::new(lines), inner);
        },
        |state, key| handle_picker_key(state, key, allow_clear),
        |state, text| as_pick(&handle_paste(state, &text)),
    )
}

/// The extra keys the picker can offer: one character each, none of the
/// calendar's own.
fn usable_keys(keys: &[ExtraKey]) -> Vec<ExtraKey> {
    keys.iter()
        .copied()
        .filter(|key| key.usable_char().is_some())
        .collect()
}

/// Applies one key: an extra key closes the picker while the calendar has the
/// keyboard, every other key goes to [`handle_key`].
fn handle_picker_key(
    state: &mut Picker,
    key: KeyEvent,
    allow_clear: bool,
) -> PopupFlow<DatePick> {
    if let Some(character) = extra_key_pressed(state, key) {
        return PopupFlow::Done(DatePick::Extra(character));
    }
    as_pick(&handle_key(state, key, allow_clear))
}

/// The character of the extra key `key` presses, if the calendar (not the
/// entry field) has the keyboard and it is a bare key.
fn extra_key_pressed(state: &Picker, key: KeyEvent) -> Option<char> {
    if state.entry.is_some() || input::is_command(key) {
        return None;
    }
    let KeyCode::Char(character) = key.code else {
        return None;
    };
    state
        .extra
        .iter()
        .any(|extra| extra.usable_char() == Some(character))
        .then_some(character)
}

/// Lifts a date flow into a [`DatePick`] flow.
fn as_pick(flow: &PopupFlow<Option<NaiveDate>>) -> PopupFlow<DatePick> {
    match flow {
        PopupFlow::Continue => PopupFlow::Continue,
        PopupFlow::Done(date) => PopupFlow::Done(DatePick::Date(*date)),
        PopupFlow::Cancelled => PopupFlow::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::super::{WeekColumn, calendar_hints, shift};
    use super::*;

    const INBOX: ExtraKey = ExtraKey {
        key: "E",
        label: "inbox",
    };

    fn start() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 15).expect("a real date")
    }

    fn picker(extra: &[ExtraKey]) -> Picker {
        Picker {
            cursor: start(),
            entry: None,
            week_column: WeekColumn::Shown,
            extra: usable_keys(extra),
        }
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn an_extra_key_closes_the_picker_with_its_character() {
        let mut state = picker(&[INBOX]);
        assert!(matches!(
            handle_picker_key(&mut state, press(KeyCode::Char('E')), true),
            PopupFlow::Done(DatePick::Extra('E'))
        ));
    }

    #[test]
    fn the_calendar_keys_still_pick_a_date() {
        let mut state = picker(&[INBOX]);
        handle_picker_key(&mut state, press(KeyCode::Char('l')), true);
        let picked = shift(start(), 1);
        assert!(matches!(
            handle_picker_key(&mut state, press(KeyCode::Enter), true),
            PopupFlow::Done(DatePick::Date(Some(date))) if date == picked
        ));
    }

    #[test]
    fn an_extra_key_types_into_the_open_entry_field() {
        let mut state = picker(&[INBOX]);
        handle_picker_key(&mut state, press(KeyCode::Char('e')), true);
        assert!(matches!(
            handle_picker_key(&mut state, press(KeyCode::Char('E')), true),
            PopupFlow::Continue
        ));
        assert!(state.entry.is_some(), "the field stays open");
    }

    #[test]
    fn a_key_the_calendar_uses_is_never_an_extra_key() {
        let taken = ExtraKey {
            key: "e",
            label: "taken",
        };
        let long = ExtraKey {
            key: "Ex",
            label: "long",
        };
        assert_eq!(usable_keys(&[taken, long, INBOX]), vec![INBOX]);
        let mut state = picker(&[taken]);
        assert!(matches!(
            handle_picker_key(&mut state, press(KeyCode::Char('e')), true),
            PopupFlow::Continue
        ));
        assert!(state.entry.is_some(), "e still opens the entry field");
    }

    #[test]
    fn the_hints_show_extra_keys_after_the_own_ones() {
        let hints = calendar_hints(true, &[INBOX]);
        assert_eq!(hints.last(), Some(&("E", "inbox")));
        assert!(!calendar_hints(true, &[]).contains(&("E", "inbox")));
    }
}
