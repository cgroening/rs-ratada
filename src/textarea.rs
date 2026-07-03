//! A multi-line text area: wrapped editing with a caret, selection and
//! clipboard. Reuses [`TextCursor`] from the single-line `input` module.
//!
//! The caller handles its own control keys (e.g. `Esc`, `Ctrl+G` for an
//! external editor) before delegating editing keys here.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthChar;

use super::{chrome, clipboard, input::TextCursor, scroll, style};
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
    force_box: bool,
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

    /// Draws the area inside a rounded box in `Fancy` mode, plain otherwise.
    #[must_use]
    pub fn boxed(mut self, decor: chrome::BoxDecor) -> Self {
        self.decor = Some(decor);
        self
    }

    /// Like [`Self::boxed`] but always draws the box, regardless of the mode.
    #[must_use]
    pub fn boxed_always(mut self, decor: chrome::BoxDecor) -> Self {
        self.decor = Some(decor);
        self.force_box = true;
        self
    }

    /// Forces the plain (unframed) style, dropping any [`Self::boxed`]
    /// decoration even in `Fancy` mode.
    #[must_use]
    pub fn minimal(mut self) -> Self {
        self.decor = None;
        self.force_box = false;
        self
    }

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
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let width = self.width.get().max(1);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let mut chars: Vec<char> = self.text.chars().collect();
        let len = chars.len();
        self.cursor.pos = self.cursor.pos.min(len);

        let consumed = match key.code {
            KeyCode::Left => {
                self.move_to(self.cursor.pos.saturating_sub(1), shift);
                true
            }
            KeyCode::Right => {
                self.move_to((self.cursor.pos + 1).min(len), shift);
                true
            }
            KeyCode::Up => {
                let to = self.vertical(&chars, width, -1);
                self.move_to(to, shift);
                true
            }
            KeyCode::Down => {
                let to = self.vertical(&chars, width, 1);
                self.move_to(to, shift);
                true
            }
            KeyCode::Home => {
                let (start, _) = self.row_bounds(&chars, width);
                self.move_to(start, shift);
                true
            }
            KeyCode::End => {
                let (_, end) = self.row_bounds(&chars, width);
                self.move_to(end, shift);
                true
            }
            KeyCode::Char('a') if ctrl => {
                self.cursor.anchor = Some(0);
                self.cursor.pos = len;
                true
            }
            KeyCode::Char('c') if ctrl => {
                self.copy(&chars);
                true
            }
            KeyCode::Char('x') if ctrl => {
                self.copy(&chars);
                self.delete_selection(&mut chars);
                true
            }
            KeyCode::Char('v') if ctrl => {
                self.paste(&mut chars);
                true
            }
            KeyCode::Enter => {
                self.insert(&mut chars, '\n');
                true
            }
            KeyCode::Backspace => {
                if !self.delete_selection(&mut chars) && self.cursor.pos > 0 {
                    chars.remove(self.cursor.pos - 1);
                    self.cursor.pos -= 1;
                }
                true
            }
            KeyCode::Delete => {
                if !self.delete_selection(&mut chars)
                    && self.cursor.pos < chars.len()
                {
                    chars.remove(self.cursor.pos);
                }
                true
            }
            KeyCode::Char(ch) if !ctrl => {
                self.insert(&mut chars, ch);
                true
            }
            _ => false,
        };

        if consumed {
            self.text = chars.into_iter().collect();
        }
        consumed
    }

    /// Renders the buffer into `area`, scrolling so the caret stays visible and
    /// filling the field with the input background (active tint when `focused`).
    /// A block caret is shown only when `focused`. Wrapped in a box when
    /// decorated and in `Fancy` mode (or forced via [`Self::boxed_always`]).
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        skin: &Skin,
        focused: bool,
    ) {
        let inner = match &self.decor {
            Some(decor) if self.force_box || skin.is_fancy() => {
                chrome::framed_decor(frame, area, skin, decor, &self.badge())
            }
            _ => area,
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
        let rows = wrap(&chars, width);
        let (caret_row, caret_col) = locate(&rows, self.cursor.pos);

        let scroll =
            keep_row_visible(self.scroll.get(), caret_row, height, rows.len());
        self.scroll.set(scroll);

        let selection = self.cursor.selection();
        let cursor_style = style::bg(palette.cursor).fg(Color::Black);
        let selection_style = style::bg(palette.selection_bg);

        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(scroll)
            .take(height)
            .map(|(row_index, &(start, end))| {
                let mut spans: Vec<Span> = Vec::new();
                for (offset, ch) in chars[start..end].iter().enumerate() {
                    let index = start + offset;
                    let mut cell = Style::default();
                    if let Some((from, to)) = selection
                        && index >= from
                        && index < to
                    {
                        cell = selection_style;
                    }
                    if focused && row_index == caret_row && offset == caret_col
                    {
                        cell = cursor_style;
                    }
                    spans.push(Span::styled(ch.to_string(), cell));
                }
                if focused && row_index == caret_row && caret_col == end - start
                {
                    spans.push(Span::styled(" ".to_string(), cursor_style));
                }
                Line::from(spans)
            })
            .collect();

        // The paragraph's base style fills the whole field (including blank
        // rows) with the input background; spans override per cell.
        frame.render_widget(
            Paragraph::new(lines).style(style::bg(base_bg)),
            inner,
        );
        // A scrollbar on the right whenever the wrapped text overflows.
        scroll::render_scrollbar(frame, inner, rows.len(), scroll, height);
    }

    /// The automatic badge text: character count, or `n/max` with a limit.
    fn badge(&self) -> String {
        let count = self.text.chars().count();
        match self.max_len {
            Some(max) => format!("{count}/{max}"),
            None => count.to_string(),
        }
    }

    fn move_to(&mut self, pos: usize, extend: bool) {
        if extend {
            self.cursor.anchor.get_or_insert(self.cursor.pos);
        } else {
            self.cursor.anchor = None;
        }
        self.cursor.pos = pos;
    }

    fn insert(&mut self, chars: &mut Vec<char>, ch: char) {
        self.delete_selection(chars);
        if self.max_len.is_some_and(|max| chars.len() >= max) {
            return;
        }
        chars.insert(self.cursor.pos, ch);
        self.cursor.pos += 1;
        self.cursor.anchor = None;
    }

    fn delete_selection(&mut self, chars: &mut Vec<char>) -> bool {
        let Some((start, end)) = self.cursor.selection() else {
            return false;
        };
        chars.drain(start..end);
        self.cursor.pos = start;
        self.cursor.anchor = None;
        true
    }

    fn copy(&self, chars: &[char]) {
        if let Some((start, end)) = self.cursor.selection() {
            let text: String = chars[start..end].iter().collect();
            clipboard::copy(&text);
        }
    }

    fn paste(&mut self, chars: &mut Vec<char>) {
        let Some(text) = clipboard::paste() else {
            return;
        };
        self.delete_selection(chars);
        for ch in text.chars().filter(|ch| *ch == '\n' || !ch.is_control()) {
            if self.max_len.is_some_and(|max| chars.len() >= max) {
                break;
            }
            chars.insert(self.cursor.pos, ch);
            self.cursor.pos += 1;
        }
        self.cursor.anchor = None;
    }

    fn row_bounds(&self, chars: &[char], width: usize) -> (usize, usize) {
        let rows = wrap(chars, width);
        let (row, _) = locate(&rows, self.cursor.pos);
        rows[row]
    }

    fn vertical(&self, chars: &[char], width: usize, delta: isize) -> usize {
        let rows = wrap(chars, width);
        let (row, col) = locate(&rows, self.cursor.pos);
        let target = (row as isize + delta).clamp(0, rows.len() as isize - 1);
        let (start, end) = rows[target as usize];
        start + col.min(end - start)
    }
}

