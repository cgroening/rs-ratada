//! A header bar with an accented brand and dim secondary text.
//!
//! `Minimal` renders a single plain line; `Boxed` wraps it in a rounded,
//! accent-bordered box.

use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
};

use super::style;
use crate::theme::Skin;

/// Renders a header: `brand` in accent bold, followed by `status` in dim. In
/// `Boxed` mode the line sits inside a rounded accent box.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    brand: &str,
    status: &str,
) {
    let line = Line::from(vec![
        Span::styled(
            format!(" {brand} "),
            style::fg(skin.palette.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {status}"), style::secondary(&skin.palette)),
    ]);
    let paragraph = Paragraph::new(line);
    if skin.is_boxed() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style::border(&skin.palette))
            .padding(Padding::horizontal(1));
        frame.render_widget(paragraph.block(block), area);
    } else {
        frame.render_widget(paragraph, area);
    }
}
