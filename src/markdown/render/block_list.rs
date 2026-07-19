//! List state: nesting levels, bullet glyphs and task-list markers.

use ratatui::style::Style;

use super::{BlockState, inline::styled_cells};

/// One open list level: ordered lists carry the next item number.
pub(super) struct ListLevel {
    pub(super) ordered: Option<u64>,
}
impl BlockState<'_> {
    /// Computes the marker cells for a new list item (bullet/number).
    pub(super) fn begin_item(&mut self) {
        let depth = self.lists.len().saturating_sub(1);
        let marker = match self.lists.last_mut() {
            Some(level) => match level.ordered.as_mut() {
                Some(number) => {
                    let text = format!("{number}. ");
                    *number += 1;
                    styled_cells(&text, self.bullet_style())
                }
                None => {
                    let glyph = self.bullet_glyph(depth);
                    styled_cells(&format!("{glyph} "), self.bullet_style())
                }
            },
            None => Vec::new(),
        };
        self.pending_marker = Some(marker);
    }

    /// Replaces the pending bullet with a checkbox glyph for a task item.
    pub(super) fn set_task_marker(&mut self, checked: bool) {
        let (glyph, style) = if checked {
            (
                &self.sheet.checkbox.checked,
                self.sheet.checkbox.checked_style(),
            )
        } else {
            (
                &self.sheet.checkbox.unchecked,
                self.sheet.checkbox.unchecked_style(),
            )
        };
        self.pending_marker = Some(styled_cells(&format!("{glyph} "), style));
    }

    pub(super) fn bullet_glyph(&self, depth: usize) -> String {
        let glyphs = &self.sheet.bullet.glyphs;
        if glyphs.is_empty() {
            "\u{2022}".to_string() // •
        } else {
            glyphs[depth % glyphs.len()].clone()
        }
    }

    pub(super) fn bullet_style(&self) -> Style {
        match self.sheet.bullet.fg {
            Some(color) => Style::default().fg(color),
            None => Style::default(),
        }
    }
}
