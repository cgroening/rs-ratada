//! The clipboard seam: copy, cut and paste, plus the sanitising a pasted
//! string goes through before it reaches a buffer.

use crossterm::event::{KeyCode, KeyEvent};

use super::{
    EditMode, TextCursor, edit::apply_edit_key, keys::is_command,
    mutate::delete_selection,
};
use crate::clipboard;

/// Handles the clipboard chords against `(text, cursor)`, returning whether the
/// key was one of them: `Ctrl+C` copies the selection, `Ctrl+X` cuts it and
/// `Ctrl+V` replaces the selection (or inserts at the caret) with the clipboard.
///
/// [`apply_edit_key`] already covers these; this is for hosts that dispatch the
/// clipboard separately from the rest of their editing keys. A paste strips
/// control characters, keeping newlines only in [`EditMode::Multiline`].
pub fn handle_clipboard(
    text: &mut String,
    cursor: &mut TextCursor,
    key: KeyEvent,
    mode: EditMode,
) -> bool {
    if !is_command(key) {
        return false;
    }
    if !matches!(key.code, KeyCode::Char('c' | 'x' | 'v')) {
        return false;
    }
    apply_edit_key(text, cursor, key, mode, None)
}

/// Inserts pasted `payload` at the caret, replacing any active selection.
///
/// Applies the same filter as a `Ctrl+V` paste: control characters are dropped,
/// and newlines survive only in [`EditMode::Multiline`], so a paste into a
/// single-line field stays on one line; `max_len` caps the result. This is the
/// seam bracketed-paste routing inserts through, so terminal-native paste and
/// `Ctrl+V` share one insertion path.
///
/// # Examples
///
/// ```
/// use ratada::input::{EditMode, TextCursor, paste_text};
///
/// let mut text = String::from("ab");
/// let mut cursor = TextCursor::at_end(&text);
/// paste_text(&mut text, &mut cursor, EditMode::SingleLine, None, "c\nd");
/// assert_eq!(text, "abcd");
/// ```
pub fn paste_text(
    text: &mut String,
    cursor: &mut TextCursor,
    mode: EditMode,
    max_len: Option<usize>,
    payload: &str,
) {
    let mut chars: Vec<char> = text.chars().collect();
    cursor.pos = cursor.pos.min(chars.len());
    insert_pasted(&mut chars, cursor, max_len, payload, paste_keep(mode));
    *text = chars.into_iter().collect();
}

/// The character filter a paste applies in `mode`: control characters are
/// always dropped, and newlines survive only in a multiline field, so a paste
/// into a single-line field can never break onto a second line.
pub(super) fn paste_keep(mode: EditMode) -> fn(char) -> bool {
    if mode.keeps_newlines() {
        |ch| ch == '\n' || !ch.is_control()
    } else {
        |ch| !ch.is_control()
    }
}

/// Replaces any selection, then inserts `payload`'s `keep`-passing characters at
/// the caret, respecting `max_len`.
pub(super) fn insert_pasted(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
    max_len: Option<usize>,
    payload: &str,
    keep: impl Fn(char) -> bool,
) {
    delete_selection(chars, cursor);
    for ch in payload.chars().filter(|&ch| keep(ch)) {
        if max_len.is_some_and(|max| chars.len() >= max) {
            break;
        }
        chars.insert(cursor.pos, ch);
        cursor.pos += 1;
    }
    cursor.anchor = None;
}

/// Reads the clipboard and inserts it at the caret with `mode`'s paste filter.
pub(super) fn paste_clipboard(
    chars: &mut Vec<char>,
    cursor: &mut TextCursor,
    max_len: Option<usize>,
    mode: EditMode,
) {
    let Some(text) = clipboard::paste() else {
        return;
    };
    insert_pasted(chars, cursor, max_len, &text, paste_keep(mode));
}
/// Copies the current selection (if any) to the clipboard.
pub(crate) fn copy_selection(chars: &[char], cursor: &TextCursor) {
    if let Some((start, end)) = cursor.selection() {
        let text: String = chars[start..end].iter().collect();
        clipboard::copy(&text);
    }
}
