//! A wrapping, segment-aware top tab bar: a brand plus numbered view tabs.
//!
//! Tabs are packed onto as few rows as fit `width`; overflow wraps to the next
//! row, indented under the brand. The active tab (its number and label) stands
//! out in the normal text color and bold; the rest are dim.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::style;
use crate::theme::Skin;

const SEPARATOR: &str = " \u{2502} ";

fn brand_label(brand: &str) -> String {
    format!(" {brand}   ")
}

fn tab_label(key: &str, label: &str) -> String {
    format!("{key} {label}")
}

/// Number of rows the tab bar needs at `width`.
pub fn height(brand: &str, tabs: &[(&str, &str)], width: usize) -> u16 {
    pack(brand, tabs, width).len().max(1) as u16
}

/// Renders the tab bar into `area`, wrapping tabs across rows when needed and
/// highlighting the `active` tab.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    brand: &str,
    tabs: &[(&str, &str)],
    active: usize,
) {
    let palette = &skin.palette;
    let brand_width = brand_label(brand).width();
    let rows = pack(brand, tabs, area.width as usize);

    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(row_index, segments)| {
            let mut spans: Vec<Span> = Vec::new();
            if row_index == 0 {
                spans.push(Span::styled(
                    brand_label(brand),
                    style::fg(palette.accent).add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::raw(" ".repeat(brand_width)));
            }
            for (position, &index) in segments.iter().enumerate() {
                if position > 0 {
                    spans.push(Span::styled(
                        SEPARATOR,
                        style::secondary(palette),
                    ));
                }
                let (key, label) = tabs[index];
                // The active tab's number and label share one style: the normal
                // text color in bold; inactive tabs are wholly dimmed.
                let tab_style = if index == active {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    style::secondary(palette)
                };
                spans.push(Span::styled(format!("{key} "), tab_style));
                spans.push(Span::styled(label.to_string(), tab_style));
            }
            Line::from(spans)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

/// Packs tab indices onto rows that each fit `width`. The first tab on a row is
/// always placed (never split); the brand occupies the start of row 0 and the
/// continuation rows are indented by the same width.
fn pack(brand: &str, tabs: &[(&str, &str)], width: usize) -> Vec<Vec<usize>> {
    let brand_width = brand_label(brand).width();
    let separator_width = SEPARATOR.width();
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut used = brand_width;

    for (index, (key, label)) in tabs.iter().enumerate() {
        let segment_width = tab_label(key, label).width();
        if !current.is_empty() && used + separator_width + segment_width > width
        {
            rows.push(std::mem::take(&mut current));
            used = brand_width;
        }
        if !current.is_empty() {
            used += separator_width;
        }
        current.push(index);
        used += segment_width;
    }
    if !current.is_empty() {
        rows.push(current);
    }
    if rows.is_empty() {
        rows.push(Vec::new());
    }
    rows
}
