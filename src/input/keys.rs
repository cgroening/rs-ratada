//! Classifying a key press: command chord versus plain typed character.
//!
//! The `AltGr` distinction lives here. crossterm reports `AltGr` as
//! `Control + Alt`, so a naive CONTROL check would swallow the characters it
//! types - which is why both predicates compare modifiers exactly.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Whether `key` is a Ctrl **command** chord rather than a typed character.
///
/// A command requires Control *without* Alt: on many keyboards (e.g. German)
/// `AltGr` is reported as `Control + Alt` and produces real characters (`\`,
/// `@`, `[`, `]`, `|`, ...), so those must type, not be swallowed as a chord.
///
/// # Examples
///
/// ```
/// use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
/// use ratada::input::is_command;
///
/// let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
/// assert!(is_command(ctrl_s));
///
/// let alt_gr = KeyEvent::new(
///     KeyCode::Char('\\'),
///     KeyModifiers::CONTROL | KeyModifiers::ALT,
/// );
/// assert!(!is_command(alt_gr));
/// ```
#[must_use]
pub fn is_command(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
}

/// Whether `key` is a bare printable character: one that has to reach a text
/// field, or trigger a plain single-letter binding, rather than a chord.
///
/// The counterpart of [`is_command`], and deliberately stricter than its
/// negation: this also excludes `AltGr` (`Control + Alt`) and plain `Alt`,
/// which do produce characters but never a bare one. A widget matching a plain
/// letter (`y` to confirm, `j` to move) must gate on this, or both `Ctrl+Y`
/// and `AltGr+Y` silently answer the dialog. `Shift` stays allowed: it is what
/// makes the character uppercase.
///
/// # Examples
///
/// ```
/// use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
/// use ratada::input::is_bare_character;
///
/// let plain = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
/// assert!(is_bare_character(plain));
///
/// // Shift only decides the character's case.
/// let shifted = KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT);
/// assert!(is_bare_character(shifted));
///
/// let ctrl_y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL);
/// assert!(!is_bare_character(ctrl_y));
///
/// let alt_gr = KeyEvent::new(
///     KeyCode::Char('@'),
///     KeyModifiers::CONTROL | KeyModifiers::ALT,
/// );
/// assert!(!is_bare_character(alt_gr));
///
/// // Alt alone is a chord too, and it is the only case the `Control` half of
/// // the check does not already cover.
/// let alt_y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::ALT);
/// assert!(!is_bare_character(alt_y));
///
/// // Not a character at all.
/// let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
/// assert!(!is_bare_character(enter));
/// ```
#[must_use]
pub fn is_bare_character(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char(_))
        && !key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
}
