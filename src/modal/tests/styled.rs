//! The styled dialog: body, views, toggles, scrolling.

use super::*;

/// The dialog of `question` without a second view.
fn single<'q, 'a>(question: &'q StyledQuestion<'a>) -> Dialog<'q, 'a> {
    Dialog {
        question,
        alternate: None,
        toggle: None,
    }
}

/// Draws a styled confirm dialog on a real frame and returns the text of
/// every screen row.
fn rendered_styled(question: &StyledQuestion<'_>) -> Vec<String> {
    rendered_dialog(&single(question), &DialogState::default(), SCREEN)
}

/// Draws `dialog` in `state` on a `screen` sized frame and returns the
/// text of every screen row.
fn rendered_dialog(
    dialog: &Dialog<'_, '_>,
    state: &DialogState,
    screen: Rect,
) -> Vec<String> {
    let backend = TestBackend::new(screen.width, screen.height);
    let mut terminal = Terminal::new(backend).expect("a test terminal");
    terminal
        .draw(|frame| {
            let rect = dialog_area(dialog, frame.area());
            render_dialog(frame, &skin(), &Drawn { dialog, state }, rect);
        })
        .expect("the frame draws");
    let buffer = terminal.backend().buffer();
    (0..screen.height)
        .map(|y| (0..screen.width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

fn two_rows(_width: u16) -> Vec<Line<'static>> {
    vec![Line::from("Date:    today"), Line::from("Amount:  -4,20")]
}

#[test]
fn a_styled_confirm_shows_its_title_body_rows_and_hints() {
    let question = StyledQuestion {
        title: " Delete? ",
        lines: &two_rows,
        default_yes: true,
    };
    let rows = rendered_styled(&question);
    let title = rows.iter().position(|row| row.contains(" Delete? "));
    let date = rows.iter().position(|row| row.contains("Date:    today"));
    let amount = rows.iter().position(|row| row.contains("Amount:  -4,20"));
    let hints = rows.iter().position(|row| row.contains("enter/y"));
    let title = title.expect("the title sits in the frame");
    // The top padding row separates the body from the title.
    assert_eq!(date, Some(title + 2));
    assert_eq!(amount, Some(title + 3));
    assert_eq!(hints, Some(title + 5), "the hint block stays in view");
}

#[test]
fn a_styled_confirm_box_grows_one_row_per_further_body_line() {
    let question = StyledQuestion {
        title: " Delete? ",
        lines: &two_rows,
        default_yes: true,
    };
    assert_eq!(
        dialog_area(&single(&question), SCREEN).height,
        crate::chrome::MODAL_CHROME_HEIGHT + 1 + 2 + 1
    );
}

/// A title longer than the body must not be cut off by a box sized for
/// the body alone.
#[test]
fn a_styled_confirm_box_is_wide_enough_for_a_long_title() {
    let title = " Combine all of these into one transaction? ";
    let question = StyledQuestion {
        title,
        lines: &|_| vec![Line::from("x")],
        default_yes: true,
    };
    let rows = rendered_styled(&question);
    assert!(rows.iter().any(|row| row.contains(title)));
}

/// Sizing and drawing must ask for the same body width, or a body that
/// wraps to it would be measured at one height and drawn at another.
#[test]
fn a_styled_confirm_asks_for_one_body_width_when_sizing_and_drawing() {
    let seen = RefCell::new(Vec::new());
    let lines = |width: u16| {
        seen.borrow_mut().push(width);
        vec![Line::from("x".repeat(usize::from(width)))]
    };
    let question = StyledQuestion {
        title: " Wide ",
        lines: &lines,
        default_yes: true,
    };
    rendered_styled(&question);
    let seen = seen.borrow();
    assert!(seen.len() >= 2);
    assert!(seen.iter().all(|width| *width == seen[0]), "{seen:?}");
}

#[test]
fn a_styled_confirm_honours_its_enter_default() {
    let mut tui = Tui::for_test(SCREEN.width, SCREEN.height)
        .expect("an in-memory terminal");
    for default_yes in [true, false] {
        script_keys([key(KeyCode::Enter)]);
        let question = StyledQuestion {
            title: " Delete? ",
            lines: &two_rows,
            default_yes,
        };
        let answer = confirm_styled(&mut tui, &skin(), &question, |_| {})
            .expect("the scripted modal runs");
        assert!(matches!(answer, ModalSignal::Value(v) if v == default_yes));
    }
    clear_scripted_input();
}

fn numbered_rows(_width: u16) -> Vec<Line<'static>> {
    (1..=30)
        .map(|row| Line::from(format!("row {row}")))
        .collect()
}

fn compact_row(_width: u16) -> Vec<Line<'static>> {
    vec![Line::from("all in one row")]
}

