//! The [`Skin`]: the bundled visual context handed to every widget.
//!
//! It groups the resolved [`Palette`] (colors) and the [`Glyphs`] (icon
//! variant), so widgets take a single context parameter instead of several.
//! Framework-agnostic, like the rest of this layer.

use super::{Glyphs, Palette};

/// The visual context shared across the UI: colors and glyphs.
#[derive(Debug, Clone, Copy)]
pub struct Skin {
    pub palette: Palette,
    pub glyphs: Glyphs,
}

impl Skin {
    /// Builds a skin from its parts.
    pub fn new(palette: Palette, glyphs: Glyphs) -> Self {
        Self { palette, glyphs }
    }
}
