//! A wrapping, segment-aware top tab bar: a brand plus numbered view tabs.
//!
//! Tabs are packed onto as few rows as fit `width`; overflow wraps to the next
//! row, indented under the brand. The active tab is accented; the rest are dim.
//! In `Fancy` mode the bar is wrapped in a rounded, accent-bordered box; in
//! `Minimal` mode it is a single plain row.

use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::style;
use crate::theme::Skin;

const SEPARATOR: &str = " \u{2502} ";

fn brand_label(brand: &str) -> String {
    format!(" {brand}  ")
}

fn tab_label(key: &str, label: &str) -> String {
    format!("{key} {label}")
}

/// Number of rows the tab bar needs at `width`, including the border in `Fancy`
/// mode.
pub fn height(
    skin: &Skin,
    brand: &str,
    tabs: &[(&str, &str)],
    width: usize,
) -> u16 {
    let inner = inner_width(skin, width);
    let rows = pack(brand, tabs, inner).len().max(1) as u16;
    rows + frame_rows(skin)
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
    let rows = pack(brand, tabs, inner_width(skin, area.width as usize));

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
                    spans.push(Span::styled(SEPARATOR, style::dim()));
                }
                let (key, label) = tabs[index];
                let label_style = if index == active {
                    style::fg(palette.accent).add_modifier(Modifier::BOLD)
                } else {
                    style::dim()
                };
                spans.push(Span::styled(format!("{key} "), style::dim()));
                spans.push(Span::styled(label.to_string(), label_style));
            }
            Line::from(spans)
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    if skin.is_fancy() {
        frame.render_widget(paragraph.block(fancy_block(skin)), area);
    } else {
        frame.render_widget(paragraph, area);
    }
}

/// The rounded accent box used to frame the bar in `Fancy` mode.
fn fancy_block(skin: &Skin) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style::fg(skin.palette.accent))
        .padding(Padding::horizontal(1))
}

/// Extra rows the frame occupies: two (top/bottom border) in `Fancy`, none in
/// `Minimal`.
fn frame_rows(skin: &Skin) -> u16 {
    if skin.is_fancy() { 2 } else { 0 }
}

/// The width available to the packed tabs after the frame/padding.
fn inner_width(skin: &Skin, width: usize) -> usize {
    if skin.is_fancy() {
        // 2 border columns + 2 padding columns.
        width.saturating_sub(4)
    } else {
        width
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{
        ColorOverrides, GlyphVariant, Glyphs, Mode, Palette, ThemeRegistry,
    };

    fn skin(mode: Mode) -> Skin {
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
            mode,
        )
    }

    #[test]
    fn fancy_height_adds_the_border_rows() {
        let tabs = [("1", "One"), ("2", "Two")];
        let width = 80;
        let minimal = height(&skin(Mode::Minimal), "demo", &tabs, width);
        let fancy = height(&skin(Mode::Fancy), "demo", &tabs, width);
        assert_eq!(fancy, minimal + 2);
    }
}
