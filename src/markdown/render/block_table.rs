//! GFM table state: collecting rows and cells, then emitting the aligned
//! grid with its separator.

use pulldown_cmark::{Alignment, Event};
use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::{Cell, cells_width};

use super::{
    BlockState,
    inline::inline_to_cells,
    wrap::{align_cell, cells_to_spans},
};

/// A GFM table being accumulated: column alignments, the collected rows (each a
/// list of styled cells), and how many leading rows are the header.
pub(super) struct TableState {
    pub(super) aligns: Vec<Alignment>,
    pub(super) rows: Vec<Vec<Vec<Cell>>>,
    pub(super) head_rows: usize,
    pub(super) in_head: bool,
}

/// One rendered table row: each cell aligned and padded to its column width,
/// wrapped in `│` borders.
fn table_row_line(
    row: &[Vec<Cell>],
    widths: &[usize],
    aligns: &[Alignment],
    border: Style,
) -> Line<'static> {
    let mut spans = vec![Span::styled("\u{2502}".to_string(), border)];
    for (i, &width) in widths.iter().enumerate() {
        let empty = Vec::new();
        let cell = row.get(i).unwrap_or(&empty);
        let align = aligns.get(i).copied().unwrap_or(Alignment::None);
        spans.push(Span::raw(" ".to_string()));
        spans.extend(cells_to_spans(&align_cell(cell, width, align)));
        spans.push(Span::raw(" ".to_string()));
        spans.push(Span::styled("\u{2502}".to_string(), border));
    }
    Line::from(spans)
}

/// The header separator line (`├──┼──┤`) sized to the column widths.
fn table_separator(widths: &[usize], border: Style) -> Line<'static> {
    let mut text = String::from("\u{251c}");
    for (i, &width) in widths.iter().enumerate() {
        if i > 0 {
            text.push('\u{253c}');
        }
        text.push_str(&"\u{2500}".repeat(width + 2));
    }
    text.push('\u{2524}');
    Line::from(Span::styled(text, border))
}
impl BlockState<'_> {
    /// Starts a new table row (header or body).
    pub(super) fn table_begin_row(&mut self, head: bool) {
        if let Some(table) = self.table.as_mut() {
            table.in_head = head;
            table.rows.push(Vec::new());
            if head {
                table.head_rows += 1;
            }
        }
    }

    /// Converts the buffered inline events into the current row's next cell.
    pub(super) fn table_push_cell(&mut self) {
        let cell = {
            let events: Vec<&Event> = self.inline.iter().collect();
            inline_to_cells(&events, self.sheet, self.sheet.base)
        };
        self.inline.clear();
        if let Some(table) = self.table.as_mut()
            && let Some(row) = table.rows.last_mut()
        {
            row.push(cell);
        }
    }

    /// Lays out the accumulated table: aligned columns joined by `│`, with a
    /// `├─┼─┤` rule after the header, capped to the box width.
    pub(super) fn flush_table(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        let cols = table
            .aligns
            .len()
            .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
        if cols == 0 {
            return;
        }
        let mut widths = vec![1usize; cols];
        for row in &table.rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cells_width(cell).max(1));
            }
        }
        // Cap to the box: cols+1 bars + 2 padding spaces per column.
        let overhead = cols + 1 + 2 * cols;
        let avail = self.width.saturating_sub(overhead).max(cols);
        if widths.iter().sum::<usize>() > avail {
            let cap = (avail / cols).max(1);
            for width in &mut widths {
                *width = (*width).min(cap);
            }
        }
        let border = match self.sheet.table_border {
            Some(color) => Style::default().fg(color),
            None => Style::default(),
        };
        for (index, row) in table.rows.iter().enumerate() {
            self.out
                .push(table_row_line(row, &widths, &table.aligns, border));
            if table.head_rows > 0 && index + 1 == table.head_rows {
                self.out.push(table_separator(&widths, border));
            }
        }
        if self.at_top_level() {
            self.produced = true;
        }
    }
}
