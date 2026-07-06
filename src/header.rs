//! A header bar with an accented brand and dim secondary text.

use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::style;
use crate::theme::Skin;

/// Renders a header: `brand` in accent bold, followed by `status` in dim.
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
    frame.render_widget(Paragraph::new(line), area);
}
