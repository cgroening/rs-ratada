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

/// How much darker the inactive tabs of a locked bar are than their dim.
const LOCK_DARKEN: f32 = 0.25;

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
    draw(frame, area, skin, brand, tabs, active);
}

/// Renders the tab bar while switching tabs is not possible (a form field
/// is being edited): the inactive tabs darker than their usual dim, so the
/// bar reads as disabled, the active tab bold as always.
pub fn render_locked(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    brand: &str,
    tabs: &[(&str, &str)],
    active: usize,
) {
    let mut locked = *skin;
    locked.palette.foreground_dim =
        skin.palette.foreground_dim.darken(LOCK_DARKEN);
    draw(frame, area, &locked, brand, tabs, active);
}

/// Draws the bar with `skin`'s colours.
fn draw(
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

#[cfg(test)]
mod tests {
    use super::*;

    const TABS: &[(&str, &str)] =
        &[("1", "Summary"), ("2", "Tasks"), ("3", "Gallery")];

    /// A way to draw the bar: [`render`] or [`render_locked`].
    type Draw = fn(&mut Frame, Rect, &Skin, &str, &[(&str, &str)], usize);

    /// The foreground of the first cell of `label` in a bar drawn by `draw`.
    fn label_fg(draw: Draw, label: &str) -> Option<ratatui::style::Color> {
        use crate::theme::{
            ColorOverrides, GlyphVariant, Glyphs, Palette, ThemeRegistry,
        };
        use ratatui::{Terminal, backend::TestBackend};
        let base = ThemeRegistry::builtin().resolve("default");
        let skin = Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
        );
        let mut terminal = Terminal::new(TestBackend::new(60, 1)).unwrap();
        terminal
            .draw(|frame| draw(frame, frame.area(), &skin, "app", TABS, 0))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let row: String = (0..60).map(|x| buffer[(x, 0)].symbol()).collect();
        let column = row.find(label)?;
        let x = u16::try_from(row[..column].chars().count()).ok()?;
        buffer[(x, 0)].style().fg
    }

    #[test]
    fn a_locked_bar_draws_the_inactive_tabs_darker() {
        let open = label_fg(render, "Tasks");
        let locked = label_fg(render_locked, "Tasks");
        assert!(open.is_some());
        assert_ne!(open, locked, "the inactive tab must look disabled");
        assert_eq!(
            label_fg(render, "Summary"),
            label_fg(render_locked, "Summary")
        );
    }

    #[test]
    fn everything_fits_on_one_row_when_wide() {
        let rows = pack("app", TABS, 200);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec![0, 1, 2]);
    }

    /// A continuation row is indented under the brand, so the brand width
    /// counts against every row, not only the first.
    #[test]
    fn a_narrow_width_wraps_onto_further_rows() {
        let rows = pack("app", TABS, 30);
        assert!(rows.len() > 1, "expected a wrap, got {rows:?}");
        let flat: Vec<usize> = rows.concat();
        assert_eq!(flat, vec![0, 1, 2], "every tab must appear exactly once");
    }

    /// A tab wider than the row must still get a row of its own rather than
    /// being dropped or looping forever.
    #[test]
    fn a_tab_too_wide_for_the_row_still_gets_one() {
        let rows = pack("brand", &[("1", "an extremely long tab label")], 4);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec![0]);
    }

    #[test]
    fn no_tabs_yields_a_single_empty_row() {
        let rows = pack("app", &[], 80);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_empty());
    }

    /// `height` is what a caller reserves before rendering; it must agree with
    /// the number of rows `pack` actually produces, or the bar is clipped.
    #[test]
    fn the_reported_height_matches_the_packed_rows() {
        for width in [200, 40, 30, 20, 12] {
            let rows = pack("app", TABS, width);
            assert_eq!(
                height("app", TABS, width) as usize,
                rows.len(),
                "width {width}"
            );
        }
    }
}
