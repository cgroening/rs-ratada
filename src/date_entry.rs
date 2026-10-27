//! The typed-entry field shared by the three calendar pickers.
//!
//! Each of them draws a centred header - an ISO date, a `YYYY-MM`, a range -
//! and [`OPEN_KEY`] turns exactly that line into a text field prefilled with
//! the current value. Only the *format* differs between the three, so this
//! module stays value-agnostic: it owns the field, the snapshot a cancelling
//! `Esc` restores, the key routing and the look of the line, while each picker
//! keeps its own parse and decides what a parsed value does to its calendar.
//! Same division as `filter_list`, which owns a query buffer without knowing
//! what a row is.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use super::{
    input::{self, InputField, ScrollPaint},
    shortcut_hints, style,
};
use crate::theme::Palette;

/// The key that turns a picker's header into the entry field.
pub(crate) const OPEN_KEY: char = 'e';

/// The hint token advertising [`OPEN_KEY`], so all three pickers word it alike.
pub(crate) const EDIT_HINT: (&str, &str) = ("e", "edit");

/// The hints shown while the entry field holds the keyboard.
pub(crate) const ENTRY_HINTS: [(&str, &str); 2] =
    [("enter", "ok"), ("esc", "cancel")];

/// Whether the typed text currently parses.
///
/// Drives the field's color and, at the picker, whether `Enter` may return:
/// text that does not parse is shown in the error color and cannot be
/// confirmed, so a picker never yields a value the user did not see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Validity {
    /// The text parses into a value.
    Valid,
    /// The text does not parse; an empty field included.
    Invalid,
}

impl Validity {
    /// The validity of a parse attempt, where `parses` is whether it yielded a
    /// value.
    pub(crate) fn of(parses: bool) -> Self {
        if parses { Self::Valid } else { Self::Invalid }
    }
}

/// What one key inside the field did, for the picker to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryAction {
    /// The key went to the field; the picker re-parses and previews.
    Edited,
    /// `Enter`: return the typed value, if it parses.
    Committed,
    /// `Esc`: close the field and restore the pre-entry state.
    Aborted,
}

/// The typed-entry state: the field plus the calendar state to return to when
/// the user leaves entry mode with `Esc`.
///
/// `S` is that snapshot - a day, a year and month, a range - and is only stored
/// and handed back, so it carries no behaviour. It lives here rather than
/// beside the picker's `Option<DateEntry<_>>` so the two cannot desync: there
/// is no snapshot without an open field, and no open field without a snapshot.
pub(crate) struct DateEntry<S> {
    field: InputField,
    /// The value's length limit, mirroring the field's own private one.
    max_len: usize,
    snapshot: S,
}

impl<S: Copy> DateEntry<S> {
    /// Opens the field over `initial`, capped at `max_len` characters and fully
    /// selected, so the first typed character replaces the value instead of
    /// appending to it. `snapshot` is the state a cancelling `Esc` puts back.
    pub(crate) fn open(initial: &str, max_len: usize, snapshot: S) -> Self {
        let mut field = InputField::new(initial).max_len(max_len);
        field.cursor.select_all(initial.chars().count());
        Self {
            field,
            max_len,
            snapshot,
        }
    }

    /// Applies one key inside the field.
    ///
    /// `Enter` and `Esc` belong to the picker and are only reported back; every
    /// other key goes to [`InputField::handle_key`], which is what makes
    /// `Ctrl+A`/`U`/`K`/`C`/`X`/`V`, `Shift`+motion and `Home`/`End` work. A
    /// picker's `input::is_command` guard must therefore sit on its calendar
    /// branch, never in front of this call. A key the field does not consume
    /// leaves the text as it was, which the picker's re-parse simply confirms.
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> EntryAction {
        match key.code {
            KeyCode::Enter => EntryAction::Committed,
            KeyCode::Esc => EntryAction::Aborted,
            _ => {
                self.field.handle_key(key);
                EntryAction::Edited
            }
        }
    }

    /// Inserts a bracketed paste at the caret, replacing any selection.
    pub(crate) fn paste(&mut self, text: &str) {
        self.field.paste(text);
    }

    /// The text typed so far, for the picker to parse.
    pub(crate) fn text(&self) -> &str {
        self.field.value()
    }

    /// The calendar state the field opened over, which a cancelling `Esc`
    /// restores.
    pub(crate) fn snapshot(&self) -> S {
        self.snapshot
    }

    /// The field, drawn in place of the picker's centred header.
    ///
    /// The value keeps the left padding the header had, so it does not jump
    /// when the mode changes, and sits on the active input background in the
    /// accent color while it parses and in the error color while it does not.
    /// The field is one column wider than its value: the block caret, which
    /// [`Self::open`] leaves past the last character, needs a cell of its own.
    pub(crate) fn header_line(
        &self,
        palette: &Palette,
        inner_width: usize,
        validity: Validity,
    ) -> Line<'static> {
        let width = (self.max_len + 1).min(inner_width);
        let pad = inner_width.saturating_sub(self.max_len) / 2;
        let base = field_style(palette, validity);
        let paint = ScrollPaint {
            cursor: self.field.cursor,
            width,
            base,
            content: None,
        };
        let mut spans = vec![Span::styled(" ".repeat(pad), Style::default())];
        spans.extend(input::scrolled_line_spans(self.text(), paint, palette));
        // `scrolled_line_spans` stops after the value; the fill makes the field
        // read as one solid strip of its full width.
        let used: usize = spans.iter().map(|span| span.content.width()).sum();
        if used < pad + width {
            spans.push(Span::styled(" ".repeat(pad + width - used), base));
        }
        Line::from(spans)
    }
}

