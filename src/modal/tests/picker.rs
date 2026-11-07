//! The list pickers: navigation, keys below the rows, hint width.

use super::*;

#[test]
fn navigate_list_wraps_on_arrows_and_clamps_on_page_and_ends() {
    let mut cursor = 0;
    // Up wraps to the last item.
    assert!(navigate_list(&mut cursor, key(KeyCode::Up), 5, 2));
    assert_eq!(cursor, 4);
    // PageUp clamps at the top rather than wrapping.
    assert!(navigate_list(&mut cursor, key(KeyCode::PageUp), 5, 2));
    assert_eq!(cursor, 2);
    assert!(navigate_list(&mut cursor, key(KeyCode::PageUp), 5, 2));
    assert_eq!(cursor, 0);
    // End and Home jump to the last and first item.
    assert!(navigate_list(&mut cursor, key(KeyCode::End), 5, 2));
    assert_eq!(cursor, 4);
    assert!(navigate_list(&mut cursor, key(KeyCode::Home), 5, 2));
    assert_eq!(cursor, 0);
    // A non-navigation key is left for the caller.
    assert!(!navigate_list(&mut cursor, key(KeyCode::Enter), 5, 2));
}

/// `Ctrl+J`/`Ctrl+K` are commands, not motions: in raw mode crossterm
/// reports Ctrl+J as `Char('j') + CONTROL`, so without the guard a chord
/// would move the cursor. The key must be left for the caller.
#[test]
fn navigate_list_leaves_ctrl_chords_to_the_caller() {
    let mut cursor = 2;
    for code in [
        KeyCode::Char('j'),
        KeyCode::Char('k'),
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
    ] {
        assert!(
            !navigate_list(&mut cursor, ctrl(code), 5, 2),
            "Ctrl+{code:?} must not be consumed as navigation"
        );
        assert_eq!(cursor, 2, "Ctrl+{code:?} must not move the cursor");
    }
}

/// The keys sit below the last row, not over it.
#[test]
fn a_list_picker_shows_its_keys_below_the_rows() {
    let items = vec!["alpha".to_string(), "beta".to_string()];
    let hints = [("space", "tick"), ("enter", "confirm"), ("esc", "cancel")];
    let rows = rendered_picker(&items, &hints);
    let row_of = |text: &str| {
        rows.iter().position(|row| row.contains(text)).expect(text)
    };
    let hint_row = row_of("space tick");
    assert!(rows[hint_row].contains("enter confirm"));
    assert!(rows[hint_row].contains("esc cancel"));
    assert!(hint_row > row_of("beta"));
}

#[test]
fn a_picker_is_wide_enough_for_its_hints() {
    let area = Rect::new(0, 0, 60, 20);
    let hints = [("space", "tick"), ("enter", "confirm"), ("esc", "cancel")];
    let picker = picker_area(area, 2, 10, &hints);
    let hint_width = crate::modal::render::hints_width(&hints);
    assert!(usize::from(picker.width) >= hint_width + 2);
}
