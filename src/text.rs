//! Text helpers for terminal rendering.

use std::mem;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Truncates `text` to `width` display columns, appending '…' when clipped.
pub fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.width() <= width {
        return text.to_string();
    }
    let budget = width.saturating_sub(1);
    let mut result = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let char_width = ch.width().unwrap_or(0);
        if used + char_width > budget {
            break;
        }
        result.push(ch);
        used += char_width;
    }
    result.push('\u{2026}');
    result
}

/// Returns the slice of `text` occupying display columns
/// `[start_col, start_col + width)`. Column-accurate via `unicode-width`; a wide
/// glyph straddling either edge is dropped rather than split, so the result
/// spans at most `width` columns with no partial cell. Used for horizontal
/// scrolling (an alternative to [`truncate`]'s ellipsis clip).
pub fn window(text: &str, start_col: usize, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut column = 0; // display column where the current char begins
    let mut used = 0; // columns already emitted into the window
    for ch in text.chars() {
        let char_width = ch.width().unwrap_or(0);
        let char_start = column;
        column += char_width;
        // Skip anything ending at or before the window start, including a wide
        // glyph that straddles the start boundary.
        if char_start < start_col {
            continue;
        }
        if used + char_width > width {
            break;
        }
        result.push(ch);
        used += char_width;
    }
    result
}

/// Pads `text` with trailing spaces to exactly `width` display columns, so a
/// styled background reads as a full-width bar. Returns `text` unchanged when it
/// already meets or exceeds `width`.
pub fn pad_end(text: &str, width: usize) -> String {
    let current = text.width();
    if current >= width {
        return text.to_string();
    }
    let mut result = text.to_string();
    result.push_str(&" ".repeat(width - current));
    result
}

/// Wraps `text` to lines of at most `width` display columns, breaking at spaces
/// where possible and hard-splitting a word longer than `width`. Embedded
/// newlines force a break (a blank line is preserved). Widths are measured with
/// `unicode-width` (a wide glyph counts as two columns); runs of whitespace
/// within a line collapse to a single space. A `width` of `0` is treated as `1`.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for segment in text.split('\n') {
        wrap_segment(segment, width, &mut lines);
    }
    lines
}

/// Greedily wraps one newline-free `segment` into `out`.
fn wrap_segment(segment: &str, width: usize, out: &mut Vec<String>) {
    let mut line = String::new();
    let mut line_width = 0;
    for word in segment.split_whitespace() {
        let word_width = word.width();
        if word_width > width {
            // An over-long word: flush, then hard-split it across full lines;
            // the trailing partial continues as the current line.
            if !line.is_empty() {
                out.push(mem::take(&mut line));
            }
            let chunks = split_to_width(word, width);
            let last = chunks.len() - 1;
            for (index, chunk) in chunks.into_iter().enumerate() {
                if index < last {
                    out.push(chunk);
                } else {
                    line_width = chunk.width();
                    line = chunk;
                }
            }
            continue;
        }
        let separator = usize::from(!line.is_empty());
        if !line.is_empty() && line_width + separator + word_width > width {
            out.push(mem::take(&mut line));
            line_width = 0;
        }
        if !line.is_empty() {
            line.push(' ');
            line_width += 1;
        }
        line.push_str(word);
        line_width += word_width;
    }
    out.push(line);
}

/// Splits `text` into pieces each at most `width` display columns wide, breaking
/// between characters (never splitting a wide glyph).
fn split_to_width(text: &str, width: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let char_width = ch.width().unwrap_or(0);
        if used + char_width > width && !current.is_empty() {
            pieces.push(mem::take(&mut current));
            used = 0;
        }
        current.push(ch);
        used += char_width;
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_leaves_short_text_untouched() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_appends_ellipsis_when_clipped() {
        assert_eq!(truncate("hello world", 5), "hell\u{2026}");
    }

    #[test]
    fn truncate_to_zero_is_empty() {
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn window_from_start_takes_leading_columns() {
        assert_eq!(window("hello world", 0, 5), "hello");
    }

    #[test]
    fn window_offset_skips_leading_columns() {
        assert_eq!(window("hello world", 6, 5), "world");
    }

    #[test]
    fn window_drops_wide_glyph_straddling_the_start() {
        // The wide 'あ' occupies columns 0..2; starting at column 1 drops it.
        assert_eq!(window("\u{3042}b", 1, 2), "b");
    }

    #[test]
    fn window_never_emits_a_partial_wide_cell() {
        // 'a' fills column 0; 'あ' (cols 1..3) would exceed width 2, so it is
        // dropped rather than split.
        assert_eq!(window("a\u{3042}b", 0, 2), "a");
    }

    #[test]
    fn window_to_zero_is_empty() {
        assert_eq!(window("hello", 3, 0), "");
    }

    #[test]
    fn pad_end_fills_to_width() {
        assert_eq!(pad_end("ab", 5), "ab   ");
    }

    #[test]
    fn pad_end_leaves_full_text_untouched() {
        assert_eq!(pad_end("abcde", 5), "abcde");
        assert_eq!(pad_end("abcdef", 5), "abcdef");
    }

    #[test]
    fn wrap_breaks_at_word_boundaries() {
        assert_eq!(
            wrap("the quick brown fox", 9),
            vec!["the quick", "brown fox"],
        );
    }

    #[test]
    fn wrap_hard_splits_an_overlong_word() {
        assert_eq!(
            wrap("supercalifragilistic", 5),
            vec!["super", "calif", "ragil", "istic"],
        );
    }

    #[test]
    fn wrap_keeps_wrapping_after_a_split_word() {
        // The long word splits into "abcde"/"f"; "gh" then joins the tail "f"
        // since "f gh" (4 cols) fits the width.
        assert_eq!(wrap("abcdef gh", 5), vec!["abcde", "f gh"]);
    }

    #[test]
    fn wrap_preserves_blank_lines_from_newlines() {
        assert_eq!(wrap("a\n\nb", 10), vec!["a", "", "b"]);
    }

    #[test]
    fn wrap_measures_wide_glyphs_as_two_columns() {
        // The two wide glyphs fill the 4-column line, so 'x' wraps.
        assert_eq!(
            wrap("\u{4e16}\u{754c} x", 4),
            vec!["\u{4e16}\u{754c}", "x"]
        );
    }

    #[test]
    fn wrap_zero_width_behaves_as_one() {
        assert_eq!(wrap("ab", 0), vec!["a", "b"]);
    }
}
