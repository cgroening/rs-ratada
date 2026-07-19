//! The global hints toggle: its visibility flag, its bound chord and the
//! bindings a host advertises for it.
//!
//! Split out of the layout code because this is *state* - a per-thread
//! preference plus the chord that flips it - while the rest of the module is a
//! pure function from hints to lines. The two change for unrelated reasons.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::keymap;

thread_local! {
    /// Whether the hint footers are shown.
    ///
    /// A TUI owns one terminal and drives it from one thread, so the preference
    /// lives per thread rather than behind an atomic. It cannot be threaded
    /// through the render calls instead: [`lines`] receives only a `Color`, and
    /// the event loop that flips it has neither a `Skin` nor host state.
    static VISIBLE: Cell<bool> = const { Cell::new(true) };

    /// The chord bound to the hints toggle, or `None` once a host unbound it.
    static TOGGLE: Cell<Option<KeyEvent>> = Cell::new(Some(default_toggle_key()));
}
/// The description shown beside the hints toggle.
const TOGGLE_LABEL: &str = "toggle hints";

/// The hard quit chord, wired into `terminal::classify` and not rebindable.
const HARD_QUIT: (&str, &str) = ("ctrl+q", "force quit");

/// Whether shortcut hints are currently shown.
pub fn visible() -> bool {
    VISIBLE.with(Cell::get)
}

/// Shows or hides every hint footer at once, e.g. to restore a saved session.
pub fn set_visible(show: bool) {
    VISIBLE.with(|flag| flag.set(show));
}

/// Flips the hint visibility; what the global toggle chord does.
pub fn toggle() {
    VISIBLE.with(|flag| flag.set(!flag.get()));
}

/// The chord bound to the hints toggle out of the box: `F1`, no modifiers.
///
/// A function key rather than a `Ctrl+…` chord: it can never be text input, so
/// it stays free inside every text field and modal. `Ctrl+Q` (quit), `Ctrl+S`,
/// `Ctrl+G`, `Ctrl+H` and the editing chords are already spoken for.
pub fn default_toggle_key() -> KeyEvent {
    KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)
}

/// The chord currently toggling the hints, or `None` while it is unbound.
pub fn toggle_key() -> Option<KeyEvent> {
    TOGGLE.with(Cell::get)
}

/// Rebinds the global hints toggle, or unbinds it with `None` so the key
/// reaches the host's own `handle_key` instead. Call it before `run`.
///
/// # Examples
///
/// ```
/// use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
/// use ratada::shortcut_hints::{set_toggle_key, toggle_key};
///
/// set_toggle_key(None);
/// assert!(toggle_key().is_none());
///
/// let chord = KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE);
/// set_toggle_key(Some(chord));
/// assert_eq!(toggle_key(), Some(chord));
/// ```
pub fn set_toggle_key(key: Option<KeyEvent>) {
    TOGGLE.with(|chord| chord.set(key));
}

/// The chords the toolkit itself intercepts, as `(key, description)` tokens:
/// the hints toggle (omitted while unbound) and the hard quit.
///
/// A host appends its own conventional chords (`?`, `q`) from its keymap: only
/// it knows them, and only it notices when the user rebinds them. With the
/// hints hidden the toggle appears nowhere else on screen, so a host that
/// builds a help overlay should list these.
///
/// # Examples
///
/// ```
/// use ratada::shortcut_hints::global_bindings;
///
/// let bindings = global_bindings();
/// assert!(bindings.iter().any(|(key, _)| key == "f1"));
/// assert!(bindings.iter().any(|(key, _)| key == "ctrl+q"));
/// ```
pub fn global_bindings() -> Vec<(String, String)> {
    let mut bindings = Vec::new();
    if let Some(chord) = toggle_key() {
        bindings.push((chord_label(chord), TOGGLE_LABEL.to_string()));
    }
    bindings.push((HARD_QUIT.0.to_string(), HARD_QUIT.1.to_string()));
    bindings
}

/// Consumes `key` when it is the bound hints toggle, flipping the visibility.
///
/// `driver::run` and `overlay::popup` call this before a key reaches the host
/// or a modal's handler, so every `Screen` and every modal inherits the chord.
/// An app that drives its own event loop calls it at the top of its own key
/// dispatch — rather than matching [`toggle_key`] by hand, which is how the
/// modifier comparison gets forgotten.
///
/// Only `code` and `modifiers` are compared: `kind` and `state` vary by
/// terminal.
///
/// # Examples
///
/// ```
/// use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
/// use ratada::shortcut_hints::{consume_toggle, default_toggle_key, visible};
///
/// // In an app's own `handle_key`, before anything else:
/// assert!(consume_toggle(default_toggle_key()));
/// assert!(!visible());
///
/// // Every other key passes through untouched.
/// let other = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
/// assert!(!consume_toggle(other));
/// ```
pub fn consume_toggle(key: KeyEvent) -> bool {
    let Some(bound) = toggle_key() else {
        return false;
    };
    if key.code != bound.code || key.modifiers != bound.modifiers {
        return false;
    }
    toggle();
    true
}

/// A chord as a footer token: `"f1"`, `"ctrl+h"`, `"shift+enter"`.
///
/// Renders through [`keymap::KeyChord`], so a hint reads exactly like the chord
/// a user writes in config and a handler matches on: one rendering of a key in
/// the crate, not one per caller that can drift from the others.
pub(super) fn chord_label(key: KeyEvent) -> String {
    keymap::KeyChord::from_key(key).display()
}
