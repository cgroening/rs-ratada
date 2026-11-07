//! Word-aware wrapping and the caret mapping between the logical text and
//! its wrapped display rows.
//!
//! Split out of the [`super::TextArea`] widget: these are pure functions
//! over a string and a width, and are public so a host that lays out its
//! own wrapped box gets the same geometry the widget uses.

use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

/// Splits `chars` into display rows of at most `width` columns (measured by
/// [`UnicodeWidthChar`], so wide glyphs count as two), breaking on newlines
/// (which are not included in any row).
///
/// Wrapping is **word-aware**: a soft break falls on the last space inside the
/// window, and that space is consumed (it belongs to no row). A word longer than
/// `width` is hard-split, and a single glyph wider than `width` still gets its
/// own row, so the loop always makes progress.
pub(super) fn wrap_ranges(chars: &[char], width: usize) -> Vec<(usize, usize)> {
    let width = width.max(1);
    let len = chars.len();
    let mut rows = Vec::new();
    let mut line_start = 0;
    loop {
        let line_end = chars[line_start..]
            .iter()
            .position(|&c| c == '\n')
            .map_or(len, |offset| line_start + offset);
        wrap_logical(chars, line_start, line_end, width, &mut rows);
        if line_end >= len {
            break;
        }
        line_start = line_end + 1;
        if line_start == len {
            // A trailing newline opens one last, empty row.
            rows.push((line_start, line_start));
            break;
        }
    }
    if rows.is_empty() {
        rows.push((0, 0));
    }
    rows
}

/// Wraps the newline-free span `[from, to)` of `chars` into `rows`.
pub(super) fn wrap_logical(
    chars: &[char],
    from: usize,
    to: usize,
    width: usize,
    rows: &mut Vec<(usize, usize)>,
) {
    if from == to {
        rows.push((from, from));
        return;
    }
    let mut start = from;
    while start < to {
        // The greedy end: as many chars as fit into `width` display columns.
        let mut end = start;
        let mut used = 0usize;
        while end < to {
            let char_width = chars[end].width().unwrap_or(0);
            if end > start && used + char_width > width {
                break;
            }
            used += char_width;
            end += 1;
        }
        if end >= to {
            rows.push((start, to));
            return;
        }
        // Prefer the last space inside the window; it is consumed by the break.
        match (start + 1..=end).rev().find(|&p| chars[p - 1] == ' ') {
            Some(p) if p - 1 > start => {
                rows.push((start, p - 1));
                start = p;
            }
            _ => {
                rows.push((start, end));
                start = end;
            }
        }
    }
}

/// Word-wraps `text` to `width` display columns, returning each display line
/// together with the character offset (into `text`) at which it starts.
///
/// Explicit `\n` always breaks; a soft break falls on the last space that fits
/// and consumes it, so no rendered line starts with the break space. An
/// over-long word is hard-split.
///
/// The companion [`cursor_to_display`] and [`display_to_cursor`] map a caret
/// between the flat char index and the `(line, column)` of these rows.
///
/// # Examples
///
/// ```
/// use ratada::textarea::wrap_offsets;
///
/// let rows = wrap_offsets("hello world", 8);
/// assert_eq!(rows, vec![("hello".to_string(), 0), ("world".to_string(), 6)]);
/// ```
#[must_use]
pub fn wrap_offsets(text: &str, width: usize) -> Vec<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    wrap_ranges(&chars, width)
        .into_iter()
        .map(|(start, end)| (chars[start..end].iter().collect(), start))
        .collect()
}

/// A text wrapped for a box that may show a caret: the display rows and the
/// caret's place among them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaretRows {
    /// The wrapped rows as `(text, char offset of its first char)`, plus the
    /// empty row the caret needed, if any (see [`wrap_for_caret`]).
    pub rows: Vec<(String, usize)>,
    /// The caret's `(row, column in chars)`; `None` without a caret.
    pub caret: Option<(usize, usize)>,
}

