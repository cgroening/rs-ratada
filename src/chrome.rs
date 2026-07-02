//! Shared chrome: the framing that differs between the display modes.
//!
//! Centralises the one decision "framed or not" so views and widgets never
//! branch on [`Mode`](crate::theme::Mode) inline. In `Fancy` mode a [`panel`]
//! is a rounded, accent-bordered box with an inset title; in `Minimal` mode it
//! is a no-op frame, so content fills the area exactly as before.

use ratatui::{
    style::Modifier,
    text::Span,
    widgets::{Block, BorderType, Borders, Padding},
};

use super::style;
use crate::theme::Skin;

/// The container block for a view section. `Fancy` returns a rounded accent box
/// with `title`; `Minimal` returns an empty block whose inner area equals the
/// outer one, so existing layouts are unchanged. `Panels` returns a borderless
/// block that just insets its content by one cell all around, so a filled
/// column (see [`menu_panel`]) keeps its content off the panel edges.
pub fn panel(skin: &Skin, title: &str) -> Block<'static> {
    if skin.is_panels() {
        return Block::default().padding(Padding::uniform(1));
    }
    if !skin.is_fancy() {
        return Block::default();
    }
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style::fg(skin.palette.accent))
        .padding(Padding::horizontal(1))
        .title(title_span(skin, title))
}

/// Like [`panel`], but in `Panels` mode the box is filled with the `surface`
/// color so a list/menu column reads as its own panel against the body.
pub fn menu_panel(skin: &Skin, title: &str) -> Block<'static> {
    let block = panel(skin, title);
    if skin.is_panels() {
        block.style(style::bg(skin.palette.surface))
    } else {
        block
    }
}

/// The shared modal frame: a rounded accent border with a filled background and
/// an inset title. Unlike [`panel`], a modal is always framed; in `Fancy` mode
/// the body is padded and the title bold. Used by every blocking modal widget.
pub fn modal_block(skin: &Skin, title: &str) -> Block<'static> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style::fg(skin.palette.accent))
        .style(style::bg(skin.palette.background))
        .title(modal_title(skin, title));
    if skin.is_fancy() {
        block = block.padding(Padding::horizontal(1));
    }
    block
}

/// An inset title that reads as part of the top border (`╭─ Title ───`). Bold in
/// `Fancy` mode; the accent color in either.
fn modal_title(skin: &Skin, title: &str) -> Span<'static> {
    let mut style = style::fg(skin.palette.accent);
    if skin.is_fancy() {
        style = style.add_modifier(Modifier::BOLD);
    }
    Span::styled(format!("\u{2500} {} ", title.trim()), style)
}

/// An inset title that reads as part of the top border (`╭─ Title ───`); bold
/// in the accent color.
fn title_span(skin: &Skin, title: &str) -> Span<'static> {
    Span::styled(
        format!("\u{2500} {} ", title.trim()),
        style::fg(skin.palette.accent).add_modifier(Modifier::BOLD),
    )
}
