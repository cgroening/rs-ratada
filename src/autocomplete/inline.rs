//! The inline rendering: rows a host appends to its own text.
//!
//! The rows push whatever follows them down, so this suits a field that is the
//! last thing on screen or a host that reserves the room. A field inside a form
//! wants [`Autocomplete::render_popup`] instead, which floats over the view.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::{Autocomplete, MAX_VISIBLE};
use crate::style;
use crate::theme::Palette;

impl Autocomplete {
    /// The visible suggestion rows, each indented by `indent` columns, with the
    /// highlighted row in the selection style and a closing `+N more` row for
    /// the matches outside the [`MAX_VISIBLE`] window. Empty when closed.
    pub fn lines(
        &self,
        palette: &Palette,
        indent: usize,
        base: Style,
    ) -> Vec<Line<'static>> {
        if !self.is_open() {
            return Vec::new();
        }
        let pad = " ".repeat(indent);
        let offset = self.window_offset(MAX_VISIBLE);
        let shown = self
            .matches
            .iter()
            .enumerate()
            .skip(offset)
            .take(MAX_VISIBLE);
        let mut lines: Vec<Line<'static>> = shown
            .map(|(row, hit)| {
                let row_style = if Some(row) == self.selected {
                    style::bg(palette.selection)
                } else {
                    base
                };
                Line::from(vec![
                    Span::raw(pad.clone()),
                    Span::styled(self.candidates[hit.index].clone(), row_style),
                ])
            })
            .collect();
        let hidden = self.matches.len().saturating_sub(MAX_VISIBLE);
        if hidden > 0 {
            lines.push(Line::from(vec![
                Span::raw(pad),
                Span::styled(
                    format!("+{hidden} more"),
                    style::secondary(palette),
                ),
            ]));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::theme::{ColorOverrides, ThemeRegistry};

    fn palette() -> Palette {
        Palette::resolve(
            ThemeRegistry::builtin().resolve("default"),
            &ColorOverrides::default(),
        )
    }

    fn text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn closed_suggestions_render_nothing() {
        let ac = Autocomplete::new(vec!["Edeka".into()]);
        assert!(ac.lines(&palette(), 2, Style::default()).is_empty());
    }

    #[test]
    fn rows_are_indented() {
        let mut ac = Autocomplete::new(vec!["Edeka".into()]);
        ac.refresh("ed");
        let lines = ac.lines(&palette(), 3, Style::default());
        assert_eq!(text(&lines[0]), "   Edeka");
    }

    #[test]
    fn overflow_row_counts_the_hidden_matches() {
        let many = (0..11).map(|n| format!("Store {n}")).collect();
        let mut ac = Autocomplete::new(many);
        ac.refresh("store");
        let lines = ac.lines(&palette(), 0, Style::default());
        assert_eq!(lines.len(), MAX_VISIBLE + 1);
        assert_eq!(text(&lines[MAX_VISIBLE]), "+3 more");
    }

    #[test]
    fn window_follows_the_selection() {
        let many = (0..11).map(|n| format!("Store {n}")).collect();
        let mut ac = Autocomplete::new(many);
        ac.refresh("store");
        ac.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)); // last
        let lines = ac.lines(&palette(), 0, Style::default());
        assert_eq!(text(&lines[MAX_VISIBLE - 1]), "Store 10");
    }
}
