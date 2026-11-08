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
        assert!(state.range.start.is_none(), "Ctrl+{code:?} fixed the start",);
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
    for text in ["", "2026-06-20", "2026-06-20..", "2026-06-20..2026-13-01"] {
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
