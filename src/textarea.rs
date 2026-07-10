//! A multi-line text area: wrapped editing with a caret, selection and
//! clipboard. Reuses [`TextCursor`] and the shared edit core
//! [`input::apply_edit_key`] from the `input` module, so it carries the same
//! shortcuts as a single-line field - including `Ctrl+U`/`Ctrl+K`, which act on
//! the **display** line, not the logical one.
//!
//! Wrapping is word-aware: a soft break falls on the last space that fits, and
//! a word longer than the width is hard-split. [`wrap_offsets`],
//! [`cursor_to_display`] and [`display_to_cursor`] expose that mapping, for a
//! host that measures and renders a wrapped box itself.
//!
//! The caller handles its own control keys (e.g. `Esc`, `Ctrl+G` for an
//! external editor) before delegating editing keys here.

use std::cell::Cell;

use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};
use unicode_width::UnicodeWidthChar;

use super::{
    chrome,
    input::{self, TextCursor},
    nav, scroll, style,
};
use crate::theme::Skin;

/// A wrapped, editable multi-line text buffer.
#[derive(Default)]
pub struct TextArea {
    text: String,
    cursor: TextCursor,
    width: Cell<usize>,
    scroll: Cell<usize>,
    max_len: Option<usize>,
    decor: Option<chrome::BoxDecor>,
}

impl TextArea {
    /// Creates a multi-line editor pre-filled with `initial`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ratada::textarea::TextArea;
    ///
    /// let area = TextArea::new("line 1\nline 2").max_len(200);
    /// assert_eq!(area.text(), "line 1\nline 2");
    /// ```
    pub fn new(initial: &str) -> Self {
        Self {
            text: initial.to_string(),
            cursor: TextCursor::at_end(initial),
            width: Cell::new(1),
            scroll: Cell::new(0),
            ..Self::default()
        }
    }

    /// Limits the buffer to `max` characters (enforced on typing and paste) and
    /// feeds the badge in the boxed variant.
    #[must_use]
    pub fn max_len(mut self, max: usize) -> Self {
        self.max_len = Some(max);
        self
    }

    /// Draws the area inside a rounded box with the given caption/badge (see
    /// [`chrome::BoxDecor`]); omit it for a plain area.
    #[must_use]
    pub fn boxed(mut self, decor: chrome::BoxDecor) -> Self {
        self.decor = Some(decor);
        self
    }

    /// The current buffer contents.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replaces the whole buffer (e.g. after an external editor) and parks the
    /// caret at the end.
    pub fn set_text(&mut self, text: String) {
        self.cursor = TextCursor::at_end(&text);
        self.text = text;
    }

