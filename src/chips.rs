//! Chips: a short figure set off from its line by a background tint, like
//! ` Value 3.463,72 ` or a week's ` ● 38,5 h `. A chip is one blank, its
//! text and one blank, all on the tint; the text keeps its own colours, so a
//! chip can mix a dim label with a green or red figure.
//!
//! A [`Tile`] is the two-line form for a dashboard row: the label on top,
//! the value below, both on the tint; [`tiles`] lays them out and wraps them
//! onto further pairs of lines when the width runs out.
//!
//! The toolkit has no chip colour of its own: the host passes the tint from
//! its theme (a style carrying the background), so every app keeps its look.

use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

/// The blank around a chip's text and between two chips of a [`row`].
const PAD: &str = " ";

/// A chip of one text: ` text ` in `fg` on the background of `bg`.
pub fn chip(text: &str, fg: Style, bg: Style) -> Span<'static> {
    Span::styled(format!("{PAD}{text}{PAD}"), bg.patch(fg))
}

/// A chip of several spans, each keeping its own colours, on the background
/// of `bg`, with one blank on each side.
pub fn chip_spans(spans: Vec<Span<'static>>, bg: Style) -> Vec<Span<'static>> {
    let mut chip = Vec::with_capacity(spans.len() + 2);
    chip.push(Span::styled(PAD, bg));
    chip.extend(
        spans
            .into_iter()
            .map(|span| Span::styled(span.content, bg.patch(span.style))),
    );
    chip.push(Span::styled(PAD, bg));
    chip
}

/// The chips one after another, one plain blank between two.
pub fn row(chips: Vec<Vec<Span<'static>>>) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, chip) in chips.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(PAD));
        }
        spans.extend(chip);
    }
    Line::from(spans)
}

/// A figure over two lines: its label on top, its value below.
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    /// The label (drawn in the label style of [`tiles`]).
    pub label: String,
    /// The value in its own style (bold, a colour by sign, ...).
    pub value: Span<'static>,
}

impl Tile {
    /// A tile of `label` and `value`.
    pub fn new(label: impl Into<String>, value: Span<'static>) -> Self {
        Tile {
            label: label.into(),
            value,
        }
    }

    /// The columns the tile takes: its wider line and a blank each side.
    fn width(&self) -> usize {
        let text = self.label.width().max(self.value.content.width());
        text + 2 * PAD.width()
    }
}

/// `tiles` laid out in `width` columns: per row of tiles a label line and a
/// value line on the tint `bg`, the labels in `label`, one plain blank
/// between two tiles; a tile that no longer fits starts the next row. A tile
/// wider than `width` gets a row of its own (and is cut by the frame).
pub fn tiles(
    tiles: &[Tile],
    label: Style,
    bg: Style,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for row in rows(tiles, width) {
        let mut labels = Vec::new();
        let mut values = Vec::new();
        for (index, tile) in row.iter().enumerate() {
            if index > 0 {
                labels.push(Span::raw(PAD));
                values.push(Span::raw(PAD));
            }
            let inner = tile.width() - 2 * PAD.width();
            labels.push(Span::styled(
                padded(&tile.label, inner),
                bg.patch(label),
            ));
            values.push(Span::styled(
                padded(&tile.value.content, inner),
                bg.patch(tile.value.style),
            ));
        }
        lines.push(Line::from(labels));
        lines.push(Line::from(values));
    }
    lines
}

/// The lines [`tiles`] takes for `tiles` in `width` columns.
pub fn tiles_height(tiles: &[Tile], width: u16) -> u16 {
    let rows = rows(tiles, width).len();
    u16::try_from(rows * 2).unwrap_or(u16::MAX)
}

/// `tiles` split into the rows that fit `width` side by side.
fn rows(tiles: &[Tile], width: u16) -> Vec<&[Tile]> {
    let width = usize::from(width);
    let mut rows = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (index, tile) in tiles.iter().enumerate() {
        let gap = if index > start { PAD.width() } else { 0 };
        if index > start && used + gap + tile.width() > width {
            rows.push(&tiles[start..index]);
            start = index;
            used = tile.width();
        } else {
            used += gap + tile.width();
        }
    }
    if start < tiles.len() {
        rows.push(&tiles[start..]);
    }
    rows
}

/// ` text ` filled with blanks to `inner` columns of text.
fn padded(text: &str, inner: usize) -> String {
    let fill = inner.saturating_sub(text.width());
    format!("{PAD}{text}{}{PAD}", " ".repeat(fill))
}

#[cfg(test)]
mod tests {
    use ratatui::style::{Color, Modifier};

    use super::*;

    fn tint() -> Style {
        Style::default().bg(Color::Rgb(38, 44, 60))
    }

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn a_chip_pads_its_text_on_the_tint() {
        let fg = Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let chip = chip("+9,54 %", fg, tint());
        assert_eq!(chip.content, " +9,54 % ");
        assert_eq!(chip.style.fg, Some(Color::Green));
        assert_eq!(chip.style.bg, tint().bg);
        assert!(chip.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn a_chip_of_spans_keeps_their_colours_on_one_tint() {
        let spans = vec![
            Span::styled("Gain ", Style::default().fg(Color::Gray)),
            Span::styled("+301,66", Style::default().fg(Color::Green)),
        ];
        let chip = chip_spans(spans, tint());
        let shown: String = chip.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(shown, " Gain +301,66 ");
        assert!(chip.iter().all(|s| s.style.bg == tint().bg));
        assert_eq!(chip[1].style.fg, Some(Color::Gray));
        assert_eq!(chip[2].style.fg, Some(Color::Green));
    }

    #[test]
    fn a_row_leaves_one_plain_blank_between_chips() {
        let row = row(vec![
            vec![chip("a", Style::default(), tint())],
            vec![chip("b", Style::default(), tint())],
        ]);
        assert_eq!(text(&row), " a   b ");
        assert_eq!(row.spans[1].style.bg, None, "the gap is not tinted");
        assert_eq!(row.width(), 7);
    }

    fn tile(label: &str, value: &str) -> Tile {
        Tile::new(label, Span::styled(value.to_string(), Style::default()))
    }

    fn texts(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(text).collect()
    }

    #[test]
    fn a_tile_puts_its_label_over_its_value() {
        let lines = tiles(
            &[tile("Value", "3.463,72 €"), tile("p.a.", "23,3 %")],
            Style::default(),
            tint(),
            80,
        );
        assert_eq!(
            texts(&lines),
            [" Value        p.a.   ", " 3.463,72 €   23,3 % "]
        );
        assert_eq!(lines[0].spans[1].style.bg, None, "the gap is plain");
        assert_eq!(lines[1].spans[0].style.bg, tint().bg);
    }

    #[test]
    fn tiles_wrap_onto_another_pair_of_lines() {
        let all = [tile("Value", "3.463,72"), tile("Cost", "3.162,06")];
        // Each tile is 10 wide: two need 21 columns.
        assert_eq!(tiles_height(&all, 21), 2);
        assert_eq!(tiles_height(&all, 20), 4);
        let lines = tiles(&all, Style::default(), tint(), 20);
        assert_eq!(texts(&lines)[2], " Cost     ");
    }

    #[test]
    fn no_tiles_take_no_lines() {
        assert_eq!(tiles_height(&[], 40), 0);
        assert!(tiles(&[], Style::default(), tint(), 40).is_empty());
    }
}
