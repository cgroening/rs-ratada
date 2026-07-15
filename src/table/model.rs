//! Pure data and layout helpers for the table: column allocation, cell
//! wrapping, type-aware comparison, sorting and fuzzy filtering.

use std::cmp::Ordering;

use chrono::NaiveDate;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{Align, Column, ColumnKind, FilterScope, Row, SEPARATOR, SortDir};
use crate::fuzzy;

/// Distributes `avail` columns across `(min, target)` specs: each column gets
/// at least `min`; the rest is shared round-robin up to `target`.
pub(super) fn allocate_columns(
    specs: &[(usize, usize)],
    avail: usize,
) -> Vec<usize> {
    let mut widths: Vec<usize> = specs.iter().map(|&(min, _)| min).collect();
    let separators = SEPARATOR.width() * specs.len().saturating_sub(1);
    let used: usize = widths.iter().sum::<usize>() + separators;
    let mut leftover = avail.saturating_sub(used);
    while leftover > 0 {
        let mut progressed = false;
        for (width, &(_, target)) in widths.iter_mut().zip(specs) {
            if leftover == 0 {
                break;
            }
            if *width < target {
                *width += 1;
                leftover -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    widths
}

/// Wraps `text` to `width` display columns, breaking between characters.
pub(super) fn wrap_cell(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let char_width = ch.width().unwrap_or(0);
        if used + char_width > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            used = 0;
        }
        current.push(ch);
        used += char_width;
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Compares two cells for ascending order under `kind`.
fn compare_cells(kind: ColumnKind, a: &str, b: &str) -> Ordering {
    match kind {
        ColumnKind::Number => {
            let x = a.trim().parse::<f64>().unwrap_or(f64::NEG_INFINITY);
            let y = b.trim().parse::<f64>().unwrap_or(f64::NEG_INFINITY);
            x.partial_cmp(&y).unwrap_or(Ordering::Equal)
        }
        ColumnKind::Date => {
            let parse =
                |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
            parse(a).cmp(&parse(b))
        }
        ColumnKind::Text => a.cmp(b),
    }
}

/// Stably sorts `indices` (row indices into `rows`) by `col` in direction
/// `dir`, using the column's [`ColumnKind`] for type-aware comparison.
pub(super) fn sort_indices(
    rows: &[Row],
    columns: &[Column],
    mut indices: Vec<usize>,
    col: usize,
    dir: SortDir,
) -> Vec<usize> {
    let kind = columns.get(col).map_or(ColumnKind::Text, |c| c.kind);
    indices.sort_by(|&a, &b| {
        let order = compare_cells(kind, rows[a].cell(col), rows[b].cell(col));
        match dir {
            SortDir::Asc => order,
            SortDir::Desc => order.reverse(),
        }
    });
    indices
}

/// Returns row indices matching `query` fuzzily; empty query keeps all rows in
/// order, otherwise results are best-score first.
pub(super) fn filter_indices(
    rows: &[Row],
    query: &str,
    scope: FilterScope,
    active_col: usize,
) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..rows.len()).collect();
    }
    let mut scored: Vec<(u32, usize)> = rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let haystack = match scope {
                FilterScope::AllColumns => row.cells.join(" "),
                FilterScope::ActiveColumn => row.cell(active_col).to_string(),
            };
            fuzzy::score(&haystack, query).map(|score| (score, index))
        })
        .collect();
    scored.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    scored.into_iter().map(|(_, index)| index).collect()
}

/// The top row offset that keeps `cursor` visible given per-row `heights` and
/// the viewport height `area_h`, starting from `prev`.
pub(super) fn visible_offset(
    heights: &[u16],
    cursor: usize,
    area_h: u16,
    prev: usize,
) -> usize {
    if heights.is_empty() || area_h == 0 {
        return 0;
    }
    let mut off = prev.min(cursor).min(heights.len() - 1);
    while off < cursor {
        let used: u16 = heights[off..=cursor].iter().copied().sum();
        if used <= area_h {
            break;
        }
        off += 1;
    }
    off
}

/// Pads `text` to `width` display columns on the side opposite `align`.
pub(super) fn pad_align(text: &str, width: usize, align: Align) -> String {
    let pad = width.saturating_sub(text.width());
    match align {
        Align::Left => format!("{text}{}", " ".repeat(pad)),
        Align::Right => format!("{}{text}", " ".repeat(pad)),
    }
}