impl CaretRows {
    /// The caret's `(row, display column)`: where its block cell is drawn,
    /// so a popup anchored at the caret opens in the right cell even after
    /// wide glyphs. `None` without a caret.
    #[must_use]
    pub fn caret_cell(&self) -> Option<(usize, usize)> {
        let (row, column) = self.caret?;
        let text = self.rows.get(row).map_or("", |(text, _)| text.as_str());
        let before: String = text.chars().take(column).collect();
        Some((row, UnicodeWidthStr::width(before.as_str())))
    }
}

/// Word-wraps `text` to `width` display columns for a box that shows a caret
/// at char index `caret`, opening one more, empty row right below the
/// caret's row when the caret stands at the end of a row that already fills
/// the box. The block caret is drawn *after* its row's last character, so
/// without that row it would land one cell past the box. Only a full row
/// ending in the caret needs it: a soft break falls on a space, which leaves
/// its row short, and a hard break moves the caret to the next row itself.
#[must_use]
pub fn wrap_for_caret(
    text: &str,
    width: usize,
    caret: Option<usize>,
) -> CaretRows {
    let mut rows = wrap_offsets(text, width);
    let Some(caret) = caret else {
        return CaretRows { rows, caret: None };
    };
    let total = text.chars().count();
    let (row, column) = cursor_to_display(&rows, total, caret);
    let row_text = &rows[row].0;
    let is_at_end = column == row_text.chars().count();
    // Display columns, not chars: a row of wide glyphs fills the box while
    // holding half as many chars.
    if !is_at_end || UnicodeWidthStr::width(row_text.as_str()) < width.max(1) {
        return CaretRows {
            rows,
            caret: Some((row, column)),
        };
    }
    rows.insert(row + 1, (String::new(), caret.min(total)));
    CaretRows {
        rows,
        caret: Some((row + 1, 0)),
    }
}

/// The caret and the selection of the display row `index` (starting at char
/// `offset`, `len` chars long), for [`crate::input::line_spans`]: the cursor
/// column when the caret's `(row, column)` is on this row, and the part of
/// the text's `selection` (char range) that falls in it, made row-relative.
#[must_use]
pub fn row_caret(
    index: usize,
    offset: usize,
    len: usize,
    caret: Option<(usize, usize)>,
    selection: Option<(usize, usize)>,
) -> crate::input::LineCaret {
    crate::input::LineCaret {
        cursor: caret
            .filter(|(row, _)| *row == index)
            .map(|(_, column)| column),
        selection: selection
            .and_then(|(from, to)| {
                crate::input::intersect(from, to, offset, offset + len)
            })
            .map(|(from, to)| (from - offset, to - offset)),
    }
}

/// Maps a caret char index into its `(display line, column)` within `lines`, as
/// produced by [`wrap_offsets`]. `total` is the text's character count.
#[must_use]
pub fn cursor_to_display(
    lines: &[(String, usize)],
    total: usize,
    cursor: usize,
) -> (usize, usize) {
    let cursor = cursor.min(total);
    let mut line = 0;
    for (index, (_, start)) in lines.iter().enumerate() {
        if *start <= cursor {
            line = index;
        } else {
            break;
        }
    }
    let (text, start) = &lines[line];
    (line, (cursor - start).min(text.chars().count()))
}

