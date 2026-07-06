//! Framework-agnostic theming: colors, palette, glyphs and themes.
//!
//! This layer depends on no UI framework, so a CLI can share it with the TUI.
//! Ratatui styles are mapped from [`Color`] in [`crate::style`]. Widgets
//! receive a single [`Skin`] bundling the palette and glyphs.

pub mod color;
pub mod glyphs;
pub mod palette;
pub mod skin;
pub mod theme_set;

pub use color::{Color, parse_color};
pub use glyphs::{GlyphVariant, Glyphs};
pub use palette::{ColorOverrides, Palette};
pub use skin::Skin;
pub use theme_set::{DEFAULT_THEME, ThemeColors, ThemeRegistry};