/// A 12-row screen: the box keeps its frame and hints, the body scrolls.
const SMALL_SCREEN: Rect = Rect::new(0, 0, 60, 12);

#[test]
fn a_tall_styled_body_is_cut_to_the_screen_and_offers_scrolling() {
    let question = StyledQuestion {
        title: " Rename? ",
        lines: &numbered_rows,
        default_yes: true,
    };
    let dialog = single(&question);
    assert!(dialog_area(&dialog, SMALL_SCREEN).height <= SMALL_SCREEN.height);
    let rows = rendered_dialog(&dialog, &DialogState::default(), SMALL_SCREEN);
    assert!(rows.iter().any(|row| row.contains("row 1 ")));
    assert!(!rows.iter().any(|row| row.contains("row 30")));
    assert!(rows.iter().any(|row| row.contains("scroll")));
    assert!(rows.iter().any(|row| row.contains("enter/y")));
}

#[test]
fn a_styled_body_that_fits_offers_no_scrolling() {
    let question = StyledQuestion {
        title: " Delete? ",
        lines: &two_rows,
        default_yes: true,
    };
    assert!(
        !rendered_styled(&question)
            .iter()
            .any(|row| row.contains("scroll"))
    );
}

#[test]
fn scrolling_keys_clamp_at_both_ends_and_still_answer() {
    let mut tui = Tui::for_test(SMALL_SCREEN.width, SMALL_SCREEN.height)
        .expect("an in-memory terminal");
    let question = StyledQuestion {
        title: " Rename? ",
        lines: &numbered_rows,
        default_yes: false,
    };
    script_keys([
        key(KeyCode::Up),
        key(KeyCode::End),
        key(KeyCode::PageDown),
        key(KeyCode::Down),
        key(KeyCode::Home),
        key(KeyCode::Char('y')),
    ]);
    let answer = confirm_styled(&mut tui, &skin(), &question, |_| {})
        .expect("the scripted modal runs");
    clear_scripted_input();
    assert!(matches!(answer, ModalSignal::Value(true)));
}

#[test]
fn the_scroll_offset_is_drawn_clamped() {
    let question = StyledQuestion {
        title: " Rename? ",
        lines: &numbered_rows,
        default_yes: true,
    };
    let dialog = single(&question);
    let mut state = DialogState::default();
    state.offset = 1_000;
    let rows = rendered_dialog(&dialog, &state, SMALL_SCREEN);
    assert!(rows.iter().any(|row| row.contains("row 30")));
}

#[test]
fn the_second_view_is_drawn_in_a_box_sized_for_both() {
    let question = StyledQuestion {
        title: " Rename? ",
        lines: &two_rows,
        default_yes: true,
    };
    let dialog = Dialog {
        question: &question,
        alternate: Some(&compact_row),
        toggle: None,
    };
    let first = rendered_dialog(&dialog, &DialogState::default(), SCREEN);
    let mut alternate = DialogState::default();
    alternate.is_alternate = true;
    let second = rendered_dialog(&dialog, &alternate, SCREEN);
    assert!(first.iter().any(|row| row.contains("Date:    today")));
    assert!(second.iter().any(|row| row.contains("all in one row")));
    assert!(!second.iter().any(|row| row.contains("Date:")));
    let hint_row =
        |rows: &[String]| rows.iter().position(|row| row.contains("enter/y"));
    assert_eq!(
        hint_row(&first),
        hint_row(&second),
        "the box keeps its size"
    );
    assert!(first.iter().any(|row| row.contains("view")));
}

