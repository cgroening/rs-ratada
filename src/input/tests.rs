use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use super::*;
use crate::theme::{ColorOverrides, ThemeRegistry};

mod render;
mod wrapped;

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

/// `AltGr`, as crossterm reports it on a German keyboard.
fn altgr(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL | KeyModifiers::ALT)
}

fn shift(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::SHIFT)
}

/// Applies one key in the given mode and returns the new `(text, cursor)`.
fn edit(
    text: &str,
    cursor: TextCursor,
    key: KeyEvent,
    mode: EditMode,
) -> (String, TextCursor) {
    let mut text = text.to_string();
    let mut cursor = cursor;
    apply_edit_key(&mut text, &mut cursor, key, mode, None);
    (text, cursor)
}

#[test]
fn insertion_range_is_the_caret_or_the_selection() {
    let mut field = InputField::new("abcdef");
    assert_eq!(field.insertion_range(), (6, 6));
    field.handle_key(press(KeyCode::Left));
    field.handle_key(press(KeyCode::Left));
    assert_eq!(field.insertion_range(), (4, 4));
    field.handle_key(shift(KeyCode::Left));
    assert_eq!(field.insertion_range(), (3, 4));
}

#[test]
fn ctrl_u_and_ctrl_k_delete_before_and_after_the_caret() {
    let single = EditMode::SingleLine;
    let (text, cursor) = edit(
        "abcdef",
        TextCursor::at(3),
        ctrl(KeyCode::Char('u')),
        single,
    );
    assert_eq!((text.as_str(), cursor.pos), ("def", 0));
    let (text, cursor) = edit(
        "abcdef",
        TextCursor::at(3),
        ctrl(KeyCode::Char('k')),
        single,
    );
    assert_eq!((text.as_str(), cursor.pos), ("abc", 3));
    // Ctrl+U at the start is a no-op.
    let (text, cursor) =
        edit("abc", TextCursor::at(0), ctrl(KeyCode::Char('u')), single);
    assert_eq!((text.as_str(), cursor.pos), ("abc", 0));
}

#[test]
fn ctrl_u_and_ctrl_k_act_on_the_display_line_when_multiline() {
    // "alpha beta gamma" word-wraps to "alpha beta" / "gamma" at width 11.
    let mode = EditMode::Multiline {
        width: 11,
        height: 4,
    };
    let caret = TextCursor::at(13); // inside "gamma"
    let (text, cursor) =
        edit("alpha beta gamma", caret, ctrl(KeyCode::Char('k')), mode);
    assert_eq!((text.as_str(), cursor.pos), ("alpha beta ga", 13));
    let (text, cursor) =
        edit("alpha beta gamma", caret, ctrl(KeyCode::Char('u')), mode);
    assert_eq!((text.as_str(), cursor.pos), ("alpha beta mma", 11));
}

#[test]
fn multiline_home_end_and_vertical_move_on_display_lines() {
    let mode = EditMode::Multiline {
        width: 11,
        height: 4,
    };
    let text = "alpha beta gamma";
    let caret = TextCursor::at(13); // line 1, column 2
    assert_eq!(edit(text, caret, press(KeyCode::Home), mode).1.pos, 11);
    assert_eq!(edit(text, caret, press(KeyCode::End), mode).1.pos, 16);
    // Up keeps the column, landing on the first display line.
    assert_eq!(edit(text, caret, press(KeyCode::Up), mode).1.pos, 2);
    // Shift+Up extends instead of moving.
    let (_, cursor) = edit(text, caret, shift(KeyCode::Up), mode);
    assert_eq!(cursor.selection(), Some((2, 13)));
}

#[test]
fn page_keys_move_by_the_viewport_height_of_display_rows() {
    // Six one-char-wide rows via explicit newlines; height 2 is the page.
    let mode = EditMode::Multiline {
        width: 10,
        height: 2,
    };
    let text = "l0\nl1\nl2\nl3\nl4\nl5"; // rows start at 0,3,6,9,12,15
    // PageDown moves two rows down, keeping the column.
    assert_eq!(
        edit(text, TextCursor::at(1), press(KeyCode::PageDown), mode)
            .1
            .pos,
        7
    );
    // PageUp moves two rows up, keeping the column.
    assert_eq!(
        edit(text, TextCursor::at(16), press(KeyCode::PageUp), mode)
            .1
            .pos,
        10
    );
}

