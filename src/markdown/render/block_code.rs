//! Fenced and indented code blocks: a band of hard-wrapped lines, led by
//! the fence's language label.

use ratatui::style::Style;

use super::{BlockState, Cell, inline::styled_cells, source, wrap::hard_wrap};

impl BlockState<'_> {
    /// Flushes the collected code block as a banded set of hard-wrapped lines,
    /// prefixed by the fenced language label when present. Each line keeps
    /// where its text starts in the source.
    pub(super) fn flush_code(&mut self) {
        let Some(code) = self.code.take() else {
            return;
        };
        let from = self.code_from.take().unwrap_or(self.at);
        let style = self.sheet.code_block.text_style();
        let bg = self.sheet.code_block.bg;
        if let Some(lang) = self.code_lang.take() {
            let mut title = Style::default();
            if let Some(fg) = self.sheet.code_block.title_fg {
                title = title.fg(fg);
            }
            let cells = styled_cells(&lang, title);
            self.push_decorated(&[], &cells, bg, None);
        }
        let body = code.strip_suffix('\n').unwrap_or(&code);
        let sources = source::byte_offsets(body.chars(), self.src, from);
        let source_of = |index: usize| {
            sources
                .get(index)
                .or(sources.last())
                .copied()
                .unwrap_or(from)
        };
        let mut start = 0;
        for raw in body.split('\n') {
            let cells: Vec<Cell> = raw.chars().map(|ch| (ch, style)).collect();
            let mut chunk_start = start;
            for chunk in hard_wrap(&cells, self.width) {
                let source = Some(source_of(chunk_start));
                chunk_start += chunk.len();
                self.push_decorated(&[], &chunk, bg, source);
            }
            start += cells.len() + 1;
        }
        if self.at_top_level() {
            self.produced = true;
        }
    }
}