    /// Applies one editing key; returns whether it was consumed.
    ///
    /// The whole behaviour lives in [`input::apply_edit_key`], driven with the
    /// current wrap width, so a text area and a single-line field never drift
    /// apart.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        input::apply_edit_key(
            &mut self.text,
            &mut self.cursor,
            key,
            input::EditMode::Multiline {
                width: self.width.get().max(1),
            },
            self.max_len,
        )
    }

    /// Renders the buffer into `area`, scrolling so the caret stays visible and
    /// filling the field with the input background (active tint when `focused`).
    /// A block caret is shown only when `focused`. Wrapped in a box when
    /// decorated via [`Self::boxed`].
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        skin: &Skin,
        focused: bool,
    ) {
        let inner = match &self.decor {
            Some(decor) => {
                chrome::framed_decor(frame, area, skin, decor, &self.badge())
            }
            None => area,
        };
        let palette = &skin.palette;
        let base_bg = if focused {
            palette.input_bg_active
        } else {
            palette.input_bg
        };

        let width = inner.width.max(1) as usize;
        let height = inner.height.max(1) as usize;
        self.width.set(width);
        let chars: Vec<char> = self.text.chars().collect();
        let rows = wrap_ranges(&chars, width);
        let (caret_row, caret_col) = locate(&rows, self.cursor.pos);

        let scroll = nav::keep_visible(
            nav::ScrollView {
                total: rows.len(),
                offset: self.scroll.get(),
                viewport: height,
            },
            caret_row,
        );
        self.scroll.set(scroll);

        let selection = self.cursor.selection();
        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(scroll)
            .take(height)
            .map(|(row_index, &(start, end))| {
                let visible: String = chars[start..end].iter().collect();
                let paint = input::LinePaint {
                    caret: input::LineCaret {
                        cursor: (focused && row_index == caret_row)
                            .then_some(caret_col),
                        selection: selection
                            .and_then(|(from, to)| {
                                input::intersect(from, to, start, end)
                            })
                            .map(|(from, to)| (from - start, to - start)),
                    },
                    ..input::LinePaint::default()
                };
                Line::from(input::line_spans(&visible, paint, palette))
            })
            .collect();

        // The paragraph's base style fills the whole field (including blank
        // rows) with the input background; spans override per cell.
        frame.render_widget(
            Paragraph::new(lines).style(style::bg(base_bg)),
            inner,
        );
        // A scrollbar on the right whenever the wrapped text overflows.
        scroll::render_scrollbar(
            frame,
            inner,
            skin,
            nav::ScrollView {
                total: rows.len(),
                offset: scroll,
                viewport: height,
            },
        );
    }

    /// The automatic badge text: character count, or `n/max` with a limit.
    fn badge(&self) -> String {
        let count = self.text.chars().count();
        match self.max_len {
            Some(max) => format!("{count}/{max}"),
            None => count.to_string(),
        }
    }
}

/// Splits `chars` into display rows of at most `width` columns (measured by
/// [`UnicodeWidthChar`], so wide glyphs count as two), breaking on newlines
/// (which are not included in any row).
///
/// Wrapping is **word-aware**: a soft break falls on the last space inside the
/// window, and that space is consumed (it belongs to no row). A word longer than
/// `width` is hard-split, and a single glyph wider than `width` still gets its
/// own row, so the loop always makes progress.
fn wrap_ranges(chars: &[char], width: usize) -> Vec<(usize, usize)> {
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
fn wrap_logical(
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
fn locate(rows: &[(usize, usize)], pos: usize) -> (usize, usize) {
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
    use crossterm::event::{KeyCode, KeyModifiers};

    use super::*;

    #[test]
    fn altgr_char_is_typed_not_swallowed() {
        // German keyboards emit `\` as AltGr (Control + Alt); it must insert.
        let mut area = TextArea::new("C:");
        let altgr = KeyEvent::new(
            KeyCode::Char('\\'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );
        assert!(area.handle_key(altgr));
        assert_eq!(area.text(), "C:\\");

        // A genuine Ctrl chord still does not type a character.
        let mut area = TextArea::new("C:");
        let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(!area.handle_key(ctrl_s));
        assert_eq!(area.text(), "C:");
    }

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

    #[test]
    fn enter_inserts_newline() {
        let mut area = TextArea::new("ab");
        area.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(area.text(), "ab\n");
    }

    #[test]
    fn max_len_blocks_typing_past_the_limit() {
        let mut area = TextArea::new("ab").max_len(3);
        area.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert_eq!(area.text(), "abc");
        area.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(area.text(), "abc");
    }

    #[test]
    fn backspace_delegates_to_the_shared_edit_core() {
        let mut area = TextArea::new("ab");
        area.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(area.text(), "a");
    }

    #[test]
    fn shift_left_selects_and_typing_replaces_via_the_core() {
        let mut area = TextArea::new("abc");
        area.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
        assert_eq!(area.cursor.selection(), Some((2, 3)));
        area.handle_key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE));
        assert_eq!(area.text(), "abX");
    }

    #[test]
    fn ctrl_a_selects_all_then_backspace_clears() {
        let mut area = TextArea::new("line 1\nline 2");
        area.handle_key(KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL,
        ));
        assert_eq!(area.cursor.selection(), Some((0, 13)));
        area.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(area.text(), "");
    }
}
