//! Table rendering: the header band, the multi-line body with selection/cursor
//! styling, the scrollbar and the status/filter line.

use std::fmt::Write as _;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::model::{allocate_columns, pad_align, visible_offset, wrap_cell};
use super::{GUTTER, SEPARATOR, SelectMode, SortDir, Table};
use crate::{chrome, input, nav, scroll, style, text::truncate, theme::Skin};

impl Table {
    /// Renders the table: header, multi-line body with selection/cursor styling,
    /// scrollbar and an optional status/filter line.
    pub fn render(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let area = match &self.decor {
            Some(decor) => chrome::framed_decor(
                frame,
                area,
                skin,
                decor,
                &chrome::position_badge(self.cursor, self.view.len()),
            ),
            None => area,
        };
        let bottom = u16::from(self.filtering || self.show_status);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(bottom),
            ])
            .split(area);

        let specs: Vec<(usize, usize)> =
            self.columns.iter().map(|c| (c.min, c.target)).collect();
        let avail = (chunks[1].width as usize).saturating_sub(1 + GUTTER);
        let widths = allocate_columns(&specs, avail);

        // The header sits on its own `panel` band so it reads as a distinct
        // strip above the content rows.
        frame.render_widget(
            Paragraph::new(self.header_line(&widths, skin))
                .style(style::bg(skin.palette.panel)),
            chunks[0],
        );

        self.render_body(frame, chunks[1], &widths, skin);

        if bottom > 0 {
            let line = self.bottom_line(skin, chunks[2].width as usize);
            frame.render_widget(Paragraph::new(line), chunks[2]);
        }
    }

    /// Renders the scrollable body: the visible rows starting at the scroll
    /// offset (kept so the cursor stays in view) plus the scrollbar.
    fn render_body(
        &self,
        frame: &mut Frame,
        body: Rect,
        widths: &[usize],
        skin: &Skin,
    ) {
        let heights: Vec<u16> = self
            .view
            .iter()
            .map(|&row| self.row_height(row, widths))
            .collect();
        let offset = visible_offset(
            &heights,
            self.cursor,
            body.height,
            self.offset.get(),
        );
        self.offset.set(offset);

        let mut lines: Vec<Line> = Vec::new();
        let mut used = 0u16;
        let mut count = 0usize;
        for (view_idx, &height) in heights.iter().enumerate().skip(offset) {
            if used + height > body.height {
                break;
            }
            lines.extend(self.row_lines(
                view_idx,
                widths,
                body.width as usize,
                skin,
            ));
            used += height;
            count += 1;
        }
        self.viewport.set(count.max(1));
        frame.render_widget(Paragraph::new(lines), body);
        scroll::render_scrollbar(
            frame,
            body,
            skin,
            nav::ScrollView {
                total: self.view.len(),
                offset,
                viewport: count.max(1),
            },
        );
    }

    fn header_line(&self, widths: &[usize], skin: &Skin) -> Line<'static> {
        let mut spans: Vec<Span> = vec![Span::raw(" ".repeat(GUTTER))];
        for (col, (column, &width)) in
            self.columns.iter().zip(widths).enumerate()
        {
            if col > 0 {
                spans.push(Span::raw(SEPARATOR));
            }
            let mut title = column.title.clone();
            if let Some((sorted, dir)) = self.sort
                && sorted == col
            {
                title = format!("{title} {}", sort_arrow(dir));
            }
            let mut style = column.header_style.unwrap_or(self.header_style);
            if col == self.active_col {
                style =
                    style::fg(skin.palette.accent).add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(
                pad_align(&truncate(&title, width), width, column.align),
                style,
            ));
        }
        Line::from(spans)
    }

    fn row_height(&self, row: usize, widths: &[usize]) -> u16 {
        let row = &self.rows[row];
        let height = (0..self.columns.len())
            .map(|col| {
                if self.columns[col].wrap {
                    wrap_cell(row.cell(col), widths[col]).len()
                } else {
                    1
                }
            })
            .max()
            .unwrap_or(1);
        height.max(1) as u16
    }

    fn row_lines(
        &self,
        view_idx: usize,
        widths: &[usize],
        line_width: usize,
        skin: &Skin,
    ) -> Vec<Line<'static>> {
        let orig = self.view[view_idx];
        let row = &self.rows[orig];
        let is_cursor = view_idx == self.cursor;
        let row_bg = self.row_highlight(orig, is_cursor, skin);
        let sublines: Vec<Vec<String>> = self
            .columns
            .iter()
            .enumerate()
            .map(|(col, column)| {
                if column.wrap {
                    wrap_cell(row.cell(col), widths[col])
                } else {
                    vec![truncate(row.cell(col), widths[col])]
                }
            })
            .collect();
        let height = sublines.iter().map(Vec::len).max().unwrap_or(1).max(1);

        // Content width already laid out before the trailing fill: gutter plus
        // every column and the separators between them.
        let separators = SEPARATOR.width() * widths.len().saturating_sub(1);
        let content_width = GUTTER + widths.iter().sum::<usize>() + separators;

        (0..height)
            .map(|line_no| {
                let mut gutter = self.gutter_span(orig, line_no, skin);
                if let Some(bg) = row_bg {
                    gutter.style = gutter.style.patch(bg);
                }
                let mut spans: Vec<Span> = vec![gutter];
                for (col, &width) in widths.iter().enumerate() {
                    if col > 0 {
                        spans.push(Span::styled(
                            SEPARATOR,
                            row_bg.unwrap_or_default(),
                        ));
                    }
                    let text =
                        sublines[col].get(line_no).cloned().unwrap_or_default();
                    let style = self.cell_style(orig, col, is_cursor, skin);
                    spans.push(Span::styled(
                        pad_align(&text, width, self.columns[col].align),
                        style,
                    ));
                }
                // Extend the highlight bar to the right edge so it reads as one
                // continuous row rather than per-cell patches.
                if let Some(bg) = row_bg {
                    let trailing = line_width.saturating_sub(content_width);
                    if trailing > 0 {
                        spans.push(Span::styled(" ".repeat(trailing), bg));
                    }
                }
                Line::from(spans)
            })
            .collect()
    }

    /// The full-width background for a row's highlight bar, or `None` when the
    /// row is neither the cursor nor selected. Mirrors the background that
    /// [`Self::cell_style`] applies per cell so the bar reads as one strip;
    /// only meaningful in [`SelectMode::Row`] (cell mode highlights per cell).
    fn row_highlight(
        &self,
        orig: usize,
        is_cursor: bool,
        skin: &Skin,
    ) -> Option<Style> {
        if self.mode != SelectMode::Row {
            return None;
        }
        if is_cursor {
            return Some(cursor_highlight(skin));
        }
        if self.selected_rows.contains(&orig) {
            return Some(style::bg(skin.palette.selection));
        }
        None
    }

    /// The 2-cell left gutter: a check marker for selected rows (row mode).
    fn gutter_span(
        &self,
        orig: usize,
        line_no: usize,
        skin: &Skin,
    ) -> Span<'static> {
        let selected =
            self.mode == SelectMode::Row && self.selected_rows.contains(&orig);
        let text = if line_no == 0 && selected {
            format!("{} ", skin.glyphs.check)
        } else {
            " ".repeat(GUTTER)
        };
        Span::styled(text, style::fg(skin.palette.accent))
    }

    fn cell_style(
        &self,
        orig: usize,
        col: usize,
        is_cursor: bool,
        skin: &Skin,
    ) -> Style {
        let mut style = Style::default();
        if let Some(cs) = self.columns[col].cell_style {
            style = style.patch(cs);
        }
        if let Some(rs) = self.rows[orig].style {
            style = style.patch(rs);
        }
        let selected = match self.mode {
            SelectMode::Row => self.selected_rows.contains(&orig),
            SelectMode::Cell => self.selected_cells.contains(&(orig, col)),
        };
        if selected {
            style = style.patch(style::bg(skin.palette.selection));
        }
        let cursor_here = is_cursor
            && (self.mode == SelectMode::Row || col == self.active_col);
        if cursor_here {
            style = style.patch(cursor_highlight(skin));
        }
        style
    }

    /// The status line, or the `/query` filter line while filtering; both are
    /// clipped to `width`.
    fn bottom_line(&self, skin: &Skin, width: usize) -> Line<'static> {
        let palette = &skin.palette;
        if self.filtering {
            let mut spans = vec![Span::styled("/", style::fg(palette.accent))];
            spans.extend(input::query_spans(
                &self.filter,
                palette,
                width.saturating_sub(1),
            ));
            return Line::from(spans);
        }
        let count = match self.mode {
            SelectMode::Row => self.selected_rows().len(),
            SelectMode::Cell => self.selected_cells.len(),
        };
        let position = if self.view.is_empty() {
            0
        } else {
            self.cursor + 1
        };
        let mut text = format!(
            " row {position}/{} \u{b7} {count} selected",
            self.view.len()
        );
        if let Some((col, dir)) = self.sort {
            let title = self.columns.get(col).map_or("", |c| c.title.as_str());
            let _ = write!(text, " \u{b7} sort: {title}{}", sort_arrow(dir));
        }
        if !self.filter.is_empty() {
            let _ = write!(text, " \u{b7} filter: {}", self.filter);
        }
        Line::from(Span::styled(text, style::secondary(palette)))
    }
}

/// The up/down triangle marking a column's sort direction.
fn sort_arrow(dir: SortDir) -> char {
    match dir {
        SortDir::Asc => '\u{25b2}',
        SortDir::Desc => '\u{25bc}',
    }
}

/// The cursor-row highlight: a subtle `selection` tint.
fn cursor_highlight(skin: &Skin) -> Style {
    style::bg(skin.palette.selection)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two directions must not share a glyph, or a sorted column would not
    /// show which way it is sorted.
    #[test]
    fn the_sort_arrow_distinguishes_the_two_directions() {
        assert_eq!(sort_arrow(SortDir::Asc), '\u{25b2}');
        assert_eq!(sort_arrow(SortDir::Desc), '\u{25bc}');
        assert_ne!(sort_arrow(SortDir::Asc), sort_arrow(SortDir::Desc));
    }
}
