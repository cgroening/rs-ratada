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
        PickerChrome, hint_block_height, hinted_box_height, picker_area,
        render_picker, text_extent,
    },
    styled::{
        Dialog, DialogState, Drawn, StyledQuestion, StyledViews, ToggleView,
        confirm_styled, confirm_styled_views, dialog_area, dialog_key,
        render_dialog,
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
            let rect = confirm_area(&question, frame.area());
            render_confirm(frame, &skin(), &question, rect);
        })
        .expect("the frame draws");
    let buffer = terminal.backend().buffer();
    (0..SCREEN.height)
        .map(|y| (0..SCREEN.width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// Draws a list picker over `items` with `hints` and returns the text of
/// every screen row.
fn rendered_picker(items: &[String], hints: &[(&str, &str)]) -> Vec<String> {
    let backend = TestBackend::new(SCREEN.width, SCREEN.height);
    let mut terminal = Terminal::new(backend).expect("a test terminal");
    let chrome = PickerChrome {
        title: "Pick",
        cursor: 0,
        checked: None,
        hints,
    };
    terminal
        .draw(|frame| {
            let rect = picker_area(frame.area(), items.len(), 10, hints);
            render_picker(frame, &skin(), items, &chrome, rect);
        })
        .expect("the frame draws");
    let buffer = terminal.backend().buffer();
    (0..SCREEN.height)
        .map(|y| (0..SCREEN.width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

mod confirm;
mod picker;
mod styled;

/// Every popup wants a minimum width or height. A terminal smaller than
/// that must shrink the popup, not panic: these helpers used to reach
/// `clamp(min, max)` with `max < min`.
#[test]
fn popup_geometry_survives_a_terminal_below_its_minimum() {
    for (width, height) in [(1, 1), (4, 2), (20, 6), (27, 10)] {
        let area = Rect::new(0, 0, width, height);
        for rect in [
            picker_area(area, 40, 10, &[("enter", "pick")]),
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
    let picker = picker_area(area, 4, 40, &[("enter", "pick")]);
    // As wide as its rows (label, pointer and scrollbar columns) plus
    // the frame and padding - not a share of the screen.
    assert_eq!(picker.width, 40 + 3 + 6);
    let narrow = picker_area(area, 4, 3, &[("enter", "pick")]);
    assert_eq!(narrow.width, 28 + 6); // the narrowest picker
    // One row per item, plus the frame and the hint block.
    assert_eq!(picker.height, 4 + crate::chrome::MODAL_CHROME_HEIGHT + 2);
}

#[test]
fn a_text_extent_is_the_longest_line_and_the_line_count() {
    assert_eq!(text_extent("one\nthree\nxy"), (5, 3));
    assert_eq!(text_extent("single"), (6, 1));
    assert_eq!(text_extent(""), (0, 1));
}

#[test]
fn a_message_box_grows_with_its_body_lines_but_keeps_its_minimum() {
    let frame = crate::chrome::MODAL_CHROME_HEIGHT;
    assert_eq!(message_area("Saved.", SCREEN).height, frame + 3);
    assert_eq!(message_area("a\nb\nc", SCREEN).height, frame + 3);
    assert_eq!(message_area("a\nb\nc\nd\ne", SCREEN).height, frame + 5);
}

/// On a screen narrower than the hint line, the hints wrap onto more
/// rows instead of losing keys, and the box grows by them.
#[test]
fn hints_wrap_on_a_narrow_screen_and_the_box_grows() {
    let hints = [
        ("enter", "pick"),
        ("esc", "cancel"),
        ("tab", "next field"),
        ("ctrl+o", "reset"),
    ];
    assert_eq!(hint_block_height(&hints, 80), 2);
    assert_eq!(hint_block_height(&hints, 28), 4);
    let lines = super::hint_block(&hints, &skin().palette, 28);
    let text: String = lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    for (key, _) in hints {
        assert!(text.contains(key), "{key} missing in\n{text}");
    }
    let picker = picker_area(Rect::new(0, 0, 30, 24), 2, 10, &hints);
    let wide = picker_area(Rect::new(0, 0, 80, 24), 2, 10, &hints);
    assert!(picker.height > wide.height, "{picker:?} vs {wide:?}");
}
