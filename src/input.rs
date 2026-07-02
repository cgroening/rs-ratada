//! Shared single-line text editing: one caret with an optional selection
//! anchor over a `String`. This is the single source of editing behaviour for
//! every text field, so shortcuts stay consistent.
//!
//! The editor handles only editing keys. A field's control keys (`Esc`, a
//! confirming `Enter`, other chords) belong to the caller and must be handled
//! before delegating here.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use super::{clipboard, style};
use crate::theme::Palette;

/// A text caret with an optional selection anchor, both as char indices.
#[derive(Debug, Clone, Default)]
pub struct TextCursor {
    pub pos: usize,
    pub anchor: Option<usize>,
}

impl TextCursor {
    /// A caret placed at the end of `text`.
    pub fn at_end(text: &str) -> Self {
        Self {
            pos: text.chars().count(),
            anchor: None,
        }
    }

    /// The current selection as an ordered `(start, end)` range, if any.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        let (start, end) = if anchor <= self.pos {
            (anchor, self.pos)
        } else {
            (self.pos, anchor)
        };
        (start != end).then_some((start, end))
    }
}

/// A single-line input field bundling text with its caret.
#[derive(Debug, Clone, Default)]
pub struct InputField {
    pub text: String,
    pub cursor: TextCursor,
}

impl InputField {
    /// Creates a field pre-filled with `initial`, caret at the end.
    pub fn new(initial: &str) -> Self {
        Self {
            text: initial.to_string(),
            cursor: TextCursor::at_end(initial),
        }
    }

    /// Handles one editing key; returns whether it was consumed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        apply_edit_key(&mut self.text, &mut self.cursor, key)
    }

    /// The current text.
    pub fn value(&self) -> &str {
        &self.text
    }

    /// Renders the field as a single horizontally scrolling line.
    pub fn render(&self, palette: &Palette, width: usize) -> Line<'static> {
        render_line(&self.text, &self.cursor, palette, width)
    }
}

/// Applies one editing key to `text`/`cursor`. Returns whether it was consumed.
pub fn apply_edit_key(
    text: &mut String,
    cursor: &mut TextCursor,
    key: KeyEvent,
) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let mut chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    cursor.pos = cursor.pos.min(len);

    let consumed = match key.code {
        KeyCode::Left => {
            move_caret(cursor, cursor.pos.saturating_sub(1), shift);
            true
        }
        KeyCode::Right => {
            move_caret(cursor, (cursor.pos + 1).min(len), shift);
            true
        }
        KeyCode::Home => {
            move_caret(cursor, 0, shift);
            true
        }
        KeyCode::End => {
            move_caret(cursor, len, shift);
            true
        }
        KeyCode::Char('a') if ctrl => {
            cursor.anchor = Some(0);
            cursor.pos = len;
            true
        }
        KeyCode::Char('u') if ctrl => {
            delete_range(&mut chars, cursor, 0, cursor.pos);
            true
        }
        KeyCode::Char('k') if ctrl => {
            delete_range(&mut chars, cursor, cursor.pos, len);
            true
        }
        KeyCode::Char('c') if ctrl => {
            copy_selection(&chars, cursor);
            true
        }
        KeyCode::Char('x') if ctrl => {
            copy_selection(&chars, cursor);
            delete_selection(&mut chars, cursor);
            true
        }
        KeyCode::Char('v') if ctrl => {
            paste(&mut chars, cursor);
            true
        }
        KeyCode::Backspace => {
            backspace(&mut chars, cursor);
            true
        }
        KeyCode::Delete => {
            delete_forward(&mut chars, cursor);
            true
        }
        KeyCode::Char(ch) if !ctrl => {
            insert_char(&mut chars, cursor, ch);
            true
        }
        _ => false,
    };

    if consumed {
        *text = chars.into_iter().collect();
    }
    consumed
}

