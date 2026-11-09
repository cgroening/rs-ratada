use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::*;
use crate::form::field::FieldStyles;
use crate::theme::{
    ColorOverrides, GlyphVariant, Glyphs, Palette, Skin, ThemeRegistry,
};

fn skin() -> Skin {
    let base = ThemeRegistry::builtin().resolve("default");
    Skin::new(
        Palette::resolve(base, &ColorOverrides::default()),
        Glyphs::new(GlyphVariant::Unicode),
    )
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn press(form: &mut FormState, codes: &[KeyCode]) -> FormEvent {
    let mut last = FormEvent::Pending;
    for code in codes {
        last = form.handle_key(key(*code));
    }
    last
}

fn form() -> FormState {
    FormState::new(
        "Entry",
        vec![
            FormField::text("title", "Title", "hop"),
            FormField::section("Details"),
            FormField::picker("due", "Due", "").clearable(),
            FormField::stepper(
                "kind",
                "Kind",
                vec!["git".into(), "path".into()],
                "git",
            ),
            FormField::toggle("fav", "Favourite", false),
        ],
    )
}

/// Draws `form` as a modal on a `width`x`height` screen; rows trimmed.
fn drawn(form: &FormState, width: u16, height: u16) -> Vec<String> {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| form.render(frame, frame.area(), &skin()))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            let row: String =
                (0..width).map(|x| buffer[(x, y)].symbol()).collect();
            row.trim_end().to_string()
        })
        .collect()
}

#[test]
fn letters_are_commands_until_enter_starts_typing() {
    let mut form = form();
    press(&mut form, &[KeyCode::Char('x')]);
    assert_eq!(form.value("title"), "hop");
    let event = press(
        &mut form,
        &[KeyCode::Enter, KeyCode::Char('x'), KeyCode::Enter],
    );
    assert_eq!(event, FormEvent::Changed("title".into()));
    assert_eq!(form.value("title"), "hopx");
    assert!(!form.is_editing());
}

#[test]
fn the_focus_skips_section_bars_and_wraps() {
    let mut form = form();
    press(&mut form, &[KeyCode::Down]);
    assert_eq!(form.focus_key(), "due");
    press(&mut form, &[KeyCode::Up, KeyCode::Up]);
    assert_eq!(form.focus_key(), "fav");
}

#[test]
fn a_picker_asks_the_host_and_del_clears_it() {
    let mut form = form();
    form.focus_on("due");
    assert_eq!(
        press(&mut form, &[KeyCode::Enter]),
        FormEvent::Activate("due".into())
    );
    form.set_value("due", "2026-11-07");
    assert_eq!(
        press(&mut form, &[KeyCode::Delete]),
        FormEvent::Changed("due".into())
    );
    assert_eq!(form.value("due"), "");
}

#[test]
fn stepper_and_toggle_change_in_place() {
    let mut form = form();
    form.focus_on("kind");
    press(&mut form, &[KeyCode::Right]);
    assert_eq!(form.value("kind"), "path");
    form.focus_on("fav");
    press(&mut form, &[KeyCode::Char(' ')]);
    assert!(is_yes(form.value("fav")));
}

/// A value outside the options (set by the host) steps to the ends.
#[test]
fn a_stepper_off_its_options_steps_to_the_first_or_last() {
    let mut forward = form();
    forward.set_value("kind", "custom");
    forward.focus_on("kind");
    press(&mut forward, &[KeyCode::Right]);
    assert_eq!(forward.value("kind"), "git");

    let mut back = form();
    back.set_value("kind", "custom");
    back.focus_on("kind");
    press(&mut back, &[KeyCode::Left]);
    assert_eq!(back.value("kind"), "path");
}

#[test]
fn a_save_stays_open_and_a_second_press_closes() {
    let mut form = form();
    assert_eq!(press(&mut form, &[KeyCode::Char('s')]), FormEvent::Save);
    form.mark_saved();
    assert_eq!(press(&mut form, &[KeyCode::Char('s')]), FormEvent::Close);
}

#[test]
fn esc_closes_an_unchanged_form_and_asks_for_a_changed_one() {
    let mut unchanged = form();
    assert_eq!(press(&mut unchanged, &[KeyCode::Esc]), FormEvent::Discard);

    let mut back = form();
    back.focus_on("fav");
    press(&mut back, &[KeyCode::Char(' '), KeyCode::Esc]);
    assert_eq!(press(&mut back, &[KeyCode::Char('b')]), FormEvent::Pending);
    press(&mut back, &[KeyCode::Esc]);
    assert_eq!(press(&mut back, &[KeyCode::Char('y')]), FormEvent::Save);
    assert!(back.closes_after_save());

    let mut discard = form();
    discard.focus_on("fav");
    press(&mut discard, &[KeyCode::Char(' '), KeyCode::Esc]);
    assert_eq!(
        press(&mut discard, &[KeyCode::Char('n')]),
        FormEvent::Discard
    );
}

