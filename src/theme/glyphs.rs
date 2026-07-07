//! Glyph variants and the resolved marker set, usable by CLI and TUI.

use serde::{Deserialize, Serialize};

/// Whether the UI renders Unicode glyphs or a plain ASCII fallback.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum GlyphVariant {
    /// Unicode symbols (the default).
    #[default]
    Unicode,
    /// Plain ASCII-only fallbacks.
    Ascii,
}

/// Marker glyphs used across the UI, resolved from a [`GlyphVariant`].
#[derive(Debug, Clone, Copy)]
pub struct Glyphs {
    /// The variant these glyphs were resolved from.
    pub variant: GlyphVariant,
    /// The checkmark for selected/toggled items.
    pub check: &'static str,
    /// The bullet for list markers.
    pub bullet: &'static str,
    /// The pointer marking the focused/selected row.
    pub pointer: &'static str,
}

impl Glyphs {
    /// Returns the glyph set for the given variant.
    pub fn new(variant: GlyphVariant) -> Self {
        match variant {
            GlyphVariant::Unicode => Self {
                variant,
                check: "\u{2713}",
                bullet: "\u{2022}",
                pointer: "\u{203a}",
            },
            GlyphVariant::Ascii => Self {
                variant,
                check: "x",
                bullet: "*",
                pointer: ">",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_variant_uses_symbol_glyphs() {
        let glyphs = Glyphs::new(GlyphVariant::Unicode);
        assert_eq!(glyphs.variant, GlyphVariant::Unicode);
        assert_eq!(glyphs.check, "\u{2713}");
        assert_eq!(glyphs.pointer, "\u{203a}");
    }

    #[test]
    fn ascii_variant_uses_plain_fallbacks() {
        let glyphs = Glyphs::new(GlyphVariant::Ascii);
        assert_eq!(glyphs.check, "x");
        assert_eq!(glyphs.bullet, "*");
        assert_eq!(glyphs.pointer, ">");
    }

    #[test]
    fn variant_defaults_to_unicode() {
        assert_eq!(GlyphVariant::default(), GlyphVariant::Unicode);
    }
}
