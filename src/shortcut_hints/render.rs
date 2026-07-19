//! Laying hints out into rows: the flat popup footer and the grouped main-app
//! footer with its aligned label column.
//!
//! Pure layout - every function here maps hints plus a width onto `Line`s.
//! The visibility state that gates the grouped API lives in [`super::state`].

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::{HintGroup, HintStyle, state::visible, style};
use crate::theme::Color;

const SEPARATOR: &str = " \u{00b7} ";

/// Spaces between the aligned label column and the first hint.
const LABEL_GAP: usize = 2;

/// The rows a popup's hint footer of `rows` lines occupies: always `rows`.
///
/// Popup hints are essential key prompts, so they ignore the global F1 toggle
/// (which governs only the main-app footer via the grouped [`height`]/[`render`]
/// API). A popup that does want its footer to follow the toggle reserves
/// `if visible() { footer_height(rows) } else { 0 }` itself.
pub fn footer_height(rows: u16) -> u16 {
    rows
}

/// Wraps `(key, description)` hints into lines at `width` without splitting a
/// token across lines. `key_color` styles the keys (e.g. a dimmed accent).
///
/// The lines are built regardless of the global F1 toggle: a popup's key hints
/// are essential prompts and always show. The toggle governs only the main-app
/// footer, drawn through the grouped [`height`]/[`render`] API. A popup that
/// does want its hints to follow the toggle guards this call with [`visible`].
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
///
/// Yields nothing while the hints are hidden (see [`visible`]).
pub fn group_lines<S: AsRef<str>>(
    groups: &[HintGroup<'_, S>],
    opts: &HintStyle,
    width: usize,
) -> Vec<Line<'static>> {
    if !visible() {
        return Vec::new();
    }
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
/// `top_margin`. At least one row, or `0` once the hints are hidden, so a
/// caller reclaims the margin along with the hints.
pub fn height<S: AsRef<str>>(
    groups: &[HintGroup<'_, S>],
    width: usize,
    top_margin: u16,
) -> u16 {
    if !visible() {
        return 0;
    }
    // The styles do not affect the line count, only the text does.
    let count = group_lines(groups, &HintStyle::default(), width).len() as u16;
    (count + top_margin).max(1)
}

/// Renders the grouped hints into `area`: `opts.top_margin` blank rows, then
/// the aligned hint lines over `opts.background` (if any). Draws nothing at all
/// while the hints are hidden, margin included.
pub fn render<S: AsRef<str>>(
    frame: &mut Frame,
    area: Rect,
    groups: &[HintGroup<'_, S>],
    opts: &HintStyle,
) {
    if !visible() {
        return;
    }
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
