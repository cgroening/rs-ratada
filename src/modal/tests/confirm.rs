//! The yes/no question: answers, defaults, sizing and drawing.

use super::*;

/// The whole point of the guard: a modified `y` must never confirm.
/// `confirm_default` is what every destructive dialog goes through, so a
/// stray chord answering "yes" is silent and unrecoverable. `AltGr+Y`
/// counts too - it is a character, but not a bare one.
#[test]
fn a_modified_y_does_not_confirm_a_prompt() {
    let altgr = |ch| {
        KeyEvent::new(
            KeyCode::Char(ch),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        )
    };
    for modified in [
        ctrl(KeyCode::Char('y')),
        ctrl(KeyCode::Char('Y')),
        altgr('y'),
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::ALT),
    ] {
        assert!(
            matches!(confirm_key(modified, false), PopupFlow::Continue),
            "{modified:?} must not confirm"
        );
    }
    // The bare key still answers, in either case.
    assert!(matches!(
        confirm_key(key(KeyCode::Char('y')), false),
        PopupFlow::Done(true)
    ));
    assert!(matches!(
        confirm_key(
            KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT),
            false
        ),
        PopupFlow::Done(true)
    ));
}

/// The same rule for the declining answer, so a chord cannot dismiss a
/// prompt the user never read either.
#[test]
fn a_modified_n_does_not_decline_a_prompt() {
    assert!(matches!(
        confirm_key(ctrl(KeyCode::Char('n')), true),
        PopupFlow::Continue
    ));
    assert!(matches!(
        confirm_key(key(KeyCode::Char('n')), true),
        PopupFlow::Done(false)
    ));
}

/// `Enter` follows the question's default, and `Esc` always declines.
#[test]
fn confirm_honours_the_default_and_esc_declines() {
    assert!(matches!(
        confirm_key(key(KeyCode::Enter), true),
        PopupFlow::Done(true)
    ));
    assert!(matches!(
        confirm_key(key(KeyCode::Enter), false),
        PopupFlow::Done(false)
    ));
    assert!(matches!(
        confirm_key(key(KeyCode::Esc), true),
        PopupFlow::Done(false)
    ));
}

#[test]
fn a_plain_question_lets_enter_confirm() {
    let question = Question::new("Save the file?");
    assert!(question.default_yes);
    assert_eq!(question.hints(), [("Enter/y", "yes"), ("n", "no")]);
}

/// The point of `declining`: a stray `Enter` on a destructive prompt must
/// answer "no", and the footer must advertise that binding.
#[test]
fn a_declining_question_lets_enter_decline() {
    let question = Question::declining("Delete everything?");
    assert!(!question.default_yes);
    assert_eq!(question.hints(), [("y", "yes"), ("Enter/n", "no")]);
}

#[test]
fn a_confirm_box_grows_one_row_per_further_prompt_line() {
    let single = confirm_area(&Question::new("Delete?"), SCREEN);
    let triple =
        confirm_area(&Question::new("Delete?\nDate: today\nAmount: 1"), SCREEN);
    let one_row = crate::chrome::MODAL_CHROME_HEIGHT + 1 + 2;
    assert_eq!(single.height, one_row);
    assert_eq!(triple.height, one_row + 2);
}

/// The width follows the longest line, not the whole prompt: summing the
/// lines is what laid a multi-line prompt out as one overlong row.
#[test]
fn a_confirm_box_is_as_wide_as_its_longest_prompt_line() {
    let prompt = "Current total: 508,02\nFactor: 0.136727";
    assert_eq!(
        confirm_area(&Question::new(prompt), SCREEN).width,
        confirm_area(&Question::new("Current total: 508,02"), SCREEN).width,
    );
}

#[test]
fn a_confirm_renders_each_prompt_line_on_its_own_row() {
    let rows = rendered_confirm("Current total: 508,02\nFactor: 0.136727");
    let total = rows.iter().position(|row| row.contains("Current total"));
    let factor = rows.iter().position(|row| row.contains("Factor:"));
    let hints = rows.iter().position(|row| row.contains("yes"));
    let total = total.expect("the first prompt line is drawn");
    assert_eq!(factor, Some(total + 1));
    assert_eq!(hints, Some(total + 3), "the hint block stays in view");
}

/// A confirm's key prompt is essential, so the box reserves its hint rows
/// even while the global F1 hints are hidden (that toggle governs only the
/// main-app footer).
#[test]
fn a_confirm_box_keeps_its_hint_rows_while_global_hints_are_hidden() {
    let hints = [("y", "yes"), ("n", "no")];
    let expected = crate::chrome::MODAL_CHROME_HEIGHT + 1 + 2;
    assert_eq!(hinted_box_height(&hints, 40), expected);
    shortcut_hints::set_visible(false);
    assert_eq!(hinted_box_height(&hints, 40), expected);
    shortcut_hints::set_visible(true);
}
