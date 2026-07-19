//! Turning the command list into displayable rows: grouped by category while
//! the query is empty, fuzzy-ranked once it is not.

use super::{CommandItem, Row};
use crate::fuzzy;

/// The width of the category column shown while searching.
pub(super) const CATEGORY_WIDTH: usize = 12;

/// The rows to render plus the navigation index maps for the current query.
pub(super) struct RowLayout<'a> {
    pub(super) rows: Vec<Row<'a>>,
    /// Row index of each selectable (enabled) item, in display order.
    pub(super) selectable: Vec<usize>,
    /// Position within `selectable` of each section's first selectable item.
    /// Empty while searching (the flat list has no sections).
    pub(super) section_starts: Vec<usize>,
}

/// The original index of the command the cursor sits on, if any.
pub(super) fn selected_index(
    layout: &RowLayout,
    cursor: usize,
) -> Option<usize> {
    let row = *layout.selectable.get(cursor)?;
    match layout.rows[row] {
        Row::Item { index, .. } => Some(index),
        Row::Header(_) => None,
    }
}

/// Builds the rows for `items`: grouped under section headers when `query` is
/// empty, otherwise a flat list ranked by fuzzy score.
pub(super) fn layout_rows<'a>(
    items: &'a [CommandItem<'a>],
    query: &str,
) -> RowLayout<'a> {
    if query.trim().is_empty() {
        grouped_rows(items)
    } else {
        ranked_rows(items, query.trim())
    }
}

/// Groups `items` under a header per category, preserving their given order.
/// Only enabled items are selectable; disabled ones still render (dimmed).
pub(super) fn grouped_rows<'a>(items: &'a [CommandItem<'a>]) -> RowLayout<'a> {
    let mut rows: Vec<Row<'a>> = Vec::new();
    let mut selectable: Vec<usize> = Vec::new();
    let mut section_starts: Vec<usize> = Vec::new();
    let mut current_category: Option<&str> = None;
    let mut section_has_selectable = false;

    for (index, item) in items.iter().enumerate() {
        if current_category != Some(item.category) {
            rows.push(Row::Header(item.category));
            current_category = Some(item.category);
            section_has_selectable = false;
        }
        if item.enabled {
            if !section_has_selectable {
                section_starts.push(selectable.len());
                section_has_selectable = true;
            }
            selectable.push(rows.len());
        }
        rows.push(Row::Item { item, index });
    }
    RowLayout {
        rows,
        selectable,
        section_starts,
    }
}

/// Filters `items` to those matching `query` and orders them by score, best
/// first. Only enabled items are selectable; disabled matches still render.
pub(super) fn ranked_rows<'a>(
    items: &'a [CommandItem<'a>],
    query: &str,
) -> RowLayout<'a> {
    let ranked = fuzzy::rank_by(items, query, |item| {
        format!("{} {}", item.category, item.label).into()
    });

    let mut rows: Vec<Row<'a>> = Vec::new();
    let mut selectable: Vec<usize> = Vec::new();
    for index in ranked {
        let item = &items[index];
        if item.enabled {
            selectable.push(rows.len());
        }
        rows.push(Row::Item { item, index });
    }
    RowLayout {
        rows,
        selectable,
        section_starts: Vec::new(),
    }
}