#[test]
fn arrows_switch_the_view_and_answers_still_work() {
    let mut tui = Tui::for_test(SCREEN.width, SCREEN.height)
        .expect("an in-memory terminal");
    let views = StyledViews {
        question: StyledQuestion {
            title: " Rename? ",
            lines: &two_rows,
            default_yes: true,
        },
        alternate: &compact_row,
        toggle: None,
    };
    script_keys([
        key(KeyCode::Right),
        key(KeyCode::Left),
        key(KeyCode::Char('n')),
    ]);
    let answer = confirm_styled_views(&mut tui, &skin(), &views, |_| {})
        .expect("the scripted modal runs");
    clear_scripted_input();
    assert!(matches!(answer, ModalSignal::Value(false)));
}

fn table_rows(_width: u16) -> Vec<Line<'static>> {
    vec![Line::from("Date │ Amount"), Line::from("today │ -4,20")]
}

/// The table toggle of the tests: `t`, hinted as `t table`.
fn table_toggle() -> ToggleView<'static> {
    ToggleView {
        key: 't',
        hint: ("t", "table"),
        lines: &table_rows,
    }
}

#[test]
fn the_toggled_view_shows_over_both_others_without_the_view_hint() {
    let question = StyledQuestion {
        title: " Rename? ",
        lines: &two_rows,
        default_yes: true,
    };
    let toggle = table_toggle();
    let dialog = Dialog {
        question: &question,
        alternate: Some(&compact_row),
        toggle: Some(&toggle),
    };
    let closed = rendered_dialog(&dialog, &DialogState::default(), SCREEN);
    assert!(closed.iter().any(|row| row.contains("view")));
    assert!(closed.iter().any(|row| row.contains("table")));
    for is_alternate in [false, true] {
        let mut state = DialogState::default();
        state.is_alternate = is_alternate;
        state.is_toggled = true;
        let shown = rendered_dialog(&dialog, &state, SCREEN);
        assert!(shown.iter().any(|row| row.contains("today │ -4,20")));
        assert!(!shown.iter().any(|row| row.contains("view")));
    }
}

#[test]
fn the_toggle_key_shows_and_hides_its_view_and_blocks_the_arrows() {
    let question = StyledQuestion {
        title: " Rename? ",
        lines: &two_rows,
        default_yes: true,
    };
    let toggle = table_toggle();
    let dialog = Dialog {
        question: &question,
        alternate: Some(&compact_row),
        toggle: Some(&toggle),
    };
    let mut state = DialogState::default();
    assert!(matches!(
        dialog_key(&dialog, &mut state, key(KeyCode::Char('t'))),
        PopupFlow::Continue
    ));
    assert!(state.is_toggled);
    dialog_key(&dialog, &mut state, key(KeyCode::Right));
    assert!(
        !state.is_alternate,
        "the arrows wait for the table to close"
    );
    dialog_key(&dialog, &mut state, key(KeyCode::Char('t')));
    assert!(!state.is_toggled);
    dialog_key(&dialog, &mut state, key(KeyCode::Right));
    assert!(state.is_alternate);
}

#[test]
fn a_toggled_view_still_answers_the_question() {
    let mut tui = Tui::for_test(SCREEN.width, SCREEN.height)
        .expect("an in-memory terminal");
    let views = StyledViews {
        question: StyledQuestion {
            title: " Rename? ",
            lines: &two_rows,
            default_yes: true,
        },
        alternate: &compact_row,
        toggle: Some(table_toggle()),
    };
    script_keys([key(KeyCode::Char('t')), key(KeyCode::Enter)]);
    let answer = confirm_styled_views(&mut tui, &skin(), &views, |_| {})
        .expect("the scripted modal runs");
    clear_scripted_input();
    assert!(matches!(answer, ModalSignal::Value(true)));
}
