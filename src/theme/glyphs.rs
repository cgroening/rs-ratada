//! Glyph variants and the resolved marker set, usable by CLI and TUI.
//!
//! Every glyph has one meaning across all apps: the pointer marks only the
//! cursor, the check only the value that applies (or done), and so on. A view
//! never writes one of these characters as a literal; it reads them from
//! [`Glyphs`] so the ASCII tier follows the configuration.

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
    /// The checkmark for the value that applies, or a done/toggled item.
    pub check: &'static str,
    /// The bullet for list markers.
    pub bullet: &'static str,
    /// The pointer marking the cursor row - and nothing else.
    pub pointer: &'static str,
    /// The mark of a row ticked in a multi-selection.
    pub mark: &'static str,
    /// The marker of a collapsed node or section.
    pub fold_closed: &'static str,
    /// The marker of an expanded node or section.
    pub fold_open: &'static str,
    /// The left half of a value stepper (`← value →`).
    pub stepper_left: &'static str,
    /// The right half of a value stepper (`← value →`).
    pub stepper_right: &'static str,
    /// The marker of a row that leads into a sub-page.
    pub descend: &'static str,
    /// The marker in front of a link or reference.
    pub link: &'static str,
}

impl Glyphs {
    /// Returns the glyph set for the given variant.
    pub fn new(variant: GlyphVariant) -> Self {
        match variant {
            GlyphVariant::Unicode => Self {
                variant,
                check: "\u{2713}",         // ✓
                bullet: "\u{2022}",        // •
                pointer: "\u{203a}",       // ›
                mark: "\u{25cf}",          // ●
                fold_closed: "\u{25b8}",   // ▸
                fold_open: "\u{25be}",     // ▾
                stepper_left: "\u{2190}",  // ←
                stepper_right: "\u{2192}", // →
                descend: "\u{bb}",         // »
                link: "\u{2197}",          // ↗
            },
            GlyphVariant::Ascii => Self {
                variant,
                check: "x",
                bullet: "*",
                pointer: ">",
                mark: "*",
                fold_closed: "+",
                fold_open: "-",
                stepper_left: "<",
                stepper_right: ">",
                descend: ">>",
                link: "->",
            },
        }
    }

    /// Returns `value` between the stepper halves: `← value →`.
    pub fn stepper(&self, value: &str) -> String {
        format!("{} {value} {}", self.stepper_left, self.stepper_right)
    }

    /// Returns the expand/collapse marker for a node in the given state.
    pub fn fold(&self, expanded: bool) -> &'static str {
        if expanded {
            self.fold_open
        } else {
            self.fold_closed
        }
    }

    /// The glyphs whose meanings must stay apart, with their names.
    #[cfg(test)]
    fn meanings(&self) -> [(&'static str, &'static str); 10] {
        [
            ("check", self.check),
            ("pointer", self.pointer),
            ("mark", self.mark),
            ("fold_closed", self.fold_closed),
            ("fold_open", self.fold_open),
            ("stepper_left", self.stepper_left),
            ("stepper_right", self.stepper_right),
            ("descend", self.descend),
            ("link", self.link),
            ("bullet", self.bullet),
        ]
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
        assert_eq!(glyphs.stepper("on"), "\u{2190} on \u{2192}");
    }

    #[test]
    fn ascii_variant_uses_plain_fallbacks() {
        let glyphs = Glyphs::new(GlyphVariant::Ascii);
        assert_eq!(glyphs.check, "x");
        assert_eq!(glyphs.bullet, "*");
        assert_eq!(glyphs.pointer, ">");
        assert_eq!(glyphs.stepper("on"), "< on >");
        assert!(glyphs.meanings().iter().all(|(_, glyph)| glyph.is_ascii()));
    }

    /// The Unicode tier gives every meaning its own glyph. (The ASCII tier
    /// has too few characters for that: `*` is both bullet and mark, `>`
    /// both pointer and stepper.)
    #[test]
    fn every_unicode_glyph_has_one_meaning() {
        let meanings = Glyphs::new(GlyphVariant::Unicode).meanings();
        for (i, (name, glyph)) in meanings.iter().enumerate() {
            for (other, other_glyph) in &meanings[i + 1..] {
                assert_ne!(glyph, other_glyph, "{name} and {other} share");
            }
        }
    }

    #[test]
    fn fold_picks_the_marker_for_the_state() {
        let glyphs = Glyphs::new(GlyphVariant::Unicode);
        assert_eq!(glyphs.fold(true), "\u{25be}");
        assert_eq!(glyphs.fold(false), "\u{25b8}");
    }

    #[test]
    fn variant_defaults_to_unicode() {
        assert_eq!(GlyphVariant::default(), GlyphVariant::Unicode);
    }
}