#[test]
fn page_keys_clamp_at_the_first_and_last_row() {
    let mode = EditMode::Multiline {
        width: 10,
        height: 4,
    };
    let text = "l0\nl1\nl2"; // rows start at 0,3,6
    // PageUp near the top lands on row 0, not a no-op.
    assert_eq!(
        edit(text, TextCursor::at(6), press(KeyCode::PageUp), mode)
            .1
            .pos,
        0
    );
    // PageDown past the end lands on the last row.
    assert_eq!(
        edit(text, TextCursor::at(0), press(KeyCode::PageDown), mode)
            .1
            .pos,
        6
    );
}

#[test]
fn shift_page_down_extends_the_selection() {
    let mode = EditMode::Multiline {
        width: 10,
        height: 2,
    };
    let text = "l0\nl1\nl2\nl3";
    let (_, cursor) =
        edit(text, TextCursor::at(0), shift(KeyCode::PageDown), mode);
    assert_eq!(cursor.selection(), Some((0, 6)));
}

#[test]
fn enter_inserts_a_newline_only_in_multiline() {
    let (text, _) = edit(
        "ab",
        TextCursor::at(2),
        press(KeyCode::Enter),
        EditMode::Multiline {
            width: 8,
            height: 4,
        },
    );
    assert_eq!(text.as_str(), "ab\n");
    // In a single line the caller owns `Enter`, so it is not consumed.
    let mut text = String::from("ab");
    let mut cursor = TextCursor::at(2);
    let consumed = apply_edit_key(
        &mut text,
        &mut cursor,
        press(KeyCode::Enter),
        EditMode::SingleLine,
        None,
    );
    assert!(!consumed);
    assert_eq!(text.as_str(), "ab");
}

#[test]
fn replace_selection_swaps_the_range_and_clears_the_anchor() {
    let mut text = String::from("abcd");
    let mut cursor = TextCursor {
        pos: 3,
        anchor: Some(1),
    };
    replace_selection(&mut text, &mut cursor, "X");
    assert_eq!((text.as_str(), cursor.pos), ("aXd", 2));
    assert_eq!(cursor.selection(), None);
}

#[test]
fn selected_text_reads_the_ordered_range() {
    let cursor = TextCursor {
        pos: 13,
        anchor: Some(6),
    };
    assert_eq!(
        selected_text("alpha beta gamma", &cursor).as_deref(),
        Some("beta ga"),
    );
    assert_eq!(selected_text("abc", &TextCursor::at(1)), None);
}

#[test]
fn is_command_requires_control_without_alt() {
    assert!(is_command(ctrl(KeyCode::Char('s'))));
    assert!(!is_command(altgr(KeyCode::Char('\\'))));
    assert!(!is_command(press(KeyCode::Char('a'))));
}

/// A plain letter, whatever its case, but nothing carrying `Control` or
/// `Alt`. The `Alt`-only case is the one that matters: every other
/// rejection here is also caught by the `Control` half, so dropping the
/// `Alt` half of the check would go unnoticed without it.
#[test]
fn is_bare_character_accepts_only_an_unmodified_letter() {
    assert!(is_bare_character(press(KeyCode::Char('y'))));
    assert!(is_bare_character(KeyEvent::new(
        KeyCode::Char('Y'),
        KeyModifiers::SHIFT,
    )));

    assert!(!is_bare_character(ctrl(KeyCode::Char('y'))));
    assert!(!is_bare_character(altgr(KeyCode::Char('@'))));
    assert!(!is_bare_character(KeyEvent::new(
        KeyCode::Char('y'),
        KeyModifiers::ALT,
    )));
    // Not a character at all.
    assert!(!is_bare_character(press(KeyCode::Enter)));
}