#[test]
fn ctrl_o_puts_the_field_back_to_its_baseline() {
    let mut form = form();
    press(&mut form, &[KeyCode::Enter, KeyCode::Char('z')]);
    form.handle_key(ctrl('o'));
    assert_eq!(form.live_value("title"), "hop");
    press(&mut form, &[KeyCode::Esc]);
    assert!(!form.is_dirty());
}

#[test]
fn a_mixed_toggle_turns_on_first_and_counts_as_changed() {
    let mut form = FormState::new(
        "Bulk",
        vec![FormField::toggle("fav", "Favourite", false).mixed()],
    );
    assert!(!form.is_dirty());
    press(&mut form, &[KeyCode::Char(' ')]);
    assert!(is_yes(form.value("fav")));
    assert!(!form.fields()[0].is_mixed);
    assert!(form.is_dirty());
}

#[test]
fn a_mixed_picker_set_to_its_hidden_value_counts_as_changed() {
    let mut form = FormState::new(
        "Bulk",
        vec![FormField::picker("section", "Section", "Ungrouped").mixed()],
    );
    form.set_value("section", "Ungrouped");
    assert!(form.is_changed(0));
    form.handle_key(ctrl('o'));
    assert!(form.fields()[0].is_mixed, "ctrl+o makes it mixed again");
    assert!(!form.is_dirty());
}

#[test]
fn a_paste_starts_typing_into_a_text_field_and_skips_a_toggle() {
    let mut form = form();
    form.paste("!");
    assert!(form.is_editing());
    assert_eq!(form.live_value("title"), "hop!");
    let mut form = self::form();
    form.focus_on("fav");
    form.paste("x");
    assert!(!form.is_editing());
    assert_eq!(form.value("fav"), NO);
}

#[test]
fn a_locked_field_does_not_change() {
    let mut form = form();
    form.set_lock("title", "read-only");
    assert_eq!(press(&mut form, &[KeyCode::Enter]), FormEvent::Pending);
    assert!(!form.is_editing());
}

#[test]
fn a_normalized_value_is_unchanged_when_it_means_the_same() {
    let mut form = FormState::new(
        "Hours",
        vec![
            FormField::text("h", "Hours", "1,5")
                .normalized(|v| v.replace(',', ".")),
        ],
    );
    form.set_value("h", "1.5");
    assert!(!form.is_dirty());
}

/// The whole modal: frame and title, the badge, the label column, the
/// section bar, the picker field with its placeholder and pointer, the
/// stepper, the checkbox, the side information and the two hint rows.
#[test]
fn the_form_renders_as_a_whole() {
    let mut form = form();
    form.set_side_info("title", "3/40", false);
    let text = drawn(&form, 80, 16).join("\n");
    let expected = [
        "Entry",
        "  Title      hop",
        "3/40",
        "Details",
        "  Due        enter to choose",
        "\u{203a}",
        "  Kind       \u{2190} git \u{2192}",
        "  Favourite  [ ]",
        "Enter edit",
        "Tab next \u{b7} s save \u{b7} y copy \u{b7} Ctrl+O reset \u{b7} Esc leave (asks to save)",
        "1/4",
    ];
    for part in expected {
        assert!(text.contains(part), "missing {part:?} in\n{text}");
    }
}

#[test]
fn a_problem_shows_under_its_field_and_takes_the_focus() {
    let mut form = form();
    form.set_error("due", "Too late.");
    assert_eq!(form.focus_key(), "due");
    let rows = drawn(&form, 80, 16);
    let due = rows.iter().position(|r| r.contains("Due")).unwrap();
    assert!(rows[due + 1].contains("Too late."), "{rows:#?}");
}

#[test]
fn the_hints_are_complete_at_the_narrowest_box() {
    let form = form();
    let width = u16::try_from(form.content_width()).unwrap()
        + crate::chrome::MODAL_CHROME_WIDTH;
    let text = drawn(&form, width, 16).join("\n");
    assert!(text.contains("Esc leave (asks to save)"), "{text}");
}

#[test]
fn the_box_keeps_its_height_while_the_focus_moves() {
    let mut form = form();
    let height = |form: &FormState| {
        drawn(form, 80, 20)
            .iter()
            .filter(|r| r.contains('\u{2502}'))
            .count()
    };
    let first = height(&form);
    form.focus_on("kind");
    assert_eq!(height(&form), first);
}

