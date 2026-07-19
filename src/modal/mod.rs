//! Reusable modal widgets. Each is a thin wrapper over [`crate::overlay::popup`]: it
//! sets up its state and closures and returns a [`ModalSignal`]. The dimmed
//! backdrop, box centering and event loop live in [`crate::overlay`], not here.
//!
//! Yes/no questions go through [`confirm`], which lets `Enter` mean yes. A
//! destructive action goes through [`confirm_default`] with
//! [`Question::declining`] instead, so a stray `Enter` cannot confirm the
//! deletion.
//!
//! Every modal takes a [`crate::theme::Skin`], whose palette drives the colors.

mod confirm;
mod message;
mod number;
mod picker;
mod render;
mod text_input;

pub use confirm::{Question, confirm, confirm_default};
pub use message::message;
pub use number::{number_input, number_input_bounded};
pub use picker::{
    ListAction, multi_select, multi_select_styled, select, select_reorderable,
    select_styled,
};
pub use text_input::{input, input_wide};

/// Outcome of a modal interaction.
pub enum ModalSignal<T> {
    /// The user confirmed with a value.
    Value(T),
    /// The user dismissed the modal (Esc).
    Cancelled,
    /// The global quit chord was pressed inside the modal.
    Quit,
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use ratatui::layout::Rect;

    use super::{
        confirm::{Question, confirm_key},
        picker::navigate_list,
        render::{HINT_BLOCK_ROWS, hinted_box_height, picker_area},
        text_input::{input_area, input_area_wide},
    };
    use crate::{overlay::PopupFlow, shortcut_hints};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

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

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
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

    /// Every popup wants a minimum width or height. A terminal smaller than
    /// that must shrink the popup, not panic: these helpers used to reach
    /// `clamp(min, max)` with `max < min`.
    #[test]
    fn popup_geometry_survives_a_terminal_below_its_minimum() {
        for (width, height) in [(1, 1), (4, 2), (20, 6), (27, 10)] {
            let area = Rect::new(0, 0, width, height);
            for rect in [
                picker_area(area, 40),
                input_area(area),
                input_area_wide(area),
            ] {
                assert!(rect.width <= area.width, "{rect:?} in {area:?}");
                assert!(rect.height <= area.height, "{rect:?} in {area:?}");
            }
        }
    }

    #[test]
    fn a_roomy_terminal_still_gets_the_preferred_size() {
        let area = Rect::new(0, 0, 100, 40);
        let picker = picker_area(area, 4);
        assert_eq!(picker.width, 50); // half the width
        assert_eq!(picker.height, 6); // one row per item, plus borders
    }

    #[test]
    fn a_plain_question_lets_enter_confirm() {
        let question = Question::new("Save the file?");
        assert!(question.default_yes);
        assert_eq!(question.hints(), [("enter/y", "yes"), ("n", "no")]);
    }

    /// The point of `declining`: a stray `Enter` on a destructive prompt must
    /// answer "no", and the footer must advertise that binding.
    #[test]
    fn a_declining_question_lets_enter_decline() {
        let question = Question::declining("Delete everything?");
        assert!(!question.default_yes);
        assert_eq!(question.hints(), [("y", "yes"), ("enter/n", "no")]);
    }

    /// A confirm's key prompt is essential, so the box reserves its hint rows
    /// even while the global F1 hints are hidden (that toggle governs only the
    /// main-app footer).
    #[test]
    fn a_confirm_box_keeps_its_hint_rows_while_global_hints_are_hidden() {
        assert_eq!(hinted_box_height(), 3 + HINT_BLOCK_ROWS);
        shortcut_hints::set_visible(false);
        assert_eq!(hinted_box_height(), 3 + HINT_BLOCK_ROWS);
        shortcut_hints::set_visible(true);
    }
}
