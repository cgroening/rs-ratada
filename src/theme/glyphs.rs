//! Glyph variants and the resolved marker set, usable by CLI and TUI.

use serde::{Deserialize, Serialize};

/// Whether the UI renders Unicode glyphs or a plain ASCII fallback.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum GlyphVariant {
    #[default]
    Unicode,
    Ascii,
}

/// Marker glyphs used across the UI, resolved from a [`GlyphVariant`].
#[derive(Debug, Clone, Copy)]
pub struct Glyphs {
    pub variant: GlyphVariant,
    pub check: &'static str,
    pub bullet: &'static str,
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