/// Maps a `(display line, column)` from [`wrap_offsets`] back to a caret char
/// index. The inverse of [`cursor_to_display`].
#[must_use]
pub fn display_to_cursor(
    lines: &[(String, usize)],
    line: usize,
    col: usize,
) -> usize {
    let (text, start) = &lines[line];
    start + col.min(text.chars().count())
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn wrap_breaks_on_width_and_newlines() {
        let chars: Vec<char> = "abcdef\nxy".chars().collect();
        // width 4: "abcd","ef","xy" - no space, so the word is hard-split.
        assert_eq!(wrap_ranges(&chars, 4), vec![(0, 4), (4, 6), (7, 9)]);
    }

    #[test]
    fn wrap_breaks_on_the_last_space_and_consumes_it() {
        // "hello world" at width 8: the break falls on the space, which belongs
        // to no row, so the second line starts at the 'w' (offset 6).
        assert_eq!(
            wrap_offsets("hello world", 8),
            vec![("hello".to_string(), 0), ("world".to_string(), 6)],
        );
    }

    #[test]
    fn wrap_hard_splits_a_word_longer_than_the_width() {
        assert_eq!(
            wrap_offsets("abcdefgh", 3),
            vec![
                ("abc".to_string(), 0),
                ("def".to_string(), 3),
                ("gh".to_string(), 6),
            ],
        );
    }

    #[test]
    fn display_round_trips_a_caret_through_a_soft_break() {
        let text = "hello world";
        let rows = wrap_offsets(text, 8);
        let total = text.chars().count();
        // The caret on the 'w' is column 0 of the second display line.
        assert_eq!(cursor_to_display(&rows, total, 6), (1, 0));
        assert_eq!(display_to_cursor(&rows, 1, 0), 6);
        // The caret at the end of "hello" stays on the first line.
        assert_eq!(cursor_to_display(&rows, total, 5), (0, 5));
        assert_eq!(display_to_cursor(&rows, 0, 5), 5);
    }

    #[test]
    fn empty_buffer_has_one_row() {
        assert_eq!(wrap_ranges(&[], 4), vec![(0, 0)]);
    }

    #[test]
    fn wrap_measures_display_width_of_wide_chars() {
        // '世'/'界' are width-2; at width 3 only one wide glyph fits per row,
        // then the narrow 'a' joins the second row.
        let chars: Vec<char> = "世界a".chars().collect();
        assert_eq!(wrap_ranges(&chars, 3), vec![(0, 1), (1, 3)]);
    }

    #[test]
    fn trailing_newline_adds_empty_row() {
        let chars: Vec<char> = "ab\n".chars().collect();
        assert_eq!(wrap_ranges(&chars, 4), vec![(0, 2), (3, 3)]);
    }
    /// The caret at the end of a row that fills the box gets a row of its
    /// own right below it; anywhere else the layout is plain wrapping.
    #[test]
    fn a_caret_at_the_end_of_a_full_row_opens_a_row() {
        let laid = wrap_for_caret("abcd", 4, Some(4));
        assert_eq!(laid.rows, vec![("abcd".into(), 0), (String::new(), 4)]);
        assert_eq!(laid.caret, Some((1, 0)));
        let short = wrap_for_caret("abc", 4, Some(3));
        assert_eq!(short.rows.len(), 1);
        assert_eq!(short.caret, Some((0, 3)));
        let none = wrap_for_caret("abcd", 4, None);
        assert_eq!(none.rows.len(), 1);
        assert_eq!(none.caret, None);
    }

    /// Wide glyphs fill a row in half the chars, and count by display width.
    #[test]
    fn wide_glyphs_fill_a_row_by_width() {
        let laid = wrap_for_caret("日本", 4, Some(2));
        assert_eq!(laid.caret, Some((1, 0)));
        let cell = wrap_for_caret("日本", 6, Some(2)).caret_cell();
        assert_eq!(cell, Some((0, 4)), "two wide glyphs before the caret");
    }

    #[test]
    fn a_row_takes_its_part_of_the_caret_and_selection() {
        let caret = row_caret(1, 4, 4, Some((1, 2)), Some((2, 6)));
        assert_eq!(caret.cursor, Some(2));
        assert_eq!(caret.selection, Some((0, 2)));
        let other = row_caret(0, 0, 4, Some((1, 2)), None);
        assert_eq!(other.cursor, None);
        assert_eq!(other.selection, None);
    }
}
