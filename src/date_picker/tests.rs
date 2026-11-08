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
fn e_opens_the_entry_field_prefilled_with_the_cursor() {
    let mut state = picker();
    handle_key(&mut state, press(KeyCode::Char('e')), true);
    let entry = state.entry.as_ref().expect("the field is open");
    assert_eq!(entry.text(), "2026-06-15");
}

/// The grid follows the typed value, so the day is confirmed by sight
/// before `Enter` ever returns it.
#[test]
fn a_typed_date_moves_the_cursor_live() {
    let mut state = picker();
    handle_key(&mut state, press(KeyCode::Char('e')), true);
    type_text(&mut state, "2026-01-02");
    assert!(state.entry.is_some());
    assert_eq!(
        state.cursor,
        NaiveDate::from_ymd_opt(2026, 1, 2).expect("a real date"),
    );
}

#[test]
fn a_typed_german_date_moves_the_cursor_live() {
    let mut state = picker();
    handle_key(&mut state, press(KeyCode::Char('e')), true);
    type_text(&mut state, "2.1.27");
    assert_eq!(
        state.cursor,
        NaiveDate::from_ymd_opt(2027, 1, 2).expect("a real date"),
    );
}

/// The live preview moves the cursor into the year being typed; a date
/// left without one must still fall in the year the field opened over.
#[test]
fn a_date_without_a_year_takes_the_year_the_field_opened_over() {
    let mut state = picker();
    handle_key(&mut state, press(KeyCode::Char('e')), true);
    type_text(&mut state, "1.2.2030");
    for _ in 0.."2030".len() {
        handle_key(&mut state, press(KeyCode::Backspace), true);
    }
    assert_eq!(
        state.cursor,
        NaiveDate::from_ymd_opt(2026, 2, 1).expect("a real date"),
    );
}

#[test]
fn an_unparseable_entry_blocks_enter() {
    for text in ["", "2026-13-40", "2026-06"] {
        let mut state = picker();
        handle_key(&mut state, press(KeyCode::Char('e')), true);
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
    handle_key(&mut state, press(KeyCode::Char('e')), true);
    type_text(&mut state, "2026-01-02");
    let expected = NaiveDate::from_ymd_opt(2026, 1, 2).expect("a real date");
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
    handle_key(&mut state, press(KeyCode::Char('e')), true);
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

    handle_key(&mut state, press(KeyCode::Char('e')), true);
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

    handle_key(&mut state, press(KeyCode::Char('e')), true);
    handle_paste(&mut state, "2026-01-02");
    assert_eq!(
        state.cursor,
        NaiveDate::from_ymd_opt(2026, 1, 2).expect("a real date"),
    );
}

#[test]
fn delete_clears_only_in_calendar_mode() {
    let mut state = picker();
    handle_key(&mut state, press(KeyCode::Char('e')), true);
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

/// The cursor is filled with the accent and its text contrasts with it;
/// today is plain accent text, so the two never look alike.
#[test]
fn the_cursor_cell_is_filled_and_today_is_plain_accent() {
    let skin = skin();
    let palette = &skin.palette;
    let cursor = cursor_cell_style(palette);
    assert_eq!(cursor.bg, Some(style::to_ratatui(palette.accent)));
    assert_ne!(cursor.fg, cursor.bg);
    assert!(cursor.add_modifier.contains(Modifier::BOLD));

    let today = today_cell_style(palette);
    assert_eq!(today, style::fg(palette.accent));
    assert!(!today.add_modifier.contains(Modifier::BOLD));
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
    assert_eq!(shift(day, 7), NaiveDate::from_ymd_opt(2026, 6, 22).unwrap());
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
