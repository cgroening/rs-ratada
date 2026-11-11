//! Blockquotes and GFM callouts: the quote bar and the callout's title line.

use pulldown_cmark::BlockQuoteKind;
use ratatui::style::Modifier;
use ratatui::style::Style;

use super::super::callout_label;
use super::{BlockState, Cell, inline::styled_cells};

impl BlockState<'_> {
    /// One quote-bar prefix cell pair (`▎ `): in a callout it uses the callout
    /// colour, else the configured quote bar colour.
    pub(super) fn quote_bar(&self) -> Vec<Cell> {
        let sheet = self.sheet;
        let mut style = Style::default();
        let fg = match self.callout_kind() {
            Some(kind) => sheet.callout.get(kind).fg,
            None => sheet.quote.bar_fg,
        };
        if let Some(fg) = fg {
            style = style.fg(fg);
        }
        let mut cells = styled_cells(&sheet.quote.bar, style);
        cells.push((' ', style));
        cells
    }

    /// Emits a callout's coloured title line (e.g. `NOTE`) above its body.
    pub(super) fn emit_callout_title(&mut self, kind: BlockQuoteKind) {
        let sheet = self.sheet;
        let callout = sheet.callout.get(kind);
        let bg = callout.bg;
        let mut style = Style::default().add_modifier(Modifier::BOLD);
        if let Some(fg) = callout.fg {
            style = style.fg(fg);
        }
        let content = styled_cells(callout_label(kind), style);
        let mut prefix: Vec<Cell> = Vec::new();
        for _ in 0..self.quotes.len() {
            prefix.extend(self.quote_bar());
        }
        self.push_decorated(&prefix, &content, bg, None);
    }
}
