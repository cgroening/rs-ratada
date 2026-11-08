//! The [`KeyChord`] grammar: parsing, matching and rendering a key press.
//!
//! Split out of the keymap itself: this is the *notation* - the shared spelling
//! of a key in config files, footers and help overlays - while [`super`] is
//! about binding chords to an app's actions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The highest function key the chord grammar accepts (`f1`..`f12`).
const MAX_FUNCTION_KEY: u8 = 12;

/// A parsed key chord: a key plus its `ctrl`/`alt`/`shift` modifiers.
///
/// See the [keymap docs](super#modifier-semantics) for how each modifier is
/// compared - in particular why `shift` is only significant for non-character
/// keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChord {
    code: KeyCode,
    ctrl: bool,
    alt: bool,
    shift: bool,
}

impl KeyChord {
    /// Parses a chord like `"a"`, `"G"`, `"ctrl+q"`, `"Ctrl+Q"`,
    /// `"shift+left"`, `"f2"`, `"pgup"` or `"enter"`, or `None` for an
    /// unrecognised string.
    ///
    /// The last `+`-separated token is the key; the ones before it are
    /// modifiers (`ctrl`/`control`, `alt`/`option`, `shift`), in any case. A
    /// bare letter keeps its case, so `"G"` is the shifted `g`; after `ctrl`
    /// or `alt` a letter is read case-insensitively and its shift must be
    /// written (`"ctrl+shift+q"`), so a label like `Ctrl+Q` reads as typed.
    #[must_use]
    pub fn parse(text: &str) -> Option<KeyChord> {
        let parts: Vec<&str> = text
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect();
        let (code_token, modifiers) = parts.split_last()?;
        let mut chord = KeyChord {
            code: code_from_token(code_token)?,
            ctrl: false,
            alt: false,
            shift: false,
        };
        for modifier in modifiers {
            match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => chord.ctrl = true,
                "alt" | "option" => chord.alt = true,
                "shift" => chord.shift = true,
                _ => return None,
            }
        }
        // After `Ctrl`/`Alt` a letter is read the way a label writes it:
        // `Ctrl+Q` is Ctrl and q, the shifted letter needs `Shift+` spelled
        // out (`Ctrl+Shift+Q`). So what a footer shows can be typed into a
        // `[keys]` table as it stands.
        if (chord.ctrl || chord.alt)
            && let KeyCode::Char(letter) = chord.code
            && letter.is_alphabetic()
        {
            chord.code = KeyCode::Char(if chord.shift {
                letter.to_ascii_uppercase()
            } else {
                letter.to_ascii_lowercase()
            });
            chord.shift = false;
        }
        Some(chord)
    }

    /// The chord a pressed key *is*, for rendering a live key as a label.
    ///
    /// The inverse of [`KeyChord::matches`] in the sense that the result always
    /// matches `key`: a character keeps its case and drops `shift` (the case
    /// carries it), any other key keeps `shift` as pressed.
    #[must_use]
    pub fn from_key(key: KeyEvent) -> Self {
        KeyChord {
            code: key.code,
            ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
            shift: !matches!(key.code, KeyCode::Char(_))
                && key.modifiers.contains(KeyModifiers::SHIFT),
        }
    }

    /// Whether `key` triggers this chord.
    ///
    /// `ctrl`/`alt` must match exactly, so `AltGr` (`Control + Alt`) never
    /// stands in for `Control`. `shift` is compared only for a non-character
    /// key; for a character the case already carries it.
    #[must_use]
    pub fn matches(&self, key: &KeyEvent) -> bool {
        if self.code != key.code
            || self.ctrl != key.modifiers.contains(KeyModifiers::CONTROL)
            || self.alt != key.modifiers.contains(KeyModifiers::ALT)
        {
            return false;
        }
        if matches!(self.code, KeyCode::Char(_)) {
            return true;
        }
        self.shift == key.modifiers.contains(KeyModifiers::SHIFT)
    }

    /// The chord's display string in config syntax, e.g. `ctrl+q`,
    /// `shift+left`, `f2`, `G`, `ctrl+shift+q`.
    ///
    /// This is the single rendering of a chord: the hints footer and a config
    /// file show the same text, and it round-trips - [`KeyChord::parse`] of the
    /// result yields the same chord. That contract is why the case is preserved
    /// (`G` must not become `g`, or it collides with the `g` binding) and why
    /// the tokens are the terse ones (`del`, `pgup`) the grammar accepts.
    #[must_use]
    pub fn display(&self) -> String {
        let mut text = String::new();
        if self.ctrl {
            text.push_str("ctrl+");
        }
        if self.alt {
            text.push_str("alt+");
        }
        // A character's shift lives in its case; only after `ctrl`/`alt`,
        // where a letter is read case-insensitively, is it spelled out
        // (`ctrl+shift+q`).
        let shifted_letter = (self.ctrl || self.alt)
            && matches!(self.code, KeyCode::Char(c) if c.is_uppercase());
        if (self.shift && !matches!(self.code, KeyCode::Char(_)))
            || shifted_letter
        {
            text.push_str("shift+");
        }
        let token = token_for_code(self.code);
        if shifted_letter {
            text.push_str(&token.to_lowercase());
        } else {
            text.push_str(&token);
        }
        text
    }

    /// The chord as a hint, a help screen or a README shows it to a reader:
    /// `Ctrl+S`, `Shift+Tab`, `Alt+↑`, `Enter`, `PgUp`, `F2`, `Space`, and a
    /// bare character as it is typed (`g`, `G`, `?`). A letter in a chord with
    /// `Ctrl` or `Alt` is written upper-case, its shift spelled out
    /// (`Ctrl+Shift+S`). Not config syntax: [`KeyChord::display`] is that.
    #[must_use]
    pub fn label(&self) -> String {
        let mut text = String::new();
        if self.ctrl {
            text.push_str("Ctrl+");
        }
        if self.alt {
            text.push_str("Alt+");
        }
        let modified = self.ctrl || self.alt;
        let key = match self.code {
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char(ch) if modified => {
                if ch.is_uppercase() || self.shift {
                    text.push_str("Shift+");
                }
                ch.to_uppercase().to_string()
            }
            KeyCode::Char(ch) => ch.to_string(),
            KeyCode::BackTab => {
                text.push_str("Shift+");
                "Tab".to_string()
            }
            other => {
                if self.shift {
                    text.push_str("Shift+");
                }
                label_for_code(other)
            }
        };
        text.push_str(&key);
        text
    }

    /// A key press that triggers this chord: the inverse of
    /// [`KeyChord::from_key`], and always accepted by [`KeyChord::matches`].
    ///
    /// For synthesizing a press - replaying a chord picked from a command
    /// palette, or asserting in a test that a binding resolves back to its
    /// action - without rebuilding the modifiers by hand.
    #[must_use]
    pub fn to_key(&self) -> KeyEvent {
        let mut modifiers = KeyModifiers::NONE;
        if self.ctrl {
            modifiers |= KeyModifiers::CONTROL;
        }
        if self.alt {
            modifiers |= KeyModifiers::ALT;
        }
        if self.shift {
            modifiers |= KeyModifiers::SHIFT;
        }
        KeyEvent::new(self.code, modifiers)
    }
}

