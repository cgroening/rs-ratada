//! Single-line and wide text-entry modals.

use std::io;

use crossterm::event::KeyCode;
use ratatui::{Frame, layout::Rect, widgets::Paragraph};

use super::ModalSignal;
use super::render::{hint_block, hinted_box_height};
use crate::{
    input::{self, TextCursor},
    layout::{centered_rect, fit},
    overlay::{self, PopupFlow, popup_with_paste},
    terminal::Tui,
    theme::Skin,
};

/// Prompts for a single line of text. `Enter` accepts, `Esc` cancels.
pub fn input(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<String>> {
    input_impl(tui, skin, title, initial, input_area, render_bg)
}

/// Like [`input()`], but the box spans most of the terminal width, so a long
/// value (such as a file path) stays visible instead of scrolling in a narrow
/// box. `Enter` accepts, `Esc` cancels.
pub fn input_wide(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<String>> {
    input_impl(tui, skin, title, initial, input_area_wide, render_bg)
}

/// Shared single-line text prompt; `area` sizes the box from the frame.
pub(super) fn input_impl(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: &str,
    area: impl Fn(Rect) -> Rect,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<String>> {
    let mut state = TextField {
        cursor: TextCursor::at_end(initial),
        text: initial.to_string(),
    };
    popup_with_paste(
        tui,
        &mut state,
        |rect, _| area(rect),
        |frame, _| render_bg(frame),
        |frame, rect, field: &TextField| {
            render_input(frame, skin, title, &field.text, &field.cursor, rect);
        },
        |field, key| match key.code {
            KeyCode::Enter => PopupFlow::Done(field.text.clone()),
            KeyCode::Esc => PopupFlow::Cancelled,
            _ => {
                input::apply_edit_key(
                    &mut field.text,
                    &mut field.cursor,
                    key,
                    input::EditMode::SingleLine,
                    None,
                );
                PopupFlow::Continue
            }
        },
        |field: &mut TextField, text| {
            input::paste_text(
                &mut field.text,
                &mut field.cursor,
                input::EditMode::SingleLine,
                None,
                &text,
            );
            PopupFlow::Continue
        },
    )
}

/// The text field state shared by [`input`]: an edit buffer plus its caret.
pub(super) struct TextField {
    text: String,
    cursor: TextCursor,
}

pub(super) fn render_input(
    frame: &mut Frame,
    skin: &Skin,
    title: &str,
    text: &str,
    cursor: &TextCursor,
    rect: Rect,
) {
    let inner = overlay::framed(frame, rect, skin, title);
    let width = inner.width as usize;
    let line = input::render_line(text, cursor, &skin.palette, width, true);
    let mut lines = vec![line];
    lines.extend(hint_block(
        &[("enter", "ok"), ("esc", "cancel")],
        &skin.palette,
        width,
    ));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The popup rect for the single-line text inputs.
pub(super) fn input_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(8).clamp(20, 60);
    centered_rect(width, hinted_box_height(), area)
}

/// A wide input box (~90% of the terminal width), for long values.
pub(super) fn input_area_wide(area: Rect) -> Rect {
    let width = fit(area.width * 9 / 10, 20, area.width);
    centered_rect(width, hinted_box_height(), area)
}