#[test]
fn altgr_char_is_typed_not_swallowed() {
    // German keyboards emit `\` as AltGr (Control + Alt); it must insert.
    let mut text = String::from("C:");
    let mut cursor = TextCursor::at_end(&text);
    assert!(apply_edit_key(
        &mut text,
        &mut cursor,
        altgr(KeyCode::Char('\\')),
        EditMode::SingleLine,
        None,
    ));
    assert_eq!((text.as_str(), cursor.pos), ("C:\\", 3));

    // A genuine Ctrl chord still does not type a character.
    let mut text = String::from("C:");
    let mut cursor = TextCursor::at_end(&text);
    apply_edit_key(
        &mut text,
        &mut cursor,
        ctrl(KeyCode::Char('s')),
        EditMode::SingleLine,
        None,
    );
    assert_eq!(text.as_str(), "C:");
}

fn palette() -> Palette {
    Palette::resolve(
        ThemeRegistry::builtin().resolve("default"),
        &ColorOverrides::default(),
    )
}

/// The display columns the spans occupy.
fn columns(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.width()).sum()
}

/// Whether `span` carries the block-caret background.
fn is_caret(span: &Span<'_>, palette: &Palette) -> bool {
    span.style.bg == Some(style::to_ratatui(palette.cursor))
}

#[test]
fn typing_inserts_at_the_caret() {
    let mut field = InputField::default();
    field.handle_key(press(KeyCode::Char('h')));
    field.handle_key(press(KeyCode::Char('i')));
    assert_eq!(field.value(), "hi");
    assert_eq!(field.cursor.pos, 2);
}

#[test]
fn backspace_removes_the_previous_char() {
    let mut field = InputField::new("ab");
    field.handle_key(press(KeyCode::Backspace));
    assert_eq!(field.value(), "a");
}

#[test]
fn shift_left_selects_and_typing_replaces() {
    let mut field = InputField::new("abc");
    field.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
    assert_eq!(field.cursor.selection(), Some((2, 3)));
    field.handle_key(press(KeyCode::Char('X')));
    assert_eq!(field.value(), "abX");
}

#[test]
fn max_len_blocks_typing_past_the_limit() {
    let mut field = InputField::new("ab").max_len(3);
    field.handle_key(press(KeyCode::Char('c')));
    assert_eq!(field.value(), "abc");
    // The fourth character is rejected.
    field.handle_key(press(KeyCode::Char('d')));
    assert_eq!(field.value(), "abc");
}

#[test]
fn paste_into_a_single_line_field_drops_control_and_newlines() {
    let mut text = String::new();
    let mut cursor = TextCursor::default();
    let mode = EditMode::SingleLine;
    paste_text(&mut text, &mut cursor, mode, None, "a\nb\tc");
    assert_eq!(text, "abc");
    assert_eq!(cursor.pos, 3);
}

#[test]
fn paste_into_a_multiline_field_keeps_newlines() {
    let mut text = String::new();
    let mut cursor = TextCursor::default();
    let mode = EditMode::Multiline {
        width: 10,
        height: 4,
    };
    paste_text(&mut text, &mut cursor, mode, None, "a\nb\tc");
    assert_eq!(text, "a\nbc");
}

#[test]
fn paste_caps_the_result_at_max_len() {
    let mut text = String::new();
    let mut cursor = TextCursor::default();
    let mode = EditMode::SingleLine;
    paste_text(&mut text, &mut cursor, mode, Some(3), "abcdef");
    assert_eq!(text, "abc");
}

#[test]
fn paste_replaces_the_active_selection() {
    let mut text = String::from("hello");
    let mut cursor = TextCursor {
        pos: 5,
        anchor: Some(0),
    };
    let mode = EditMode::SingleLine;
    paste_text(&mut text, &mut cursor, mode, None, "hi");
    assert_eq!((text.as_str(), cursor.pos, cursor.anchor), ("hi", 2, None));
}
