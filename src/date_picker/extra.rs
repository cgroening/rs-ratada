//! The date picker with keys and a note of the caller's own.
//!
//! An [`ExtraKey`] is a bare letter the calendar does not use itself, shown in
//! the hint footer after the picker's own keys. Pressing it closes the picker
//! with [`DatePick::Extra`], so a caller can offer a choice that is no date
//! ("E inbox") without a second dialog. While the typed entry is open, every
//! letter types into it.
//!
//! A note ([`DatePickerOptions::note`]) stands above the calendar: lines the
//! caller builds for a width and styles, for what the frame title cannot hold.
//! The box widens to the widest note line, up to [`NOTE_MAX_WIDTH`], and
//! centres the calendar below it.

use std::{fmt, io};

use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

use super::{
    MODAL_CHROME_HEIGHT, ModalSignal, OPEN_KEY, Picker, PopupFlow, Tui,
    calendar_width, centered_rect, handle_key, handle_paste, input, overlay,
    picker_lines, popup_with_paste, today, week_column,
};
use crate::{chrome::MODAL_CHROME_WIDTH, theme::Skin};

/// The widest a note makes the picker's content, in columns; a longer line
/// is built for this width (the caller wraps it).
pub const NOTE_MAX_WIDTH: u16 = 44;

/// Builds the note above the calendar for the given width, in columns, like
/// the body of a styled dialog: it is called on every draw, so a note that
/// wraps to the width follows a resize.
pub type NoteFn<'a> = &'a dyn Fn(u16) -> Vec<Line<'static>>;

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
#[derive(Clone, Copy)]
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
    /// Lines above the calendar that say what the date is for (the task a
    /// date is asked for, what is known about it), or `None`. A blank line
    /// separates them from the calendar.
    pub note: Option<NoteFn<'a>>,
}

impl fmt::Debug for DatePickerOptions<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatePickerOptions")
            .field("prompt", &self.prompt)
            .field("current", &self.current)
            .field("allow_clear", &self.allow_clear)
            .field("extra", &self.extra)
            .field("note", &self.note.map(|_| "<fn>"))
            .finish()
    }
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
        |area, state: &Picker| picker_area(skin, options, state, area),
        |frame, _| render_bg(frame),
        |frame, rect, state: &Picker| {
            render_picker(frame, skin, options, state, rect);
        },
        |state, key| handle_picker_key(state, key, allow_clear),
        |state, text| as_pick(&handle_paste(state, &text)),
    )
}

/// The picker's box in `area`: as wide as its content plus the chrome and
/// as tall as its lines.
fn picker_area(
    skin: &Skin,
    options: &DatePickerOptions<'_>,
    state: &Picker,
    area: Rect,
) -> Rect {
    let width = content_width(options, state, area);
    let lines = picker_lines(
        skin,
        state,
        options.allow_clear,
        note_lines(options, width),
        usize::from(width),
    );
    let rows = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(MODAL_CHROME_HEIGHT);
    centered_rect(width.saturating_add(MODAL_CHROME_WIDTH), rows, area)
}

/// Draws the picker in its box `rect`: the frame with the prompt as title,
/// then the note, the calendar and the hints.
fn render_picker(
    frame: &mut Frame,
    skin: &Skin,
    options: &DatePickerOptions<'_>,
    state: &Picker,
    rect: Rect,
) {
    let inner = overlay::framed(frame, rect, skin, options.prompt);
    let lines = picker_lines(
        skin,
        state,
        options.allow_clear,
        note_lines(options, inner.width),
        usize::from(inner.width),
    );
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The columns of the box's content: the calendar's, or the widest line of
/// the note when that is wider, at most [`NOTE_MAX_WIDTH`] and what `area`
/// leaves inside the chrome.
fn content_width(
    options: &DatePickerOptions<'_>,
    state: &Picker,
    area: Rect,
) -> u16 {
    let calendar = u16::try_from(calendar_width(state)).unwrap_or(u16::MAX);
    let room = area
        .width
        .saturating_sub(MODAL_CHROME_WIDTH)
        .min(NOTE_MAX_WIDTH)
        .max(calendar);
    let widest = note_lines(options, room)
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0);
    u16::try_from(widest)
        .unwrap_or(u16::MAX)
        .clamp(calendar, room)
}

