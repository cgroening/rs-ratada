//! The display mode: the layout/chrome axis of the UI.
//!
//! [`Mode`] is independent of the color [`Theme`](super::Theme): `Minimal` keeps
//! the UI compact and frameless, `Fancy` adds framed chrome, a status bar and
//! richer widgets, and `Panels` is a borderless layout whose regions are
//! separated by different background colors. Widgets read the mode from the
//! [`Skin`](super::Skin).

use serde::{Deserialize, Serialize};

/// The layout/chrome variant: compact `Minimal`, framed `Fancy`, or the
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
    Fancy,
    /// Borderless: a header panel, a sidebar plus content columns and a status
    /// panel, each region set apart by its own background color.
    Panels,
}

impl Mode {
    /// All modes; the order in which the `F2` switch cycles them.
    pub const ALL: [Mode; 3] = [Mode::Minimal, Mode::Fancy, Mode::Panels];

    /// The next mode in [`Mode::ALL`], wrapping after the last.
    #[must_use]
    pub fn next(self) -> Mode {
        match self {
            Mode::Minimal => Mode::Fancy,
            Mode::Fancy => Mode::Panels,
            Mode::Panels => Mode::Minimal,
        }
    }

    /// Whether this is the `Fancy` mode (the one with framed chrome).
    pub fn is_fancy(self) -> bool {
        matches!(self, Mode::Fancy)
    }

    /// Whether this is the borderless `Panels` mode.
    pub fn is_panels(self) -> bool {
        matches!(self, Mode::Panels)
    }

    /// A short human-readable label, e.g. for status messages.
    pub fn label(self) -> &'static str {
        match self {
            Mode::Minimal => "Minimal",
            Mode::Fancy => "Fancy",
            Mode::Panels => "Panels",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_cycles_through_all_and_wraps() {
        assert_eq!(Mode::Minimal.next(), Mode::Fancy);
        assert_eq!(Mode::Fancy.next(), Mode::Panels);
        assert_eq!(Mode::Panels.next(), Mode::Minimal);
    }

    #[test]
    fn is_fancy_only_for_fancy() {
        assert!(Mode::Fancy.is_fancy());
        assert!(!Mode::Minimal.is_fancy());
        assert!(!Mode::Panels.is_fancy());
    }

    #[test]
    fn is_panels_only_for_panels() {
        assert!(Mode::Panels.is_panels());
        assert!(!Mode::Minimal.is_panels());
        assert!(!Mode::Fancy.is_panels());
    }
}
