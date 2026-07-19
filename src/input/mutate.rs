//! Primitive buffer mutations under a caret: inserting, replacing a
//! selection and deleting a range.

use super::TextCursor;

/// Replaces the active selection - or, with none, inserts at the caret - with
/// `s`, leaving the caret after the inserted text and no selection.
///
/// The seam every host mutation goes through, so no stale anchor outlives an
/// edit.
///
/// # Examples
///
/// ```
/// use ratada::input::{TextCursor, replace_selection};
///
/// let mut text = String::from("hello");
/// let mut cursor = TextCursor { pos: 5, anchor: Some(1) };
/// replace_selection(&mut text, &mut cursor, "i");
/// assert_eq!((text.as_str(), cursor.pos), ("hi", 2));
/// ```
pub fn replace_selection(text: &mut String, cursor: &mut TextCursor, s: &str) {
    let mut chars: Vec<char> = text.chars().collect();
    delete_selection(&mut chars, cursor);
    cursor.anchor = None;
    let mut at = cursor.pos.min(chars.len());
    for ch in s.chars() {
        chars.insert(at, ch);
        at += 1;
    }
    cursor.pos = at;
    *text = chars.into_iter().collect();
}

/// Inserts `to_insert` at the caret, replacing an active selection first.
///
/// An alias of [`replace_selection`] that reads right at an insertion site.
pub fn insert_str(text: &mut String, cursor: &mut TextCursor, to_insert: &str) {
    replace_selection(text, cursor, to_insert);
}

/// The selected substring, or `None` when nothing is selected.
#[must_use]
pub fn selected_text(text: &str, cursor: &TextCursor) -> Option<String> {
    let (start, end) = cursor.selection()?;
    let chars: Vec<char> = text.chars().collect();
    let end = end.min(chars.len());
    let start = start.min(end);
    Some(chars[start..end].iter().collect())
}

pub(super) fn delete_range(
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
/// Moves the caret to `to`, extending the selection when `extend` is set (an
/// anchor is dropped there on the first extend) or clearing it otherwise.
pub(crate) fn move_caret(cursor: &mut TextCursor, to: usize, extend: bool) {
    if extend {
        cursor.anchor.get_or_insert(cursor.pos);
    } else {
        cursor.anchor = None;
    }
    cursor.pos = to;
}

/// Inserts `ch` at the caret, first replacing any selection and respecting
/// `max_len`.
pub(crate) fn insert_char(
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

/// Deletes the selection, or the char before the caret when there is none.
pub(crate) fn backspace(chars: &mut Vec<char>, cursor: &mut TextCursor) {
    if delete_selection(chars, cursor) {
        return;
    }
    if cursor.pos > 0 {
        chars.remove(cursor.pos - 1);
        cursor.pos -= 1;
    }
}

/// Deletes the selection, or the char at the caret when there is none.
pub(crate) fn delete_forward(chars: &mut Vec<char>, cursor: &mut TextCursor) {
    if delete_selection(chars, cursor) {
        return;
    }
    if cursor.pos < chars.len() {
        chars.remove(cursor.pos);
    }
}

/// Deletes the current selection if any, returning whether one was removed.
pub(crate) fn delete_selection(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
) -> bool {
    let Some((start, end)) = cursor.selection() else {
        return false;
    };
    chars.drain(start..end);
    cursor.pos = start;
    cursor.anchor = None;
    true
}
