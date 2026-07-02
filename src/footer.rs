//! Footer rendering: wrapped key hints plus an optional transient status line.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::style;
use crate::theme::{Color, Palette};

const SEPARATOR: &str = " \u{00b7} ";

/// Wraps `(key, description)` hints into lines at `width` without splitting a
/// token across lines. `key_color` styles the keys (e.g. a dimmed accent).
pub fn lines<S: AsRef<str>>(
    items: &[(S, S)],
    key_color: Color,
    width: usize,
) -> Vec<Line<'static>> {
    let key_style = style::fg(key_color).add_modifier(Modifier::BOLD);
    let desc_style = style::dim();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;

    for (key, description) in items {
        let (key, description) = (key.as_ref(), description.as_ref());
        let token_width = format!("{key} {description}").width();
        let separator_width = if spans.is_empty() {
            0
        } else {
            SEPARATOR.width()
        };
        if !spans.is_empty() && used + separator_width + token_width > width {
            lines.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
        }
        if !spans.is_empty() {
            spans.push(Span::styled(SEPARATOR, desc_style));
            used += SEPARATOR.width();
        }
        spans.push(Span::styled(format!("{key} "), key_style));
        spans.push(Span::styled(description.to_string(), desc_style));
        used += token_width;
    }
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
}

/// Footer height for `line_count` hint lines, plus one row when a status line
/// is shown. At least one row.
pub fn height(line_count: usize, has_status: bool) -> u16 {
    (line_count as u16 + u16::from(has_status)).max(1)
}

/// Renders the footer into `area`: an optional accent status line above the
/// hint lines.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
    status: Option<&str>,
    hints: Vec<Line<'static>>,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    if let Some(status) = status {
        let line = Line::from(Span::styled(
            status.to_string(),
            style::fg(palette.accent),
        ));
        frame.render_widget(Paragraph::new(line), rows[0]);
    }

    // With the status row absent, the hints fill the whole footer and so still
    // sit on the bottom rows.
    let hint_area = if status.is_some() { rows[1] } else { area };
    frame.render_widget(Paragraph::new(hints), hint_area);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEMS: &[(&str, &str)] = &[("a", "add"), ("q", "quit")];

    #[test]
    fn fits_on_one_line_when_wide() {
        let result = lines(ITEMS, Color::Default, 80);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn wraps_to_multiple_lines_when_narrow() {
        // "a add" (5) fits; the separator plus "q quit" overflows width 6.
        let result = lines(ITEMS, Color::Default, 6);
        assert_eq!(result.len(), 2);
    }
}