/// Splits `chars` into display rows of at most `width` columns (measured by
/// [`UnicodeWidthChar`], so wide glyphs count as two), breaking on newlines
/// (which are not included in any row). A single glyph wider than `width` still
/// gets its own row, so the loop always makes progress.
fn wrap(chars: &[char], width: usize) -> Vec<(usize, usize)> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut seg_start = 0;
    loop {
        let seg_end = chars[seg_start..]
            .iter()
            .position(|&c| c == '\n')
            .map_or(chars.len(), |offset| seg_start + offset);
        let mut start = seg_start;
        loop {
            let mut end = start;
            let mut used = 0usize;
            while end < seg_end {
                let char_width = chars[end].width().unwrap_or(0);
                if end > start && used + char_width > width {
                    break;
                }
                used += char_width;
                end += 1;
            }
            rows.push((start, end));
            if end >= seg_end {
                break;
            }
            start = end;
        }
        match chars[seg_start..].iter().position(|&c| c == '\n') {
            Some(offset) => seg_start += offset + 1,
            None => break,
        }
    }
    if rows.is_empty() {
        rows.push((0, 0));
    }
    rows
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

fn keep_row_visible(
    offset: usize,
    caret_row: usize,
    height: usize,
    total: usize,
) -> usize {
    if height == 0 || total == 0 {
        return 0;
    }
    let max_offset = total.saturating_sub(height);
    let mut offset = offset.min(max_offset);
    if caret_row < offset {
        offset = caret_row;
    } else if caret_row >= offset + height {
        offset = caret_row + 1 - height;
    }
    offset.min(max_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_breaks_on_width_and_newlines() {
        let chars: Vec<char> = "abcdef\nxy".chars().collect();
        // width 4: "abcd","ef","xy"
        assert_eq!(wrap(&chars, 4), vec![(0, 4), (4, 6), (7, 9)]);
    }

    #[test]
    fn empty_buffer_has_one_row() {
        assert_eq!(wrap(&[], 4), vec![(0, 0)]);
    }

    #[test]
    fn wrap_measures_display_width_of_wide_chars() {
        // '世'/'界' are width-2; at width 3 only one wide glyph fits per row,
        // then the narrow 'a' joins the second row.
        let chars: Vec<char> = "世界a".chars().collect();
        assert_eq!(wrap(&chars, 3), vec![(0, 1), (1, 3)]);
    }

    #[test]
    fn trailing_newline_adds_empty_row() {
        let chars: Vec<char> = "ab\n".chars().collect();
        assert_eq!(wrap(&chars, 4), vec![(0, 2), (3, 3)]);
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
}
