//! A horizontal progress gauge: an accent bar with a centered percentage label.
//!
//! Over the filled bar the label is drawn in a readable contrast color (near
//! black or near white, whichever suits the accent) so it stays legible; over
//! the empty track it keeps the accent color.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::style;
use crate::theme::Palette;

/// Renders a gauge filled to `ratio` (clamped to `0.0..=1.0`) on the first row
/// of `area`, with `label` centered over the bar.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
    ratio: f64,
    label: &str,
) {
    let width = area.width as usize;
    if width == 0 {
        return;
    }
    let filled = (ratio.clamp(0.0, 1.0) * width as f64).round() as usize;
    let label_chars: Vec<char> = label.chars().collect();
    let label_start = width.saturating_sub(label_chars.len()) / 2;

    let accent = style::to_ratatui(palette.accent);
    let on_fill =
        style::to_ratatui(palette.background.readable_on(palette.accent));
    let track = style::to_ratatui(palette.selection);

    let spans: Vec<Span> = (0..width)
        .map(|index| {
            let filled_here = index < filled;
            let label_char = index
                .checked_sub(label_start)
                .and_then(|offset| label_chars.get(offset));
            if let Some(&ch) = label_char {
                // Over the filled bar the text uses a readable contrast color;
                // over the track it stays in the accent color. The background
                // is always the underlying bar/track.
                let (foreground, background) = if filled_here {
                    (on_fill, accent)
                } else {
                    (accent, track)
                };
                Span::styled(
                    ch.to_string(),
                    Style::default()
                        .fg(foreground)
                        .bg(background)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                let background = if filled_here { accent } else { track };
                Span::styled(" ".to_string(), Style::default().bg(background))
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