/// How a reader sees a named key (see [`KeyChord::label`]).
fn label_for_code(code: KeyCode) -> String {
    match code {
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Up => "\u{2191}".to_string(),
        KeyCode::Down => "\u{2193}".to_string(),
        KeyCode::Left => "\u{2190}".to_string(),
        KeyCode::Right => "\u{2192}".to_string(),
        KeyCode::PageUp => "PgUp".to_string(),
        KeyCode::PageDown => "PgDn".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::Delete => "Del".to_string(),
        KeyCode::F(number) => format!("F{number}"),
        other => token_for_code(other),
    }
}

/// Parses a single key token (no modifiers) into a [`KeyCode`].
fn code_from_token(token: &str) -> Option<KeyCode> {
    let lower = token.to_ascii_lowercase();
    let code = match lower.as_str() {
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "space" => KeyCode::Char(' '),
        "backspace" => KeyCode::Backspace,
        "insert" | "ins" => KeyCode::Insert,
        "up" | "\u{2191}" => KeyCode::Up,
        "down" | "\u{2193}" => KeyCode::Down,
        "left" | "\u{2190}" => KeyCode::Left,
        "right" | "\u{2192}" => KeyCode::Right,
        "pgup" | "pageup" => KeyCode::PageUp,
        "pgdn" | "pgdown" | "pagedown" => KeyCode::PageDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "del" | "delete" => KeyCode::Delete,
        _ => return function_or_char(token, &lower),
    };
    Some(code)
}

/// Resolves an `fN` function key or a single character (preserving case).
fn function_or_char(token: &str, lower: &str) -> Option<KeyCode> {
    if let Some(digits) = lower.strip_prefix('f')
        && let Ok(number) = digits.parse::<u8>()
        && (1..=MAX_FUNCTION_KEY).contains(&number)
    {
        return Some(KeyCode::F(number));
    }
    let mut chars = token.chars();
    let first = chars.next()?;
    // More than one remaining char is a word, not a key.
    if chars.next().is_some() {
        return None;
    }
    Some(KeyCode::Char(first))
}

/// The display token for a key code, the inverse of [`code_from_token`].
fn token_for_code(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(ch) => ch.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::BackTab => "backtab".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Insert => "insert".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::PageUp => "pgup".to_string(),
        KeyCode::PageDown => "pgdn".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::Delete => "del".to_string(),
        KeyCode::F(number) => format!("f{number}"),
        _ => "?".to_string(),
    }
}
