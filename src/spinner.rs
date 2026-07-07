//! A small text spinner with Unicode (braille) and ASCII frames.

use crate::theme::GlyphVariant;

const UNICODE: [&str; 10] = [
    "\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}",
    "\u{2826}", "\u{2827}", "\u{2807}", "\u{280f}",
];
const ASCII: [&str; 4] = ["|", "/", "-", "\\"];

/// Cyclic spinner state; advance it on each tick.
#[derive(Debug, Default)]
pub struct Spinner {
    frame: usize,
}

impl Spinner {
    /// A spinner resting on its first frame.
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances to the next frame.
    pub fn advance(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// The current frame glyph for the given variant.
    pub fn frame(&self, variant: GlyphVariant) -> &'static str {
        match variant {
            GlyphVariant::Unicode => UNICODE[self.frame % UNICODE.len()],
            GlyphVariant::Ascii => ASCII[self.frame % ASCII.len()],
        }
    }
}
