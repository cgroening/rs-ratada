//! A read-only preview of the active theme: every palette color as a labeled
//! swatch, plus the accent variant ladder.
//!
//! Drop it into any app (e.g. a gallery entry) to check the configured theme at
//! a glance. Each row shows the color name and a swatch with its hex printed on
//! it in a readable contrast color.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::style;
use crate::theme::{Color, Palette, Skin};

/// Column width reserved for the color name.
const NAME_WIDTH: usize = 16;

/// Renders the palette swatch list and the accent ladder into `area`.
pub fn render(frame: &mut Frame, area: Rect, skin: &Skin) {
    let palette = &skin.palette;

    let mut lines: Vec<Line<'static>> = palette
        .entries()
        .into_iter()
        .map(|(name, color)| swatch_line(palette, name, color))
        .collect();
    lines.push(Line::from(""));
    lines.push(ladder_line(palette));

    frame.render_widget(Paragraph::new(lines), area);
}

/// One swatch row: the color name, then a block filled with the color and its
/// hex printed on top in a readable contrast color.
fn swatch_line(palette: &Palette, name: &str, color: Color) -> Line<'static> {
    let swatch =
        style::bg(color).fg(style::to_ratatui(color.readable_on(color)));
    Line::from(vec![
        Span::styled(
            format!(" {name:<NAME_WIDTH$}"),
            style::secondary(palette),
        ),
        Span::styled(format!("  {}  ", color.to_hex()), swatch),
    ])
}

/// The accent variant ladder: dark-to-light shades plus vivid and dim.
fn ladder_line(palette: &Palette) -> Line<'static> {
    let accent = palette.accent;
    let ladder = [
        accent.shade(-3),
        accent.shade(-2),
        accent.shade(-1),
        accent,
        accent.shade(1),
        accent.shade(2),
        accent.shade(3),
        accent.vivid(0.4),
        accent.dim(0.6),
    ];
    let mut spans = vec![Span::styled(
        format!(" {:<NAME_WIDTH$}", "accent ladder"),
        style::secondary(palette),
    )];
    spans.extend(
        ladder
            .into_iter()
            .map(|shade| Span::styled("  ", style::bg(shade))),
    );
    Line::from(spans)
}
