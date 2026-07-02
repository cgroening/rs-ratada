//! Shared single-line text editing: one caret with an optional selection
//! anchor over a `String`. This is the single source of editing behaviour for
//! every text field, so shortcuts stay consistent.
//!
//! The editor handles only editing keys. A field's control keys (`Esc`, a
//! confirming `Enter`, other chords) belong to the caller and must be handled
//! before delegating here.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{chrome, clipboard, style};
use crate::theme::{Palette, Skin};

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

/// A single-line input field bundling text with its caret, an optional length
/// limit and an optional boxed decoration.
#[derive(Debug, Clone, Default)]
pub struct InputField {
    pub text: String,
    pub cursor: TextCursor,
    max_len: Option<usize>,
    decor: Option<chrome::BoxDecor>,
    force_box: bool,
}

impl InputField {
    /// Creates a field pre-filled with `initial`, caret at the end.
    pub fn new(initial: &str) -> Self {
        Self {
            text: initial.to_string(),
            cursor: TextCursor::at_end(initial),
            ..Self::default()
        }
    }

    /// Limits the field to `max` characters (enforced on typing and paste) and
    /// feeds the `n/max` badge in the boxed variant.
    #[must_use]
    pub fn max_len(mut self, max: usize) -> Self {
        self.max_len = Some(max);
        self
    }

    /// Draws the field inside a rounded box in `Fancy` mode (see [`Skin`]),
    /// plain otherwise. See [`chrome::BoxDecor`] for caption/badge.
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

    /// Handles one editing key; returns whether it was consumed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        apply_edit_key(&mut self.text, &mut self.cursor, key, self.max_len)
    }

    /// The current text.
    pub fn value(&self) -> &str {
        &self.text
    }

    /// Renders the field as a single horizontally scrolling, background-filled
    /// line for embedding into a larger layout. `focused` picks the active
    /// background and shows the block caret.
    pub fn render_line(
        &self,
        palette: &Palette,
        width: usize,
        focused: bool,
    ) -> Line<'static> {
        render_line(&self.text, &self.cursor, palette, width, focused)
    }

    /// Renders the field into `area`: a filled background (active tint when
    /// `focused`) plus the text line, wrapped in a box when decorated and in
    /// `Fancy` mode (or forced via [`Self::boxed_always`]).
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
        let line =
            self.render_line(&skin.palette, inner.width as usize, focused);
        frame.render_widget(Paragraph::new(line), inner);
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

/// Applies one editing key to `text`/`cursor`, respecting an optional
/// `max_len`. Returns whether it was consumed.
pub fn apply_edit_key(
    text: &mut String,
    cursor: &mut TextCursor,
    key: KeyEvent,
    max_len: Option<usize>,
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
            paste(&mut chars, cursor, max_len);
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
            insert_char(&mut chars, cursor, ch, max_len);
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
/// visible, filled with the input background (active tint when `focused`), with
/// a block caret (only when `focused`) and selection highlight. The line is
/// padded to `width` so the field reads as a solid background strip.
pub fn render_line(
    text: &str,
    cursor: &TextCursor,
    palette: &Palette,
    width: usize,
    focused: bool,
) -> Line<'static> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let pos = cursor.pos.min(len);
    let width = width.max(1);
    let start = if pos >= width { pos + 1 - width } else { 0 };
    let selection = cursor.selection();
    let base_bg = if focused {
        palette.input_bg_active
    } else {
        palette.input_bg
    };
    let base = style::bg(base_bg);
    let cursor_style = Style::default()
        .bg(style::to_ratatui(palette.cursor))
        .fg(Color::Black);
    let selection_style = style::bg(palette.selection_bg);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for (index, ch) in chars.iter().enumerate().skip(start).take(width) {
        let mut style = base;
        if let Some((from, to)) = selection
            && index >= from
            && index < to
        {
            style = selection_style;
        }
        if focused && index == pos {
            style = cursor_style;
        }
        spans.push(Span::styled(ch.to_string(), style));
        used += 1;
    }
    if focused && pos >= len && used < width {
        spans.push(Span::styled(" ".to_string(), cursor_style));
        used += 1;
    }
    if used < width {
        spans.push(Span::styled(" ".repeat(width - used), base));
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

fn insert_char(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
    ch: char,
    max_len: Option<usize>,
) {
    delete_selection(chars, cursor);
    if max_len.is_some_and(|max| chars.len() >= max) {
        return;
    }
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

fn paste(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
    max_len: Option<usize>,
) {
    let Some(text) = clipboard::paste() else {
        return;
    };
    delete_selection(chars, cursor);
    for ch in text.chars().filter(|ch| !ch.is_control()) {
        if max_len.is_some_and(|max| chars.len() >= max) {
            break;
        }
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

    #[test]
    fn max_len_blocks_typing_past_the_limit() {
        let mut field = InputField::new("ab").max_len(3);
        field.handle_key(press(KeyCode::Char('c')));
        assert_eq!(field.value(), "abc");
        // The fourth character is rejected.
        field.handle_key(press(KeyCode::Char('d')));
        assert_eq!(field.value(), "abc");
    }
}
