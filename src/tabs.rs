//! A wrapping, segment-aware top tab bar: a brand plus numbered view tabs.
//!
//! Tabs are packed onto as few rows as fit `width`; overflow wraps to the next
//! row, indented under the brand. The active tab (its number and label) stands
//! out in the normal text color and bold; the rest are dim.
//!
//! Every tab is separated from the next one on its row by a dim `│`.
//! [`render_grouped`] takes the tabs in groups: a row then only wraps between
//! two groups, unless a single group is wider than the row. [`render_sub`]
//! draws a second-level bar for the sub-views of the active tab, indented
//! under the first tab, its items [`SUB_GAP`] apart.

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

/// Gap between two items of the sub-view bar.
pub const SUB_GAP: &str = "  ";

/// One tab placed on a row: its index into the flat tab list and the gap
/// drawn before it (empty for the first tab of a row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placed {
    index: usize,
    lead: &'static str,
}

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
    draw(frame, area, &locked_skin(skin), brand, tabs, active);
}

/// Number of rows a grouped tab bar needs at `width`.
pub fn height_grouped(
    brand: &str,
    groups: &[&[(&str, &str)]],
    width: usize,
) -> u16 {
    pack_grouped(brand_label(brand).width(), groups, SEPARATOR, width)
        .len()
        .max(1) as u16
}

/// Renders a grouped tab bar into `area`, highlighting the `active` tab.
///
/// `groups` lists the tabs group by group; `active` indexes the tabs in that
/// order as one flat list. A dim `│` separates the tabs on a row; a row wraps
/// between groups, and inside a group only when the group alone is wider
/// than the row.
pub fn render_grouped(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    brand: &str,
    groups: &[&[(&str, &str)]],
    active: usize,
) {
    draw_grouped(frame, area, skin, brand, groups, active);
}

/// Renders a grouped tab bar while switching tabs is not possible, with the
/// inactive tabs darker than their usual dim (see [`render_locked`]).
pub fn render_grouped_locked(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    brand: &str,
    groups: &[&[(&str, &str)]],
    active: usize,
) {
    draw_grouped(frame, area, &locked_skin(skin), brand, groups, active);
}

/// Number of rows the sub-view bar needs at `width` under a bar with `brand`.
pub fn sub_height(brand: &str, items: &[(&str, &str)], width: usize) -> u16 {
    pack_grouped(brand_label(brand).width(), &[items], SUB_GAP, width)
        .len()
        .max(1) as u16
}