#[test]
fn a_multiline_field_types_lines_and_keeps_them() {
    let mut form =
        FormState::new("Note", vec![FormField::multiline("body", "Body", "a")]);
    press(
        &mut form,
        &[
            KeyCode::Enter,
            KeyCode::End,
            KeyCode::Enter,
            KeyCode::Char('b'),
        ],
    );
    assert_eq!(
        press(&mut form, &[KeyCode::Esc]),
        FormEvent::Changed("body".into())
    );
    assert_eq!(form.value("body"), "a\nb");
    assert_eq!(
        form.handle_key(ctrl('g')),
        FormEvent::ExternalEditor("body".into())
    );
}

fn note_form(body: &str) -> FormState {
    FormState::new(
        "Note",
        vec![
            FormField::text("title", "Title", "hop"),
            FormField::multiline("body", "Comment", body).max_chars(60),
            FormField::text("after", "Amount", "1"),
        ],
    )
}

#[test]
fn a_multi_line_field_stands_beside_its_label_and_grows() {
    let short = drawn(&note_form("Weekly groceries"), 80, 16);
    let row = short.iter().position(|r| r.contains("Comment")).unwrap();
    assert!(short[row].contains("Weekly groceries"), "{short:#?}");
    assert!(short[row].contains("16/60"), "{short:#?}");
    assert!(short[row + 1].contains("Amount"), "one row: {short:#?}");

    let long = "Weekly groceries at the farmers market and the bakery";
    let rows = drawn(&note_form(long), 80, 16);
    let row = rows.iter().position(|r| r.contains("Comment")).unwrap();
    assert!(rows[row].contains("53/60"), "{rows:#?}");
    assert!(rows[row + 1].contains("bakery"), "{rows:#?}");
    assert!(rows[row + 2].contains("Amount"), "two rows: {rows:#?}");
}

#[test]
fn the_counter_counts_while_typing_and_the_cap_holds() {
    let mut form = note_form(&"x".repeat(59));
    form.focus_on("body");
    press(
        &mut form,
        &[
            KeyCode::Enter,
            KeyCode::Char('a'),
            KeyCode::Char('b'),
            KeyCode::Char('c'),
        ],
    );
    assert_eq!(form.live_value("body").chars().count(), 60);
    let text = drawn(&form, 80, 16).join("\n");
    assert!(text.contains("60/60"), "{text}");
}

#[test]
fn the_hints_wrap_on_a_narrow_box_and_keep_every_key() {
    let form = form();
    assert_eq!(hint_rows(80), 3);
    assert!(hint_rows(30) > 3);
    let text = drawn(&form, 34, 24).join("\n");
    for key in ["Tab", "s save", "y copy", "Ctrl+O", "Esc"] {
        assert!(text.contains(key), "{key} missing in\n{text}");
    }
}

#[test]
fn the_height_stays_while_the_focus_moves_on_a_narrow_box() {
    let mut form = form();
    let height = |form: &FormState| {
        drawn(form, 34, 30)
            .iter()
            .filter(|r| r.contains('\u{2502}'))
            .count()
    };
    let first = height(&form);
    form.focus_on("kind");
    assert_eq!(height(&form), first);
}

#[test]
fn inline_the_title_leads_and_the_tint_stays_inside() {
    let form = form();
    let mut terminal =
        Terminal::new(TestBackend::new(60, 12)).expect("terminal");
    let area = ratatui::layout::Rect::new(4, 1, 50, 10);
    terminal
        .draw(|frame| form.render_inline(frame, area, &skin()))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let row = |y: u16| -> String {
        (0..60).map(|x| buffer[(x, y)].symbol()).collect()
    };
    assert!(row(1).contains("Entry"), "{}", row(1));
    assert!(row(3).contains("Title"), "{}", row(3));
    let tint = FieldStyles::from_palette(&skin().palette).focus_tint.bg;
    assert_eq!(buffer[(5, 3)].style().bg, tint, "the focused row is tinted");
    assert_ne!(buffer[(2, 3)].style().bg, tint, "no tint left of the area");
}

#[test]
fn the_footer_gets_field_and_form_keys_and_the_question_alone() {
    let mut form = form();
    let groups = form.hint_groups();
    assert_eq!(
        groups.iter().map(|g| g.0).collect::<Vec<_>>(),
        ["Field", "Form"]
    );
    assert!(form.prompt().is_none());
    form.focus_on("fav");
    press(&mut form, &[KeyCode::Char(' '), KeyCode::Esc]);
    assert_eq!(form.prompt(), Some("Save changes?"));
    let groups = form.hint_groups();
    assert_eq!(groups.len(), 1);
    assert!(groups[0].1.iter().any(|(key, _)| *key == "n"));
}
