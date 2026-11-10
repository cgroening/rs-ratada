//! Chips: a short figure set off from its line by a background tint, like
//! ` Value 3.463,72 ` or a week's ` ● 38,5 h `. A chip is one blank, its
//! text and one blank, all on the tint; the text keeps its own colours, so a
//! chip can mix a dim label with a green or red figure.
//!
//! The toolkit has no chip colour of its own: the host passes the tint from
//! its theme (a style carrying the background), so every app keeps its look.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

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
}
