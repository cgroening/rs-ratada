//! The [`Skin`]: the bundled visual context handed to every widget.
//!
//! It groups the resolved [`Palette`] (colors) and the [`Glyphs`] (icon
//! variant), so widgets take a single context parameter instead of several.
//! Framework-agnostic, like the rest of this layer.

use super::{Glyphs, Palette};

/// The visual context shared across the UI: colors and glyphs.
#[derive(Debug, Clone, Copy)]
pub struct Skin {
    /// The resolved colors.
    pub palette: Palette,
    /// The resolved marker glyphs.
    pub glyphs: Glyphs,
}

impl Skin {
    /// Builds a skin from its parts.
    pub fn new(palette: Palette, glyphs: Glyphs) -> Self {
        Self { palette, glyphs }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ColorOverrides, GlyphVariant, ThemeRegistry};
    use super::*;

    #[test]
    fn new_bundles_palette_and_glyphs() {
        let base = ThemeRegistry::builtin().resolve("default");
        let palette = Palette::resolve(base, &ColorOverrides::default());
        let skin = Skin::new(palette, Glyphs::new(GlyphVariant::Ascii));
        assert_eq!(skin.glyphs.variant, GlyphVariant::Ascii);
        assert_eq!(skin.palette.accent, base.accent);
    }
}
