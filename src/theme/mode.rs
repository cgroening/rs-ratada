//! The display mode: the layout/chrome axis of the UI.
//!
//! [`Mode`] is independent of the color [`Theme`](super::Theme): `Minimal` keeps
//! the UI compact and frameless, `Boxed` adds framed chrome, a status bar and
//! richer widgets, and `Panels` is a borderless layout whose regions are
//! separated by different background colors. Widgets read the mode from the
//! [`Skin`](super::Skin).

use serde::{Deserialize, Serialize};

/// The layout/chrome variant: compact `Minimal`, framed `Boxed`, or the
/// borderless, background-separated `Panels`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Compact, frameless, dimmed secondary text.
    Minimal,
    /// Framed header, dedicated status bar and richer widgets.
    #[default]
    #[serde(alias = "fancy")]
    Boxed,
    /// Borderless: a header panel, a sidebar plus content columns and a status
    /// panel, each region set apart by its own background color.
    Panels,
}

impl Mode {
    /// All modes; the order in which the `F2` switch cycles them.
    pub const ALL: [Mode; 3] = [Mode::Minimal, Mode::Boxed, Mode::Panels];

    /// The next mode in [`Mode::ALL`], wrapping after the last.
    #[must_use]
    pub fn next(self) -> Mode {
        match self {
            Mode::Minimal => Mode::Boxed,
            Mode::Boxed => Mode::Panels,
            Mode::Panels => Mode::Minimal,
        }
    }

    /// Whether this is the `Boxed` mode (the one with framed chrome).
    pub fn is_boxed(self) -> bool {
        matches!(self, Mode::Boxed)
    }

    /// Whether this is the borderless `Panels` mode.
    pub fn is_panels(self) -> bool {
        matches!(self, Mode::Panels)
    }

    /// A short human-readable label, e.g. for status messages.
    pub fn label(self) -> &'static str {
        match self {
            Mode::Minimal => "Minimal",
            Mode::Boxed => "Boxed",
            Mode::Panels => "Panels",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_cycles_through_all_and_wraps() {
        assert_eq!(Mode::Minimal.next(), Mode::Boxed);
        assert_eq!(Mode::Boxed.next(), Mode::Panels);
        assert_eq!(Mode::Panels.next(), Mode::Minimal);
    }

    #[test]
    fn is_boxed_only_for_boxed() {
        assert!(Mode::Boxed.is_boxed());
        assert!(!Mode::Minimal.is_boxed());
        assert!(!Mode::Panels.is_boxed());
    }

    #[test]
    fn is_panels_only_for_panels() {
        assert!(Mode::Panels.is_panels());
        assert!(!Mode::Minimal.is_panels());
        assert!(!Mode::Boxed.is_panels());
    }
}
