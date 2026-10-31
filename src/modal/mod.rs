//! Reusable modal widgets. Each is a thin wrapper over [`crate::overlay::popup`]: it
//! sets up its state and closures and returns a [`ModalSignal`]. The dimmed
//! backdrop, box centering and event loop live in [`crate::overlay`], not here.
//!
//! Yes/no questions go through [`confirm`], which lets `Enter` mean yes. A
//! destructive action goes through [`confirm_default`] with
//! [`Question::declining`] instead, so a stray `Enter` cannot confirm the
//! deletion. [`confirm_styled`] asks the same question with a frame title and
//! a body the caller styles line by line, for a dialog that shows what is
//! about to be affected rather than one plain sentence; its body scrolls when
//! it does not fit, and [`confirm_styled_views`] adds a second view of the
//! body that `←`/`→` switch to.
//!
//! Every modal takes a [`crate::theme::Skin`], whose palette drives the colors.

mod confirm;
mod message;
mod multi_select_with;
mod number;
mod picker;
mod render;
mod select_with;
mod styled;
mod text_input;

pub use confirm::{Question, confirm, confirm_default};
pub use message::message;
pub use multi_select_with::{
    MultiChoice, MultiSelectOptions, multi_select_with,
};
pub use number::{number_input, number_input_bounded};
pub use picker::{
    ListAction, multi_select, multi_select_styled, select, select_reorderable,
    select_styled,
};
pub use select_with::{SelectItem, SelectOptions, select_with};
pub use styled::{
    StyledQuestion, StyledViews, confirm_styled, confirm_styled_views,
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

    use std::cell::RefCell;

    use ratatui::{Terminal, backend::TestBackend, text::Line};

    use super::{
        ModalSignal,
        confirm::{Question, confirm_area, confirm_key, render_confirm},
        message::message_area,
        picker::navigate_list,
        render::{
            HINT_BLOCK_ROWS, hinted_box_height, picker_area, text_extent,
        },
        styled::{
            Dialog, DialogState, Drawn, StyledQuestion, StyledViews,
            confirm_styled, confirm_styled_views, dialog_area, render_dialog,
        },
        text_input::{input_area, input_area_wide},
    };
    use crate::{
        overlay::PopupFlow,
        shortcut_hints,
        terminal::{Tui, clear_scripted_input, script_keys},
        theme::{
            ColorOverrides, GlyphVariant, Glyphs, Palette, Skin, ThemeRegistry,
        },
    };

    const SCREEN: Rect = Rect::new(0, 0, 80, 24);

    fn skin() -> Skin {
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
        )
    }

    /// Draws a confirm dialog asking `prompt` on a real frame and returns the
    /// text of every screen row, so the line breaks are checked on the
    /// rendered widget rather than on the sizing arithmetic alone.
    fn rendered_confirm(prompt: &str) -> Vec<String> {
        let backend = TestBackend::new(SCREEN.width, SCREEN.height);
        let mut terminal = Terminal::new(backend).expect("a test terminal");
        let question = Question::new(prompt);
        terminal
            .draw(|frame| {
                let rect = confirm_area(prompt, frame.area());
                render_confirm(frame, &skin(), &question, rect);
            })
            .expect("the frame draws");
        let buffer = terminal.backend().buffer();
        (0..SCREEN.height)
            .map(|y| {
                (0..SCREEN.width).map(|x| buffer[(x, y)].symbol()).collect()
            })
            .collect()
    }

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

    #[test]
    fn a_text_extent_is_the_longest_line_and_the_line_count() {
        assert_eq!(text_extent("one\nthree\nxy"), (5, 3));
        assert_eq!(text_extent("single"), (6, 1));
        assert_eq!(text_extent(""), (0, 1));
    }

    #[test]
    fn a_confirm_box_grows_one_row_per_further_prompt_line() {
        let single = confirm_area("Delete?", SCREEN);
        let triple = confirm_area("Delete?\nDate: today\nAmount: 1", SCREEN);
        assert_eq!(single.height, hinted_box_height());
        assert_eq!(triple.height, hinted_box_height() + 2);
    }

    /// The width follows the longest line, not the whole prompt: summing the
    /// lines is what laid a multi-line prompt out as one overlong row.
    #[test]
    fn a_confirm_box_is_as_wide_as_its_longest_prompt_line() {
        let prompt = "Current total: 508,02\nFactor: 0.136727";
        assert_eq!(
            confirm_area(prompt, SCREEN).width,
            confirm_area("Current total: 508,02", SCREEN).width,
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

    /// The dialog of `question` without a second view.
    fn single<'q, 'a>(question: &'q StyledQuestion<'a>) -> Dialog<'q, 'a> {
        Dialog {
            question,
            alternate: None,
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
            .map(|y| {
                (0..screen.width).map(|x| buffer[(x, y)].symbol()).collect()
            })
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
        assert_eq!(date, Some(title + 1));
        assert_eq!(amount, Some(title + 2));
        assert_eq!(hints, Some(title + 4), "the hint block stays in view");
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
            hinted_box_height() + 1
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
            assert!(
                matches!(answer, ModalSignal::Value(v) if v == default_yes)
            );
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
        assert!(
            dialog_area(&dialog, SMALL_SCREEN).height <= SMALL_SCREEN.height
        );
        let rows =
            rendered_dialog(&dialog, &DialogState::default(), SMALL_SCREEN);
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
        };
        let first = rendered_dialog(&dialog, &DialogState::default(), SCREEN);
        let mut alternate = DialogState::default();
        alternate.is_alternate = true;
        let second = rendered_dialog(&dialog, &alternate, SCREEN);
        assert!(first.iter().any(|row| row.contains("Date:    today")));
        assert!(second.iter().any(|row| row.contains("all in one row")));
        assert!(!second.iter().any(|row| row.contains("Date:")));
        let hint_row = |rows: &[String]| {
            rows.iter().position(|row| row.contains("enter/y"))
        };
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

    #[test]
    fn a_message_box_grows_with_its_body_lines_but_keeps_its_minimum() {
        assert_eq!(message_area("Saved.", SCREEN).height, 5);
        assert_eq!(message_area("a\nb\nc", SCREEN).height, 5);
        assert_eq!(message_area("a\nb\nc\nd\ne", SCREEN).height, 7);
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
