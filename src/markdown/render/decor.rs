//! How a line is dressed for its container: the base text style, the
//! full-width band colour, and the prefixes (quote bars, list indent, the
//! item marker).

use ratatui::style::Color;
use ratatui::style::Style;

use super::{BlockState, Cell, cells_width};

impl BlockState<'_> {
    /// The base text style for the current container (heading / callout / quote
    /// / body).
    pub(super) fn text_base(&self) -> Style {
        let sheet = self.sheet;
        if let Some(level) = self.heading {
            return sheet.heading_style(level);
        }
        if let Some(kind) = self.callout_kind() {
            let mut style = sheet.base;
            if let Some(fg) = sheet.callout.get(kind).fg {
                style = style.fg(fg);
            }
            return style;
        }
        if !self.quotes.is_empty() {
            let mut style = sheet.base;
            if let Some(fg) = sheet.quote.fg {
                style = style.fg(fg);
            }
            return style;
        }
        sheet.base
    }

    /// The full-width band colour for the current container, if any.
    pub(super) fn fill_bg(&self) -> Option<Color> {
        let sheet = self.sheet;
        if let Some(level) = self.heading {
            return sheet.heading_bg(level);
        }
        if let Some(kind) = self.callout_kind() {
            return sheet.callout.get(kind).bg;
        }
        if !self.quotes.is_empty() {
            return sheet.quote.bg;
        }
        None
    }

    /// The first-line and continuation-line prefixes (quote bars, list indent,
    /// item marker) padded to a common width for alignment.
    pub(super) fn prefixes(&mut self) -> (Vec<Cell>, Vec<Cell>) {
        let mut first: Vec<Cell> = Vec::new();
        for _ in 0..self.quotes.len() {
            first.extend(self.quote_bar());
        }
        let indent = self.lists.len().saturating_sub(1) * 2;
        let indent_style = self.text_base();
        for _ in 0..indent {
            first.push((' ', indent_style));
        }
        let mut cont = first.clone();
        if let Some(marker) = self.pending_marker.take() {
            let marker_w = cells_width(&marker);
            first.extend(marker);
            for _ in 0..marker_w {
                cont.push((' ', indent_style));
            }
        }
        (first, cont)
    }
}
