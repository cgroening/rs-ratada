//! The single overlay primitive: a generic popup driver plus the dimming
//! backdrop shared by every modal, picker and overlay.
//!
//! [`popup`] runs one blocking overlay. It dims the screen behind a centered
//! box (the underlying view stays visible but darkened), draws a caller-owned
//! box, and routes keys until the caller's handler resolves. Every blocking
//! widget in this toolkit is a thin wrapper over it, so the loop/`Clear`/
//! centering scaffolding lives here once (SSOT) instead of in each widget.

use std::io;

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color as RatColor, Modifier},
    widgets::Clear,
};

use super::{
    chrome::modal_block,
    modal::ModalSignal,
    quit::{self, QuitKind},
    shortcut_hints, style,
    terminal::{Tui, TuiEvent},
};
use crate::theme::Skin;

/// The darkening applied to the screen behind a popup (`0.0` = black,
/// `1.0` = unchanged).
pub(crate) const SCRIM_FACTOR: f32 = 0.4;

/// What a popup key handler decides after each key.
pub enum PopupFlow<T> {
    /// Keep the popup open.
    Continue,
    /// Close the popup, returning a value.
    Done(T),
    /// Close the popup without a value (the caller pressed Esc).
    Cancelled,
}

/// Runs one blocking popup and returns its outcome.
///
/// Each frame draws `render_bg` (the view behind), dims it, then draws the box
/// produced by `render_box` into the rect from `area`. Keys are handed to
/// `handle_key`; the global quit chord yields [`ModalSignal::Quit`] once
/// [`quit::request`] allows it, and the global hints toggle (see
/// `shortcut_hints::set_toggle_key`) is consumed here, so every modal inherits
/// it and never sees the key.
///
/// `state` is threaded explicitly so the render closures borrow it immutably and
/// `handle_key` borrows it mutably, sequentially, without aliasing. This covers
/// everything from a single cursor value to a rich picker state, and lets
/// `render_bg` re-render a live-mutated app (the settings overlay).
pub fn popup<S, T>(
    tui: &mut Tui,
    state: &mut S,
    area: impl Fn(Rect, &S) -> Rect,
    render_bg: impl Fn(&mut Frame, &S),
    render_box: impl Fn(&mut Frame, Rect, &S),
    mut handle_key: impl FnMut(&mut S, KeyEvent) -> PopupFlow<T>,
) -> io::Result<ModalSignal<T>> {
    loop {
        // One painter for the frame and for whatever the quit guard draws on
        // top of it, so its dialog sits over this popup, not over bare ground.
        let paint = |frame: &mut Frame| {
            render_bg(frame, state);
            dim(frame, SCRIM_FACTOR);
            let rect = area(frame.area(), state);
            frame.render_widget(Clear, rect);
            render_box(frame, rect, state);
        };
        tui.draw(paint)?;
        match tui.read_event()? {
            TuiEvent::Quit => {
                if quit::request(tui, QuitKind::Hard, &paint) {
                    return Ok(ModalSignal::Quit);
                }
            }
            TuiEvent::Resize => {}
            // The next iteration redraws with the new visibility.
            TuiEvent::Key(key) if shortcut_hints::consume_toggle(key) => {}
            TuiEvent::Key(key) => match handle_key(state, key) {
                PopupFlow::Continue => {}
                PopupFlow::Done(value) => {
                    return Ok(ModalSignal::Value(value));
                }
                PopupFlow::Cancelled => return Ok(ModalSignal::Cancelled),
            },
        }
    }
}

/// Draws the shared rounded modal frame into `rect` and returns the inner area
/// for the caller's content. Wraps [`modal_block`] so the box chrome stays a
/// single source of truth.
pub fn framed(frame: &mut Frame, rect: Rect, skin: &Skin, title: &str) -> Rect {
    let block = modal_block(skin, title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

/// Dims every cell currently in the frame toward black by `factor`, preserving
/// the drawn content. `Rgb` cells are darkened; cells with a non-`Rgb`
/// foreground (which has no RGB base to scale) get the terminal `DIM` attribute.
pub fn dim(frame: &mut Frame, factor: f32) {
    dim_buffer(frame.buffer_mut(), factor);
}

fn dim_buffer(buffer: &mut Buffer, factor: f32) {
    for cell in &mut buffer.content {
        if matches!(cell.fg, RatColor::Rgb(..)) {
            cell.fg = style::darken(cell.fg, factor);
        } else {
            cell.modifier |= Modifier::DIM;
        }
        if matches!(cell.bg, RatColor::Rgb(..)) {
            cell.bg = style::darken(cell.bg, factor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_darkens_rgb_foreground_and_background() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        buffer.content[0].fg = RatColor::Rgb(100, 100, 100);
        buffer.content[0].bg = RatColor::Rgb(200, 200, 200);
        dim_buffer(&mut buffer, 0.5);
        assert_eq!(buffer.content[0].fg, RatColor::Rgb(50, 50, 50));
        assert_eq!(buffer.content[0].bg, RatColor::Rgb(100, 100, 100));
    }

    #[test]
    fn dim_marks_non_rgb_foreground_dim() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        buffer.content[0].fg = RatColor::Reset;
        dim_buffer(&mut buffer, 0.5);
        assert!(buffer.content[0].modifier.contains(Modifier::DIM));
    }
}