/// The hint footer of a calendar picker, at a height that does not change with
/// the mode.
///
/// `shown` is laid out and padded with blank rows to the height `other` would
/// need, so opening or leaving entry mode never resizes the box under the
/// user's hands - the two hint sets rarely wrap to the same number of rows.
pub(crate) fn hint_lines(
    shown: &[(&str, &str)],
    other: &[(&str, &str)],
    palette: &Palette,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = shortcut_hints::lines(shown, palette.accent_dim, width);
    let height = shortcut_hints::lines(other, palette.accent_dim, width).len();
    while lines.len() < height {
        lines.push(Line::from(""));
    }
    lines
}

/// The field's fill and text color: the header's accent while the value parses,
/// the palette's error color while it does not.
fn field_style(palette: &Palette, validity: Validity) -> Style {
    let text = match validity {
        Validity::Valid => style::accent(palette),
        Validity::Invalid => style::error(palette),
    };
    text.bg(style::to_ratatui(palette.input_bg_active))
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;
    use crate::theme::{ColorOverrides, ThemeRegistry};

    const ISO: &str = "2026-06-15";
    const MAX_LEN: usize = 10;
    const INNER_WIDTH: usize = 22;

    fn palette() -> Palette {
        let base = ThemeRegistry::builtin().resolve("default");
        Palette::resolve(base, &ColorOverrides::default())
    }

    fn entry() -> DateEntry<()> {
        DateEntry::open(ISO, MAX_LEN, ())
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    /// The prefill is selected rather than appended to, so the common case -
    /// typing a whole new date - needs no clearing first.
    #[test]
    fn open_selects_the_whole_prefill() {
        let entry = entry();
        assert_eq!(entry.text(), ISO);
        assert_eq!(entry.field.cursor.selection(), Some((0, ISO.len())));
    }

    #[test]
    fn typing_replaces_the_selected_prefill() {
        let mut entry = entry();
        assert_eq!(
            entry.handle_key(press(KeyCode::Char('2'))),
            EntryAction::Edited
        );
        assert_eq!(entry.text(), "2");
    }

    #[test]
    fn enter_and_esc_are_reported_to_the_picker() {
        let mut entry = entry();
        assert_eq!(
            entry.handle_key(press(KeyCode::Enter)),
            EntryAction::Committed
        );
        assert_eq!(entry.handle_key(press(KeyCode::Esc)), EntryAction::Aborted);
        // Neither key touches the value.
        assert_eq!(entry.text(), ISO);
    }

    /// The field is where a `Ctrl` chord must land: the pickers' guard against
    /// `Ctrl+H` walking their cursor may not reach this far.
    #[test]
    fn a_ctrl_chord_reaches_the_field() {
        let mut entry = entry();
        entry.handle_key(press(KeyCode::End));
        entry.handle_key(ctrl(KeyCode::Char('u')));
        assert_eq!(entry.text(), "");
    }

    #[test]
    fn a_paste_replaces_the_selection() {
        let mut entry = entry();
        entry.paste("2026-01-02");
        assert_eq!(entry.text(), "2026-01-02");
    }

    #[test]
    fn max_len_caps_the_typed_value() {
        let mut entry = entry();
        entry.handle_key(press(KeyCode::End));
        for _ in 0..5 {
            entry.handle_key(press(KeyCode::Char('9')));
        }
        assert_eq!(entry.text().chars().count(), MAX_LEN);
    }

    /// The line is drawn into a fixed-width box, so an overflow would push the
    /// modal's border off the grid.
    #[test]
    fn the_field_line_never_exceeds_the_inner_width() {
        let palette = palette();
        for inner_width in [MAX_LEN, INNER_WIDTH] {
            let line =
                entry().header_line(&palette, inner_width, Validity::Valid);
            let used: usize =
                line.spans.iter().map(|span| span.content.width()).sum();
            assert!(used <= inner_width, "{used} columns in {inner_width}");
        }
    }

    #[test]
    fn an_invalid_value_is_drawn_in_the_error_color() {
        let palette = palette();
        let error = style::to_ratatui(palette.error);
        let accent = style::to_ratatui(palette.accent);
        let line =
            entry().header_line(&palette, INNER_WIDTH, Validity::Invalid);
        assert!(line.spans.iter().any(|span| span.style.fg == Some(error)));
        let line = entry().header_line(&palette, INNER_WIDTH, Validity::Valid);
        assert!(line.spans.iter().any(|span| span.style.fg == Some(accent)));
    }

    /// Both modes claim the same rows, so the box does not resize under the
    /// user's hands when the field opens or closes.
    #[test]
    fn the_hint_footer_is_the_same_height_in_both_modes() {
        let palette = palette();
        let calendar =
            [("\u{2190}\u{2192}", "move"), EDIT_HINT, ("enter", "pick")];
        let shown = hint_lines(&calendar, &ENTRY_HINTS, &palette, INNER_WIDTH);
        let other = hint_lines(&ENTRY_HINTS, &calendar, &palette, INNER_WIDTH);
        assert_eq!(shown.len(), other.len());
    }
}