/// Renders `text` as a single line that scrolls horizontally to keep the caret
/// visible, with a block cursor and selection highlight.
pub fn render_line(
    text: &str,
    cursor: &TextCursor,
    palette: &Palette,
    width: usize,
) -> Line<'static> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let pos = cursor.pos.min(len);
    let width = width.max(1);
    let start = if pos >= width { pos + 1 - width } else { 0 };
    let selection = cursor.selection();
    let cursor_style = Style::default()
        .bg(style::to_ratatui(palette.cursor))
        .fg(Color::Black);
    let selection_style = style::bg(palette.selection_bg);

    let mut spans: Vec<Span<'static>> = Vec::new();
    for (index, ch) in chars.iter().enumerate().skip(start).take(width) {
        let mut style = Style::default();
        if let Some((from, to)) = selection
            && index >= from
            && index < to
        {
            style = selection_style;
        }
        if index == pos {
            style = cursor_style;
        }
        spans.push(Span::styled(ch.to_string(), style));
    }
    if pos >= len && pos < start + width {
        spans.push(Span::styled(" ".to_string(), cursor_style));
    }
    Line::from(spans)
}

fn move_caret(cursor: &mut TextCursor, to: usize, extend: bool) {
    if extend {
        cursor.anchor.get_or_insert(cursor.pos);
    } else {
        cursor.anchor = None;
    }
    cursor.pos = to;
}

fn insert_char(chars: &mut Vec<char>, cursor: &mut TextCursor, ch: char) {
    delete_selection(chars, cursor);
    chars.insert(cursor.pos, ch);
    cursor.pos += 1;
    cursor.anchor = None;
}

fn backspace(chars: &mut Vec<char>, cursor: &mut TextCursor) {
    if delete_selection(chars, cursor) {
        return;
    }
    if cursor.pos > 0 {
        chars.remove(cursor.pos - 1);
        cursor.pos -= 1;
    }
}

fn delete_forward(chars: &mut Vec<char>, cursor: &mut TextCursor) {
    if delete_selection(chars, cursor) {
        return;
    }
    if cursor.pos < chars.len() {
        chars.remove(cursor.pos);
    }
}

fn delete_selection(chars: &mut Vec<char>, cursor: &mut TextCursor) -> bool {
    let Some((start, end)) = cursor.selection() else {
        return false;
    };
    chars.drain(start..end);
    cursor.pos = start;
    cursor.anchor = None;
    true
}

fn delete_range(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
    start: usize,
    end: usize,
) {
    let start = start.min(chars.len());
    let end = end.min(chars.len());
    if start >= end {
        return;
    }
    chars.drain(start..end);
    cursor.pos = start;
    cursor.anchor = None;
}

fn copy_selection(chars: &[char], cursor: &TextCursor) {
    if let Some((start, end)) = cursor.selection() {
        let text: String = chars[start..end].iter().collect();
        clipboard::copy(&text);
    }
}

fn paste(chars: &mut Vec<char>, cursor: &mut TextCursor) {
    let Some(text) = clipboard::paste() else {
        return;
    };
    delete_selection(chars, cursor);
    for ch in text.chars().filter(|ch| !ch.is_control()) {
        chars.insert(cursor.pos, ch);
        cursor.pos += 1;
    }
    cursor.anchor = None;
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut field = InputField::default();
        field.handle_key(press(KeyCode::Char('h')));
        field.handle_key(press(KeyCode::Char('i')));
        assert_eq!(field.value(), "hi");
        assert_eq!(field.cursor.pos, 2);
    }

    #[test]
    fn backspace_removes_the_previous_char() {
        let mut field = InputField::new("ab");
        field.handle_key(press(KeyCode::Backspace));
        assert_eq!(field.value(), "a");
    }

    #[test]
    fn shift_left_selects_and_typing_replaces() {
        let mut field = InputField::new("abc");
        field.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
        assert_eq!(field.cursor.selection(), Some((2, 3)));
        field.handle_key(press(KeyCode::Char('X')));
        assert_eq!(field.value(), "abX");
    }
}
