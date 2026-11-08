//! `Wrapped` is the mode for a value that is one logical line but too long
//! to show on one row. Everything below is the contract that separates it
//! from the two modes it sits between.

use super::*;

/// Wraps at 6 columns, 3 rows tall.
const MODE: EditMode = EditMode::Wrapped {
    width: 6,
    height: 3,
};

fn press(text: &mut String, cursor: &mut TextCursor, code: KeyCode) -> bool {
    apply_edit_key(text, cursor, KeyEvent::from(code), MODE, None)
}

/// The whole point of the mode: `Enter` belongs to the caller, which
/// uses it to submit. Consuming it here would insert a newline into a
/// value that must never hold one.
#[test]
fn enter_is_left_to_the_caller() {
    let mut text = "ab".to_string();
    let mut cursor = TextCursor::at(2);

    assert!(!press(&mut text, &mut cursor, KeyCode::Enter));
    assert_eq!(text, "ab", "the buffer must be untouched");
}

/// A multi-line paste collapses, as it does in a single-line field.
#[test]
fn a_paste_drops_its_line_breaks() {
    let mut text = String::new();
    let mut cursor = TextCursor::at(0);

    paste_text(&mut text, &mut cursor, MODE, None, "one\ntwo");
    assert!(!text.contains('\n'), "got: {text:?}");
    assert_eq!(text, "onetwo");
}

/// Navigation follows the display, not the logical line: that is what
/// it borrows from `Multiline`.
#[test]
fn home_and_end_act_on_the_display_line() {
    // Wrapped at 6 columns: "aaa" / "bbb".
    let text = "aaa bbb".to_string();
    let mut cursor = TextCursor::at(5);
    let mut buffer = text.clone();

    press(&mut buffer, &mut cursor, KeyCode::Home);
    assert_eq!(cursor.pos, 4, "the second display line starts at 4");

    press(&mut buffer, &mut cursor, KeyCode::End);
    assert_eq!(cursor.pos, 7, "and ends at the value's end");
}

#[test]
fn up_and_down_walk_the_wrapped_rows() {
    let mut text = "aaa bbb".to_string();
    let mut cursor = TextCursor::at(5);

    assert!(press(&mut text, &mut cursor, KeyCode::Up));
    assert!(
        cursor.pos < 4,
        "moved onto the first row, got {}",
        cursor.pos
    );

    assert!(press(&mut text, &mut cursor, KeyCode::Down));
    assert!(cursor.pos >= 4, "back onto the second row");
}

/// Typing, deleting and selecting are unchanged - the mode only alters
/// the geometry and the newline rule.
#[test]
fn ordinary_editing_still_works() {
    let mut text = String::new();
    let mut cursor = TextCursor::at(0);

    for ch in "abc".chars() {
        press(&mut text, &mut cursor, KeyCode::Char(ch));
    }
    assert_eq!(text, "abc");

    press(&mut text, &mut cursor, KeyCode::Backspace);
    assert_eq!(text, "ab");
}
