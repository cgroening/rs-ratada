//! Framework-agnostic theming: colors, palette, glyphs, themes and mode.
//!
//! This layer depends on no UI framework, so a CLI can share it with the TUI.
//! Ratatui styles are mapped from [`Color`] in [`crate::style`]. Widgets
//! receive a single [`Skin`] bundling palette, glyphs and [`Mode`].

pub mod color;
pub mod glyphs;
pub mod mode;
pub mod palette;
pub mod skin;
pub mod theme_set;

pub use color::{Color, dim_color, lighten, parse_color};
pub use glyphs::{GlyphVariant, Glyphs};
pub use mode::Mode;
pub use palette::{ColorOverrides, Palette};
pub use skin::Skin;
pub use theme_set::{
    DEFAULT_THEME, Surfaces, ThemeColors, ThemeRegistry, derive_surfaces,
};
