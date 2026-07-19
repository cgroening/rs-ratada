//! Digit-filtered number entry, plain and range-bounded.

use std::io;

use crossterm::event::KeyCode;
use ratatui::Frame;

use super::ModalSignal;
use super::text_input::{input_area, render_input};
use crate::{
    input::{self, TextCursor},
    overlay::{PopupFlow, popup_with_paste},
    terminal::Tui,
    theme::Skin,
};

/// Prompts for an integer, accepting digits (and a leading minus) only.
pub fn number_input(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: i64,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<i64>> {
    number_impl(tui, skin, title, initial, None, render_bg)
}

/// Like [`number_input`], but the accepted value is clamped to `[min, max]`.
/// `Enter` accepts (clamping), `Esc` cancels.
pub fn number_input_bounded(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: i64,
    min: i64,
    max: i64,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<i64>> {
    number_impl(tui, skin, title, initial, Some((min, max)), render_bg)
}

/// Shared integer prompt; `bounds` clamps the accepted value when set.
fn number_impl(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: i64,
    bounds: Option<(i64, i64)>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<i64>> {
    let mut text = initial.to_string();
    popup_with_paste(
        tui,
        &mut text,
        |area, _| input_area(area),
        |frame, _| render_bg(frame),
        |frame, rect, text: &String| {
            let cursor = TextCursor::at_end(text);
            render_input(frame, skin, title, text, &cursor, rect);
        },
        |text, key| match key.code {
            KeyCode::Enter => {
                let value = text.parse::<i64>().unwrap_or(initial);
                let value =
                    bounds.map_or(value, |(min, max)| value.clamp(min, max));
                PopupFlow::Done(value)
            }
            KeyCode::Esc => PopupFlow::Cancelled,
            KeyCode::Backspace => {
                text.pop();
                PopupFlow::Continue
            }
            KeyCode::Char(ch)
                if !input::is_command(key) && is_number_char(ch, text) =>
            {
                text.push(ch);
                PopupFlow::Continue
            }
            _ => PopupFlow::Continue,
        },
        |text: &mut String, pasted| {
            for ch in pasted.chars() {
                if is_number_char(ch, text) {
                    text.push(ch);
                }
            }
            PopupFlow::Continue
        },
    )
}

/// Whether `ch` may extend the number buffer `text`: a digit, or a leading `-`
/// only while the buffer is still empty.
fn is_number_char(ch: char, text: &str) -> bool {
    ch.is_ascii_digit() || (ch == '-' && text.is_empty())
}
