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
    fuzzy::rank_by(rows, query, |row| match scope {
        FilterScope::AllColumns => row.cells.join(" ").into(),
        FilterScope::ActiveColumn => row.cell(active_col).into(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cells: &[&str]) -> Row {
        Row::new(cells.iter().map(|c| (*c).to_string()).collect())
    }

    fn rows(table: &[&[&str]]) -> Vec<Row> {
        table.iter().map(|cells| row(cells)).collect()
    }

    #[test]
    fn every_column_gets_at_least_its_minimum() {
        // Far less room than the minimums plus separators need.
        let widths = allocate_columns(&[(5, 20), (5, 20), (5, 20)], 4);
        assert_eq!(widths, vec![5, 5, 5]);
    }

    #[test]
    fn leftover_room_is_shared_round_robin_up_to_the_target() {
        // 3 columns, minimum 2 each, 2 separators of 2 columns => 10 used.
        // 15 available leaves 5 to hand out, one column at a time.
        let widths = allocate_columns(&[(2, 10), (2, 10), (2, 10)], 15);
        assert_eq!(widths.iter().sum::<usize>(), 2 * 3 + 5);
        let spread =
            widths.iter().max().unwrap() - widths.iter().min().unwrap();
        assert!(spread <= 1, "not shared evenly: {widths:?}");
    }

    /// A column already at its target must not absorb further leftover, and the
    /// loop must still terminate once nothing can grow.
    #[test]
    fn a_column_never_grows_past_its_target() {
        let widths = allocate_columns(&[(1, 2), (1, 100)], 60);
        assert_eq!(widths[0], 2);
        assert!(widths[1] > 2);
    }

    #[test]
    fn wrap_cell_breaks_on_the_display_width() {
        assert_eq!(wrap_cell("abcdef", 3), vec!["abc", "def"]);
    }

    /// Wide glyphs count as two columns, so only one fits in a width of 3.
    #[test]
    fn wrap_cell_measures_wide_glyphs_as_two_columns() {
        assert_eq!(
            wrap_cell("\u{4f60}\u{597d}", 3),
            vec!["\u{4f60}", "\u{597d}"]
        );
    }

    /// A zero width would otherwise loop forever looking for room.
    #[test]
    fn wrap_cell_yields_one_empty_line_for_a_zero_width() {
        assert_eq!(wrap_cell("abc", 0), vec![String::new()]);
    }

    #[test]
    fn wrap_cell_keeps_an_empty_text_as_one_empty_line() {
        assert_eq!(wrap_cell("", 5), vec![String::new()]);
    }

    #[test]
    fn numbers_sort_numerically_not_lexically() {
        let data = rows(&[&["9"], &["10"], &["2"]]);
        let columns = vec![Column::number("n")];
        let sorted =
            sort_indices(&data, &columns, vec![0, 1, 2], 0, SortDir::Asc);
        assert_eq!(sorted, vec![2, 0, 1], "lexical order would be 10, 2, 9");
    }

    /// An unparseable number sorts as negative infinity, so it lands first
    /// ascending rather than panicking or comparing as text.
    #[test]
    fn an_unparseable_number_sorts_to_the_bottom_edge() {
        let data = rows(&[&["3"], &["n/a"], &["1"]]);
        let columns = vec![Column::number("n")];
        let sorted =
            sort_indices(&data, &columns, vec![0, 1, 2], 0, SortDir::Asc);
        assert_eq!(sorted, vec![1, 2, 0]);
    }

    #[test]
    fn dates_sort_chronologically() {
        let data = rows(&[&["2026-01-09"], &["2026-01-10"], &["2025-12-31"]]);
        let columns = vec![Column::date("d")];
        let sorted =
            sort_indices(&data, &columns, vec![0, 1, 2], 0, SortDir::Asc);
        assert_eq!(sorted, vec![2, 0, 1]);
    }

    #[test]
    fn descending_reverses_the_ascending_order() {
        let data = rows(&[&["b"], &["a"], &["c"]]);
        let columns = vec![Column::text("t")];
        let asc = sort_indices(&data, &columns, vec![0, 1, 2], 0, SortDir::Asc);
        let desc =
            sort_indices(&data, &columns, vec![0, 1, 2], 0, SortDir::Desc);
        let mut reversed = asc.clone();
        reversed.reverse();
        assert_eq!(desc, reversed);
    }

    /// An out-of-range column must neither panic nor reorder: every row reads
    /// an empty cell there, so they compare equal and the stable sort keeps
    /// the incoming order.
    #[test]
    fn sorting_an_unknown_column_is_a_no_op() {
        let data = rows(&[&["b"], &["a"]]);
        let sorted = sort_indices(&data, &[], vec![0, 1], 7, SortDir::Asc);
        assert_eq!(sorted, vec![0, 1]);
    }

    /// The sort is stable, so rows that compare equal keep their prior order -
    /// what makes sorting by one column and then another behave predictably.
    #[test]
    fn equal_cells_keep_their_previous_order() {
        let data = rows(&[&["same", "first"], &["same", "second"]]);
        let columns = vec![Column::text("t"), Column::text("u")];
        let sorted = sort_indices(&data, &columns, vec![1, 0], 0, SortDir::Asc);
        assert_eq!(sorted, vec![1, 0]);
    }

    #[test]
    fn an_empty_filter_keeps_every_row_in_order() {
        let data = rows(&[&["alpha"], &["beta"]]);
        let kept = filter_indices(&data, "   ", FilterScope::AllColumns, 0);
        assert_eq!(kept, vec![0, 1]);
    }

    /// Scoped to the active column, a match in another column must not count.
    #[test]
    fn the_active_column_scope_ignores_the_other_columns() {
        let data = rows(&[&["alpha", "zzz"], &["beta", "match"]]);
        let all = filter_indices(&data, "match", FilterScope::AllColumns, 0);
        assert_eq!(all, vec![1]);
        let scoped =
            filter_indices(&data, "match", FilterScope::ActiveColumn, 0);
        assert!(scoped.is_empty(), "column 0 holds no match");
    }

    #[test]
    fn the_offset_stays_put_while_the_cursor_is_visible() {
        let heights = [1, 1, 1, 1, 1];
        assert_eq!(visible_offset(&heights, 2, 5, 0), 0);
    }

    #[test]
    fn the_offset_advances_just_far_enough_to_reveal_the_cursor() {
        let heights = [1, 1, 1, 1, 1];
        // Rows 0..=4 need 5 lines but only 3 fit, so the window starts at 2.
        assert_eq!(visible_offset(&heights, 4, 3, 0), 2);
    }

    /// A tall row consumes several lines, so fewer rows fit above the cursor.
    #[test]
    fn a_taller_row_pushes_the_offset_further_down() {
        let heights = [3, 3, 1];
        assert_eq!(visible_offset(&heights, 2, 4, 0), 1);
    }

    #[test]
    fn an_empty_table_or_zero_height_yields_offset_zero() {
        assert_eq!(visible_offset(&[], 3, 10, 5), 0);
        assert_eq!(visible_offset(&[1, 1], 1, 0, 1), 0);
    }

    #[test]
    fn pad_align_pads_on_the_side_opposite_the_alignment() {
        assert_eq!(pad_align("ab", 5, Align::Left), "ab   ");
        assert_eq!(pad_align("ab", 5, Align::Right), "   ab");
    }

    /// Text wider than the field must not underflow the padding arithmetic.
    #[test]
    fn pad_align_leaves_overlong_text_untouched() {
        assert_eq!(pad_align("abcdef", 3, Align::Left), "abcdef");
    }
}
