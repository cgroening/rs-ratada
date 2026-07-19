//! The shared edit-key engine: one caret with an optional selection anchor,
//! driven by [`EditMode`] geometry so single-line and multi-line fields carry
//! the same shortcuts.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{
    EditMode, TextCursor,
    clip::{copy_selection, paste_clipboard},
    keys::is_command,
    mutate::{
        backspace, delete_forward, delete_range, delete_selection, insert_char,
        move_caret,
    },
};
use crate::textarea;

/// Applies one editing key to `text`/`cursor`, returning whether it was
/// consumed (so the caller stops handling it). Steering keys the caller owns -
/// `Esc`, a confirming `Enter` in [`EditMode::SingleLine`], other chords - must
/// be handled before delegating here.
///
/// A `Shift`-modified motion key (arrows, `Home`/`End`) extends the selection;
/// the same key without `Shift` moves and clears it. `Ctrl+A` selects the whole
/// value, `Ctrl+U`/`Ctrl+K` delete from the line start to the caret / from the
/// caret to the line end, and `Ctrl+C`/`X`/`V` drive the clipboard. Typing,
/// `Backspace` and `Delete` replace an active selection.
///
/// In [`EditMode::Multiline`] the line-oriented keys act on the **display**
/// line (the wrap at `width`), `Up`/`Down` walk display lines,
/// `PageUp`/`PageDown` move by the viewport `height` in display rows (clamped at
/// the edges), `Enter` inserts a newline and a paste keeps its line breaks.
/// `max_len` caps the character count on typing and paste.
///
/// # Examples
///
/// ```
/// use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
/// use ratada::input::{EditMode, TextCursor, apply_edit_key};
///
/// let mut text = String::from("ab");
/// let mut cursor = TextCursor::at_end(&text);
/// let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
/// assert!(apply_edit_key(
///     &mut text,
///     &mut cursor,
///     key,
///     EditMode::SingleLine,
///     None,
/// ));
/// assert_eq!(text, "a");
/// ```
pub fn apply_edit_key(
    text: &mut String,
    cursor: &mut TextCursor,
    key: KeyEvent,
    mode: EditMode,
    max_len: Option<usize>,
) -> bool {
    let ctrl = is_command(key);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let mut chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    cursor.pos = cursor.pos.min(len);

    if let Some(target) = motion_target(text, cursor.pos, key, mode) {
        move_caret(cursor, target, shift);
        return true;
    }

    let consumed = match key.code {
        KeyCode::Char('a') if ctrl => {
            cursor.select_all(len);
            true
        }
        KeyCode::Char('u') if ctrl => {
            let start = line_start(text, cursor.pos, mode);
            delete_range(&mut chars, cursor, start, cursor.pos);
            true
        }
        KeyCode::Char('k') if ctrl => {
            let end = line_end(text, cursor.pos, mode);
            delete_range(&mut chars, cursor, cursor.pos, end);
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
            paste_clipboard(&mut chars, cursor, max_len, mode);
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
        KeyCode::Enter if mode.keeps_newlines() => {
            insert_char(&mut chars, cursor, '\n', max_len);
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

/// The motion target for a navigation key, or `None` when `key` is not one. A
/// vertical move that cannot go further returns the unchanged `pos`, so a plain
/// `Up`/`Down` still clears the selection.
pub(super) fn motion_target(
    text: &str,
    pos: usize,
    key: KeyEvent,
    mode: EditMode,
) -> Option<usize> {
    let multiline = mode.is_wrapping();
    let target = match key.code {
        KeyCode::Left => pos.saturating_sub(1),
        KeyCode::Right => (pos + 1).min(text.chars().count()),
        KeyCode::Home => line_start(text, pos, mode),
        KeyCode::End => line_end(text, pos, mode),
        KeyCode::Up if multiline => display_line_target(text, pos, mode, -1),
        KeyCode::Down if multiline => display_line_target(text, pos, mode, 1),
        KeyCode::PageUp if multiline => {
            display_line_target(text, pos, mode, -page(mode))
        }
        KeyCode::PageDown if multiline => {
            display_line_target(text, pos, mode, page(mode))
        }
        _ => return None,
    };
    Some(target)
}

/// The page step for `PageUp`/`PageDown`: the viewport height in display rows
/// (at least one). A single line has no page, so it reports one row.
pub(super) fn page(mode: EditMode) -> isize {
    match mode {
        EditMode::Multiline { height, .. }
        | EditMode::Wrapped { height, .. } => height.max(1) as isize,
        EditMode::SingleLine => 1,
    }
}

/// The caret index for `Home`: the value's start, or the display line's start.
pub(super) fn line_start(text: &str, pos: usize, mode: EditMode) -> usize {
    let Some(width) = mode.wrap_width() else {
        return 0;
    };
    let lines = textarea::wrap_offsets(text, width);
    let (line, _) =
        textarea::cursor_to_display(&lines, text.chars().count(), pos);
    textarea::display_to_cursor(&lines, line, 0)
}

/// The caret index for `End`: the value's end, or the display line's end.
pub(super) fn line_end(text: &str, pos: usize, mode: EditMode) -> usize {
    let Some(width) = mode.wrap_width() else {
        return text.chars().count();
    };
    let lines = textarea::wrap_offsets(text, width);
    let (line, _) =
        textarea::cursor_to_display(&lines, text.chars().count(), pos);
    let col = lines[line].0.chars().count();
    textarea::display_to_cursor(&lines, line, col)
}

/// The caret index `delta` display lines away, keeping the column where
/// possible. The target line is clamped to the first/last row, so a `PageUp`
/// near the top still lands on row 0; a single step at an edge stays put (it is
/// already on the clamped row).
pub(super) fn display_line_target(
    text: &str,
    pos: usize,
    mode: EditMode,
    delta: isize,
) -> usize {
    let Some(width) = mode.wrap_width() else {
        return pos;
    };
    let lines = textarea::wrap_offsets(text, width);
    let (line, col) =
        textarea::cursor_to_display(&lines, text.chars().count(), pos);
    let target =
        (line as isize + delta).clamp(0, lines.len() as isize - 1) as usize;
    textarea::display_to_cursor(&lines, target, col)
}
