//! A one-line status bar: left- and right-aligned segments over a subtle tint.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::style;
use crate::theme::{Color, Skin};

/// Renders a status bar in `area`: `left` in dim text, `right` in the accent
/// color, separated by filler, over the `bg` tint (the caller picks it so the
/// bar matches its surrounding mode).
pub fn render(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    bg: Color,
    left: &str,
    right: &str,
) {
    let palette = &skin.palette;
    let width = area.width as usize;
    let gap = width
        .saturating_sub(left.width())
        .saturating_sub(right.width());
    let line = Line::from(vec![
        Span::styled(left.to_string(), style::dim()),
        Span::raw(" ".repeat(gap)),
        Span::styled(right.to_string(), style::fg(palette.accent)),
    ]);
    frame.render_widget(Paragraph::new(line).style(style::bg(bg)), area);
}
