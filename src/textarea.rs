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

use super::{clipboard, input::TextCursor, style};
use crate::theme::Palette;

/// A wrapped, editable multi-line text buffer.
#[derive(Default)]
pub struct TextArea {
    text: String,
    cursor: TextCursor,
    width: Cell<usize>,
    scroll: Cell<usize>,
}

impl TextArea {
    pub fn new(initial: &str) -> Self {
        Self {
            text: initial.to_string(),
            cursor: TextCursor::at_end(initial),
            width: Cell::new(1),
            scroll: Cell::new(0),
        }
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

    /// Renders the buffer into `area`, scrolling so the caret stays visible.
    /// A block caret is shown only when `focused`.
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        palette: &Palette,
        focused: bool,
    ) {
        let width = area.width.max(1) as usize;
        let height = area.height.max(1) as usize;
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

        frame.render_widget(Paragraph::new(lines), area);
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

/// Splits `chars` into display rows of at most `width` columns, breaking on
/// newlines (which are not included in any row).
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
            let end = (start + width).min(seg_end);
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
}
