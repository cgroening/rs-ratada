//! Word-aware wrapping and the caret mapping between the logical text and
//! its wrapped display rows.
//!
//! Split out of the [`super::TextArea`] widget: these are pure functions
//! over a string and a width, and are public so a host that lays out its
//! own wrapped box gets the same geometry the widget uses.

use unicode_width::UnicodeWidthChar;

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

/// Maps a caret char index to its `(row, column)` in `rows`.
pub(super) fn locate(rows: &[(usize, usize)], pos: usize) -> (usize, usize) {
    for (index, &(start, end)) in rows.iter().enumerate() {
        if pos < end {
            return (index, pos - start);
        }
        if pos == end {
            match rows.get(index + 1) {
                // Soft-wrap boundary: caret belongs to the next row's start.
                Some(&(next_start, _)) if next_start == pos => {}
                _ => return (index, pos - start),
            }
        }
    }
    let last = rows.len() - 1;
    (last, pos - rows[last].0)
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
}
