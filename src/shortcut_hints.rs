//! Shortcut hints: wrapped `(key, description)` tokens, optionally grouped
//! under aligned labels.
//!
//! [`lines`] lays a flat list of hints into as few rows as fit a width, never
//! splitting a `key desc` token. [`group_lines`] and [`render`] arrange several
//! [`HintGroup`]s one per row with their labels aligned into a left column;
//! a group too wide for one row wraps onto continuation rows indented under
//! that column.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::style;
use crate::theme::Color;

const SEPARATOR: &str = " \u{00b7} ";
/// Spaces between the aligned label column and the first hint.
const LABEL_GAP: usize = 2;

/// A titled group of key hints. An empty `label` renders without a label
/// column, so the group's hints flow like an ungrouped list.
pub struct HintGroup<'a, S: AsRef<str>> {
    pub label: &'a str,
    pub hints: &'a [(S, S)],
}

/// Rendering options for [`render`]: a style for each hint part (group label,
/// key and description), the number of blank rows above the hints, and an
/// optional background fill behind them.
pub struct HintStyle {
    /// Style of a group's label (the `"Name:"` cell).
    pub label: Style,
    /// Style of each shortcut key.
    pub key: Style,
    /// Style of each shortcut description.
    pub description: Style,
    pub top_margin: u16,
    pub background: Option<Color>,
}

impl Default for HintStyle {
    /// Dim labels and descriptions, bold (uncolored) keys, a one-row top margin
    /// and no background.
    fn default() -> Self {
        Self {
            label: style::dim(),
            key: Style::default().add_modifier(Modifier::BOLD),
            description: style::dim(),
            top_margin: 1,
            background: None,
        }
    }
}

/// Wraps `(key, description)` hints into lines at `width` without splitting a
/// token across lines. `key_color` styles the keys (e.g. a dimmed accent).
pub fn lines<S: AsRef<str>>(
    items: &[(S, S)],
    key_color: Color,
    width: usize,
) -> Vec<Line<'static>> {
    let key_style = style::fg(key_color).add_modifier(Modifier::BOLD);
    wrap(items, key_style, style::dim(), width)
        .into_iter()
        .map(Line::from)
        .collect()
}

/// Lays out `groups` one per row with their labels aligned into a left column,
/// wrapping a group that overflows onto continuation rows indented under the
/// column. Groups without a label (or all groups, if none has one) flow flat.
pub fn group_lines<S: AsRef<str>>(
    groups: &[HintGroup<'_, S>],
    opts: &HintStyle,
    width: usize,
) -> Vec<Line<'static>> {
    let label_col = label_column_width(groups);
    let hint_width = width.saturating_sub(label_col).max(1);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for group in groups {
        let rows = wrap(group.hints, opts.key, opts.description, hint_width);
        for (row_index, mut hint_spans) in rows.into_iter().enumerate() {
            let mut spans: Vec<Span<'static>> = Vec::new();
            if label_col > 0 {
                let is_label_row = row_index == 0 && !group.label.is_empty();
                let cell = if is_label_row {
                    pad(&format!("{}:", group.label), label_col)
                } else {
                    " ".repeat(label_col)
                };
                let cell_style = if is_label_row {
                    opts.label
                } else {
                    Style::default()
                };
                spans.push(Span::styled(cell, cell_style));
            }
            spans.append(&mut hint_spans);
            lines.push(Line::from(spans));
        }
    }
    lines
}