/// The note built for `width` columns; none without one.
fn note_lines(
    options: &DatePickerOptions<'_>,
    width: u16,
) -> Vec<Line<'static>> {
    options.note.map(|note| note(width)).unwrap_or_default()
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
    use std::cell::Cell;

    use crossterm::event::KeyModifiers;
    use ratatui::{Terminal, backend::TestBackend};

    use super::super::{WeekColumn, body_lines, calendar_hints, shift};
    use super::*;
    use crate::theme::{
        ColorOverrides, GlyphVariant, Glyphs, Palette, ThemeRegistry,
    };

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

    fn skin() -> Skin {
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
        )
    }

    fn options(note: Option<NoteFn<'_>>) -> DatePickerOptions<'_> {
        DatePickerOptions {
            prompt: "Recreate on",
            current: Some(start()),
            allow_clear: false,
            extra: &[],
            note,
        }
    }

    /// A task, its rule and the dates its copy stands on already.
    fn task_note(_width: u16) -> Vec<Line<'static>> {
        vec![
            Line::from("Keep the household book ~montags/freitags"),
            Line::from("Fri 06.11.2026 already there"),
        ]
    }

    /// The text of every screen row after drawing the picker with
    /// `options` on a `width` x `height` screen.
    fn rendered(
        options: &DatePickerOptions<'_>,
        width: u16,
        height: u16,
    ) -> Vec<String> {
        let state = picker(&[]);
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("a test terminal");
        terminal
            .draw(|frame| {
                let rect = picker_area(&skin(), options, &state, frame.area());
                render_picker(frame, &skin(), options, &state, rect);
            })
            .expect("the frame draws");
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    /// The first screen row holding `text`.
    fn row_of(rows: &[String], text: &str) -> usize {
        rows.iter()
            .position(|row| row.contains(text))
            .unwrap_or_else(|| panic!("no row shows {text:?}:\n{rows:#?}"))
    }

    #[test]
    fn the_note_stands_above_the_calendar_after_a_blank_line() {
        let rows = rendered(&options(Some(&task_note)), 80, 30);
        let title = row_of(&rows, "Recreate on");
        let note = row_of(&rows, "Keep the household book ~montags/freitags");
        assert_eq!(note, title + 2, "the padding row follows the title");
        assert_eq!(row_of(&rows, "Fri 06.11.2026 already there"), note + 1);
        assert_eq!(row_of(&rows, "2026-06-15"), note + 3);
    }

    #[test]
    fn a_wide_note_widens_the_box_and_centres_the_calendar() {
        let state = picker(&[]);
        let area = Rect::new(0, 0, 80, 30);
        let note: NoteFn<'_> = &task_note;
        let width = content_width(&options(Some(note)), &state, area);
        let widest = task_note(width)[0].width();
        assert_eq!(usize::from(width), widest);
        let lines = picker_lines(
            &skin(),
            &state,
            false,
            task_note(width),
            usize::from(width),
        );
        let pad = (widest - calendar_width(&state)) / 2;
        let weekdays: String = lines[4]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            weekdays.starts_with(&format!("{} Wk", " ".repeat(pad))),
            "{weekdays:?} is not centred by {pad}"
        );
    }

    #[test]
    fn a_note_is_built_for_the_room_the_limit_and_the_screen_leave() {
        let state = picker(&[]);
        let asked = Cell::new(0);
        let long = |width: u16| {
            asked.set(width);
            vec![Line::from("x".repeat(80))]
        };
        let options = options(Some(&long));
        let wide = Rect::new(0, 0, 120, 30);
        assert_eq!(content_width(&options, &state, wide), NOTE_MAX_WIDTH);
        assert_eq!(asked.get(), NOTE_MAX_WIDTH);
        let calendar = u16::try_from(calendar_width(&state)).expect("small");
        let narrow = Rect::new(0, 0, 20, 30);
        assert_eq!(content_width(&options, &state, narrow), calendar);
    }

    #[test]
    fn without_a_note_the_picker_is_the_calendar_as_before() {
        let state = picker(&[]);
        let area = Rect::new(0, 0, 80, 30);
        let width = content_width(&options(None), &state, area);
        assert_eq!(usize::from(width), calendar_width(&state));
        let lines = picker_lines(
            &skin(),
            &state,
            false,
            Vec::new(),
            calendar_width(&state),
        );
        assert_eq!(lines, body_lines(&skin(), &state, false));
        let rows = rendered(&options(None), 80, 30);
        assert_eq!(
            row_of(&rows, "Recreate on") + 2,
            row_of(&rows, "2026-06-15")
        );
    }
}
