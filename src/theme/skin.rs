//! The [`Skin`]: the bundled visual context handed to every widget.
//!
//! It groups the resolved [`Palette`] (colors), the [`Glyphs`] (icon variant)
//! and the [`Mode`] (layout/chrome), so widgets take a single context parameter
//! instead of several. Framework-agnostic, like the rest of this layer.

use super::{Glyphs, Mode, Palette};

/// The visual context shared across the UI: colors, glyphs and layout mode.
#[derive(Debug, Clone, Copy)]
pub struct Skin {
    pub palette: Palette,
    pub glyphs: Glyphs,
    pub mode: Mode,
}

impl Skin {
    /// Builds a skin from its parts.
    pub fn new(palette: Palette, glyphs: Glyphs, mode: Mode) -> Self {
        Self {
            palette,
            glyphs,
            mode,
        }
    }

    /// Whether the active mode is [`Mode::Boxed`].
    pub fn is_boxed(&self) -> bool {
        self.mode.is_boxed()
    }

    /// Whether the active mode is [`Mode::Panels`].
    pub fn is_panels(&self) -> bool {
        self.mode.is_panels()
    }
}
