//! `F5`: reload the configuration, like a restart.
//!
//! The TUI never rebuilds its services itself. On `F5` it checks the
//! configuration through a function of the app: with any problem it shows
//! the report and nothing changes; when the configuration is clean,
//! [`request`] returns the reload [`signal`] as an error. It unwinds every
//! screen like the quit chord (the session is saved on the way) up to the
//! composition root, which recognises it with [`is_reload`], builds
//! everything again from the fresh configuration and starts the TUI on the
//! saved view.

use std::{fmt, io};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;

use crate::{modal, terminal::Tui, theme::Skin};

/// The key that reloads the configuration in every app.
pub const RELOAD_KEY: KeyCode = KeyCode::F(5);

/// The `(key, description)` hint for the help overlay and the footer.
pub const RELOAD_HINT: (&str, &str) = ("f5", "reload config");

/// Whether `key` is `F5` without a modifier.
pub fn is_reload_key(key: KeyEvent) -> bool {
    key.code == RELOAD_KEY && key.modifiers == KeyModifiers::NONE
}

/// Why a configuration cannot be reloaded, as the user should read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigProblems {
    /// The title of the report, e.g. "config.toml cannot be read".
    pub title: String,
    /// The problems, one per line.
    pub body: String,
}

/// Asks for a reload after the app checked its configuration (`checked`).
/// With problems it shows them and returns `Ok`: the session goes on
/// unchanged. A clean configuration returns the reload [`signal`] as the
/// error, to be passed up to the composition root.
///
/// # Errors
///
/// The reload signal on a clean configuration; terminal I/O errors of the
/// report.
pub fn request(
    tui: &mut Tui,
    skin: &Skin,
    checked: Result<(), ConfigProblems>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<()> {
    match checked {
        Ok(()) => Err(signal()),
        Err(problems) => {
            let body = format!(
                "{}\n\nNothing was reloaded.",
                problems.body.trim_end()
            );
            modal::message(tui, skin, &problems.title, &body, render_bg)?;
            Ok(())
        }
    }
}

/// The reload signal: an I/O error that carries [`ReloadSignal`], so it
/// travels the same `?` path as every other error until the composition
/// root catches it.
pub fn signal() -> io::Error {
    io::Error::other(ReloadSignal)
}

/// Whether `err` is the reload [`signal`].
pub fn is_reload(err: &io::Error) -> bool {
    err.get_ref()
        .is_some_and(<dyn std::error::Error + Send + Sync>::is::<ReloadSignal>)
}

/// The payload of the reload [`signal`].
#[derive(Debug)]
pub struct ReloadSignal;

impl fmt::Display for ReloadSignal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "reload the configuration")
    }
}

impl std::error::Error for ReloadSignal {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{
        clear_scripted_input, script_keys, scripted_remaining,
    };
    use crate::theme::{
        ColorOverrides, GlyphVariant, Glyphs, Palette, ThemeRegistry,
    };

    fn skin() -> Skin {
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
        )
    }

    #[test]
    fn f5_is_the_reload_key() {
        assert!(is_reload_key(KeyEvent::new(
            KeyCode::F(5),
            KeyModifiers::NONE
        )));
        assert!(!is_reload_key(KeyEvent::new(
            KeyCode::Char('r'),
            KeyModifiers::CONTROL
        )));
        assert!(!is_reload_key(KeyEvent::new(
            KeyCode::F(5),
            KeyModifiers::SHIFT
        )));
    }

    #[test]
    fn a_clean_config_raises_the_signal() {
        let mut tui = Tui::for_test(60, 20).expect("a test terminal");
        let error =
            request(&mut tui, &skin(), Ok(()), |_| {}).expect_err("the signal");
        assert!(is_reload(&error));
        assert!(!is_reload(&io::Error::other("something else")));
    }

    #[test]
    fn problems_are_shown_and_nothing_is_reloaded() {
        script_keys(vec![KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)]);
        let mut tui = Tui::for_test(60, 20).expect("a test terminal");
        let problems = ConfigProblems {
            title: "config.toml".into(),
            body: "unknown key `x`".into(),
        };
        let result = request(&mut tui, &skin(), Err(problems), |_| {});
        let left = scripted_remaining();
        clear_scripted_input();
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(left, Some(0), "the report was shown");
    }
}