/// The number of rows the grouped hints occupy at `width`, including the
/// `top_margin`. At least one row.
pub fn height<S: AsRef<str>>(
    groups: &[HintGroup<'_, S>],
    width: usize,
    top_margin: u16,
) -> u16 {
    // The styles do not affect the line count, only the text does.
    let count = group_lines(groups, &HintStyle::default(), width).len() as u16;
    (count + top_margin).max(1)
}

/// Renders the grouped hints into `area`: `opts.top_margin` blank rows, then
/// the aligned hint lines over `opts.background` (if any).
pub fn render<S: AsRef<str>>(
    frame: &mut Frame,
    area: Rect,
    groups: &[HintGroup<'_, S>],
    opts: &HintStyle,
) {
    let width = area.width as usize;
    let lines = group_lines(groups, opts, width);
    let margin = opts.top_margin.min(area.height);
    let hint_area = Rect {
        x: area.x,
        y: area.y + margin,
        width: area.width,
        height: area.height.saturating_sub(margin),
    };
    let mut paragraph = Paragraph::new(lines);
    if let Some(bg) = opts.background {
        paragraph = paragraph.style(style::bg(bg));
    }
    frame.render_widget(paragraph, hint_area);
}

/// Wraps `(key, description)` tokens into rows no wider than `width`, never
/// splitting a token. Each returned row holds the spans for one line.
fn wrap<S: AsRef<str>>(
    items: &[(S, S)],
    key_style: Style,
    desc_style: Style,
    width: usize,
) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<Span<'static>>> = Vec::new();
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
            rows.push(std::mem::take(&mut spans));
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
        rows.push(spans);
    }
    rows
}

/// The width of the aligned label column (widest `"label:"` plus [`LABEL_GAP`]),
/// or `0` when no group has a label.
fn label_column_width<S: AsRef<str>>(groups: &[HintGroup<'_, S>]) -> usize {
    let widest = groups
        .iter()
        .filter(|group| !group.label.is_empty())
        .map(|group| group.label.width() + 1) // + the ':'
        .max()
        .unwrap_or(0);
    if widest == 0 { 0 } else { widest + LABEL_GAP }
}

/// Right-pads `text` with spaces to `width` (no-op if already at least `width`).
fn pad(text: &str, width: usize) -> String {
    let current = text.width();
    if current >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - current))
    }
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

    #[test]
    fn group_labels_align_into_one_column() {
        let nav = [("a", "up")];
        let commands = [("b", "help")];
        let groups = [
            HintGroup {
                label: "Nav",
                hints: &nav,
            },
            HintGroup {
                label: "Commands",
                hints: &commands,
            },
        ];
        let lines = group_lines(&groups, &HintStyle::default(), 80);
        // "Commands" (8) + ':' + gap(2) = 11 columns for both label cells.
        assert_eq!(lines[0].spans[0].content.len(), 11);
        assert_eq!(lines[1].spans[0].content.len(), 11);
        assert!(lines[0].spans[0].content.starts_with("Nav:"));
        assert!(lines[1].spans[0].content.starts_with("Commands:"));
    }

    #[test]
    fn overflowing_group_wraps_indented_under_the_label() {
        let hints = [("aaa", "bbb"), ("ccc", "ddd")];
        let groups = [HintGroup {
            label: "G",
            hints: &hints,
        }];
        // label_col = "G:".len() + gap = 4; hint budget 8 - 4 = 4 forces a wrap.
        let lines = group_lines(&groups, &HintStyle::default(), 8);
        assert_eq!(lines.len(), 2);
        // The continuation row starts with the empty, label-width indent.
        let indent = &lines[1].spans[0].content;
        assert_eq!(indent.len(), 4);
        assert!(indent.trim().is_empty());
    }

    #[test]
    fn labelless_groups_render_flat() {
        let hints = [("a", "add")];
        let groups = [HintGroup {
            label: "",
            hints: &hints,
        }];
        let lines = group_lines(&groups, &HintStyle::default(), 80);
        // No label column: the first span is the key token itself.
        assert_eq!(lines[0].spans[0].content, "a ");
    }

    #[test]
    fn height_counts_lines_plus_margin() {
        let a = [("a", "x")];
        let b = [("b", "y")];
        let groups = [
            HintGroup {
                label: "A",
                hints: &a,
            },
            HintGroup {
                label: "B",
                hints: &b,
            },
        ];
        assert_eq!(height(&groups, 80, 1), 3);
        assert_eq!(height(&groups, 80, 0), 2);
    }
}
