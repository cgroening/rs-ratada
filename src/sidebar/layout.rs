//! Sidebar geometry: the flattened row list, where the cursor sits in it, and
//! the vertical/horizontal scroll offsets that keep it visible.

use super::{Overflow, Row, Sidebar, section_header_row};
use unicode_width::UnicodeWidthStr;

impl Sidebar {
    /// The rows to render: section headers interleaved with matching items. A
    /// section contributes its header only when it has at least one match.
    pub(super) fn rows(&self) -> Vec<Row<'_>> {
        let mut rows = Vec::new();
        for section in &self.sections {
            let mut header_shown = false;
            for item in section.items.iter().filter(|item| self.matches(item)) {
                if !header_shown {
                    if !section.title.is_empty() {
                        rows.push(Row::Header(&section.title));
                    }
                    header_shown = true;
                }
                rows.push(Row::Item(item));
            }
        }
        rows
    }

    /// The row index of the currently selected item (0 when there is none).
    pub(super) fn selected_row(&self, rows: &[Row]) -> usize {
        let mut item_index = 0;
        for (row_index, row) in rows.iter().enumerate() {
            if let Row::Item(_) = row {
                if item_index == self.selected {
                    return row_index;
                }
                item_index += 1;
            }
        }
        0
    }

    /// The vertical scroll offset keeping the selected item visible. When
    /// scrolling up it reveals the item's section header too, so the first item
    /// sits at the very top (offset 0) and the header stays in view - otherwise
    /// `keep_visible` would stop at the item and clip the header above it.
    pub(super) fn scroll_offset(
        &self,
        rows: &[Row],
        selected_row: usize,
        height: usize,
    ) -> usize {
        if height == 0 || rows.is_empty() {
            return 0;
        }
        let max_offset = rows.len().saturating_sub(height);
        let anchor = section_header_row(rows, selected_row);
        let mut offset = self.offset.get().min(max_offset);
        if anchor < offset {
            offset = anchor;
        }
        if selected_row >= offset + height {
            offset = selected_row + 1 - height;
        }
        offset.min(max_offset)
    }

    /// Clamps and stores the horizontal offset against the widest row, returning
    /// the value to render (always 0 outside scroll mode).
    pub(super) fn clamp_h_offset(
        &self,
        max_width: usize,
        inner_width: usize,
    ) -> usize {
        if self.overflow != Overflow::Scroll {
            return 0;
        }
        let max_offset = max_width.saturating_sub(inner_width);
        let clamped = self.h_offset.get().min(max_offset);
        self.h_offset.set(clamped);
        clamped
    }

    /// The display width a row needs, including the two-column item prefix.
    pub(super) fn row_width(row: &Row) -> usize {
        match row {
            Row::Header(title) => title.width(),
            Row::Item(item) => item.label.width() + 2,
        }
    }
}