/// Renders the sub-view bar of the active tab into `area`: the `items`
/// (key, label) indented under the first tab of the bar above (`brand` sets
/// the indent), [`SUB_GAP`] apart, the `active` one bold in the accent
/// and the rest dim.
pub fn render_sub(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    brand: &str,
    items: &[(&str, &str)],
    active: usize,
) {
    let indent = brand_label(brand).width();
    let rows = pack_grouped(indent, &[items], SUB_GAP, area.width as usize);
    let active_style =
        style::fg(skin.palette.accent).add_modifier(Modifier::BOLD);
    let lines = rows
        .iter()
        .map(|row| {
            let mut spans = vec![Span::raw(" ".repeat(indent))];
            spans.extend(tab_spans(skin, items, row, active, active_style));
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

/// Draws a grouped bar with `skin`'s colours.
fn draw_grouped(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    brand: &str,
    groups: &[&[(&str, &str)]],
    active: usize,
) {
    let tabs: Vec<(&str, &str)> = groups
        .iter()
        .flat_map(|group| group.iter().copied())
        .collect();
    let rows = pack_grouped(
        brand_label(brand).width(),
        groups,
        SEPARATOR,
        area.width as usize,
    );
    draw_rows(frame, area, skin, brand, &tabs, &rows, active);
}

/// A copy of `skin` whose dim is darkened, for a locked bar.
fn locked_skin(skin: &Skin) -> Skin {
    let mut locked = *skin;
    locked.palette.foreground_dim =
        skin.palette.foreground_dim.darken(LOCK_DARKEN);
    locked
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
    let rows: Vec<Vec<Placed>> = pack(brand, tabs, area.width as usize)
        .into_iter()
        .map(|row| {
            row.into_iter()
                .enumerate()
                .map(|(position, index)| Placed {
                    index,
                    lead: if position == 0 { "" } else { SEPARATOR },
                })
                .collect()
        })
        .collect();
    draw_rows(frame, area, skin, brand, tabs, &rows, active);
}

/// Draws packed `rows` of `tabs`: the brand on the first row, continuation
/// rows indented under it, each tab after its gap.
fn draw_rows(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    brand: &str,
    tabs: &[(&str, &str)],
    rows: &[Vec<Placed>],
    active: usize,
) {
    let palette = &skin.palette;
    let brand_width = brand_label(brand).width();
    // The active tab's number and label share one style: the normal text
    // color in bold; inactive tabs are wholly dimmed.
    let active_style = Style::default().add_modifier(Modifier::BOLD);

    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let mut spans: Vec<Span> = Vec::new();
            if row_index == 0 {
                spans.push(Span::styled(
                    brand_label(brand),
                    style::fg(palette.accent).add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::raw(" ".repeat(brand_width)));
            }
            spans.extend(tab_spans(skin, tabs, row, active, active_style));
            Line::from(spans)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

/// The spans of one packed row: each tab's gap (dim) and its number and
/// label, in `active_style` for the `active` tab and dim otherwise.
fn tab_spans<'a>(
    skin: &Skin,
    tabs: &[(&'a str, &'a str)],
    row: &[Placed],
    active: usize,
    active_style: Style,
) -> Vec<Span<'a>> {
    let dim = style::secondary(&skin.palette);
    let mut spans = Vec::new();
    for placed in row {
        if !placed.lead.is_empty() {
            spans.push(Span::styled(placed.lead, dim));
        }
        let (key, label) = tabs[placed.index];
        let tab_style = if placed.index == active {
            active_style
        } else {
            dim
        };
        spans.push(Span::styled(format!("{key} "), tab_style));
        spans.push(Span::styled(label, tab_style));
    }
    spans
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

/// Packs grouped tabs onto rows that each fit `width`, after an indent of
/// `indent` columns on every row, `gap` between two tabs of a row. A group moves to the next row as a whole
/// when it does not fit behind the previous one; only a group wider than a
/// row of its own wraps between its tabs. A tab is never split.
fn pack_grouped(
    indent: usize,
    groups: &[&[(&str, &str)]],
    gap: &'static str,
    width: usize,
) -> Vec<Vec<Placed>> {
    let mut rows: Vec<Vec<Placed>> = Vec::new();
    let mut current: Vec<Placed> = Vec::new();
    let mut used = indent;
    let mut first_index = 0;

    for group in groups {
        let widths: Vec<usize> = group
            .iter()
            .map(|(key, label)| tab_label(key, label).width())
            .collect();
        let group_width = widths.iter().sum::<usize>()
            + gap.width() * widths.len().saturating_sub(1);
        if !current.is_empty() && used + gap.width() + group_width > width {
            rows.push(std::mem::take(&mut current));
            used = indent;
        }
        for (offset, tab_width) in widths.iter().enumerate() {
            let mut lead = if current.is_empty() { "" } else { gap };
            if !current.is_empty() && used + lead.width() + tab_width > width {
                rows.push(std::mem::take(&mut current));
                used = indent;
                lead = "";
            }
            current.push(Placed {
                index: first_index + offset,
                lead,
            });
            used += lead.width() + tab_width;
        }
        first_index += group.len();
    }
    if !current.is_empty() || rows.is_empty() {
        rows.push(current);
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

    const GROUPS: &[&[(&str, &str)]] =
        &[&[("1", "A"), ("2", "B")], &[("3", "C")], &[("0", "D")]];

    /// The default skin used by the rendering tests.
    fn test_skin() -> Skin {
        use crate::theme::{
            ColorOverrides, GlyphVariant, Glyphs, Palette, ThemeRegistry,
        };
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
        )
    }

    /// The text rows of `draw` rendered into a `width`×`height` buffer, with
    /// trailing blanks trimmed.
    fn rendered_rows(
        width: u16,
        height: u16,
        draw: impl FnOnce(&mut Frame),
    ) -> Vec<String> {
        use ratatui::{Terminal, backend::TestBackend};
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(draw).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                let row: String =
                    (0..width).map(|x| buffer[(x, y)].symbol()).collect();
                row.trim_end().to_string()
            })
            .collect()
    }

    #[test]
    fn a_grouped_bar_divides_every_tab_on_a_row() {
        let skin = test_skin();
        let rows = rendered_rows(60, 1, |frame| {
            render_grouped(frame, frame.area(), &skin, "app", GROUPS, 0);
        });
        assert_eq!(rows[0], " app   1 A │ 2 B │ 3 C │ 0 D");
    }

    #[test]
    fn a_grouped_bar_wraps_between_groups() {
        // "1 A │ 2 B" fits behind the brand, "3 C" does not fit behind it.
        let rows = pack_grouped(6, GROUPS, SEPARATOR, 20);
        let indices: Vec<Vec<usize>> = rows
            .iter()
            .map(|row| row.iter().map(|placed| placed.index).collect())
            .collect();
        assert_eq!(indices, vec![vec![0, 1], vec![2, 3]]);
        assert_eq!(rows[1][0].lead, "", "a row never starts with a gap");
        assert_eq!(rows[1][1].lead, SEPARATOR);
    }

    /// A group wider than a row on its own still wraps between its tabs
    /// instead of overflowing.
    #[test]
    fn a_group_wider_than_the_row_wraps_inside() {
        let wide: &[&[(&str, &str)]] =
            &[&[("1", "Alpha"), ("2", "Beta"), ("3", "Gamma")]];
        let rows = pack_grouped(6, wide, SEPARATOR, 18);
        let flat: Vec<usize> = rows.concat().iter().map(|p| p.index).collect();
        assert!(rows.len() > 1, "expected a wrap, got {rows:?}");
        assert_eq!(flat, vec![0, 1, 2], "every tab must appear exactly once");
    }

    #[test]
    fn the_grouped_height_matches_the_packed_rows() {
        for width in [80, 30, 20, 12] {
            let rows = pack_grouped(
                brand_label("app").width(),
                GROUPS,
                SEPARATOR,
                width,
            );
            assert_eq!(
                height_grouped("app", GROUPS, width) as usize,
                rows.len(),
                "width {width}"
            );
        }
    }

    #[test]
    fn a_locked_grouped_bar_draws_the_inactive_tabs_darker() {
        let skin = test_skin();
        let fg = |locked: bool| {
            use ratatui::{Terminal, backend::TestBackend};
            let mut terminal = Terminal::new(TestBackend::new(40, 1)).unwrap();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    if locked {
                        render_grouped_locked(
                            frame, area, &skin, "app", GROUPS, 0,
                        );
                    } else {
                        render_grouped(frame, area, &skin, "app", GROUPS, 0);
                    }
                })
                .unwrap();
            // "3 C" starts at column 19 (" app   1 A │ 2 B │ ").
            terminal.backend().buffer()[(19, 0)].style().fg
        };
        assert_ne!(fg(false), fg(true), "the inactive tab must look disabled");
    }

    #[test]
    fn the_sub_bar_is_indented_under_the_first_tab() {
        let skin = test_skin();
        let items = &[("Alt+1", "Readings"), ("Alt+2", "Months")];
        let rows = rendered_rows(60, 1, |frame| {
            render_sub(frame, frame.area(), &skin, "app", items, 1);
        });
        assert_eq!(rows[0], "       Alt+1 Readings  Alt+2 Months");
        assert_eq!(sub_height("app", items, 60), 1);
        assert_eq!(sub_height("app", items, 20), 2);
    }
}
