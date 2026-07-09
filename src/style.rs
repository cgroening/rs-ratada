//! ratatui adapter for framework-agnostic [`crate::theme::Color`] values.
//!
//! This is the single place that maps theme colors to ratatui styles, so the
//! rest of the kit never converts colors inline.

use ratatui::style::{Color as RatColor, Modifier, Style};

use crate::theme::{Color, Palette};

/// Converts a theme color to a ratatui color (`Default` -> `Reset`).
pub fn to_ratatui(color: Color) -> RatColor {
    match color {
        Color::Default => RatColor::Reset,
        Color::Rgb(red, green, blue) => RatColor::Rgb(red, green, blue),
    }
}

/// A foreground style in `color`.
pub fn fg(color: Color) -> Style {
    Style::default().fg(to_ratatui(color))
}

/// A background style in `color`.
pub fn bg(color: Color) -> Style {
    Style::default().bg(to_ratatui(color))
}

/// A base style pairing a `foreground` and `background`; set once on the
/// full-screen block so plain text inherits the theme's text color.
pub fn base(foreground: Color, background: Color) -> Style {
    Style::default()
        .fg(to_ratatui(foreground))
        .bg(to_ratatui(background))
}

/// The `DIM` modifier. Prefer the palette-driven [`secondary`] for secondary
/// text; this remains for helpers that have no [`Palette`] in scope.
pub fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

// --- Semantic style roles ---------------------------------------------------
//
// The single source for how each part of the UI is colored. Widgets use these
// instead of assembling `fg(...).add_modifier(...)` by hand, so the look lives
// in one place.

/// Primary text.
pub fn primary(palette: &Palette) -> Style {
    fg(palette.foreground)
}

/// Secondary/dimmed text (replaces the bare `DIM` modifier).
pub fn secondary(palette: &Palette) -> Style {
    fg(palette.foreground_dim)
}

/// Muted chrome text: border badges and similar annotations that sit on a frame
/// and must not compete with the content. Dimmer than [`secondary`].
pub fn muted(palette: &Palette) -> Style {
    fg(palette.foreground_muted)
}

/// A heading/title: primary text, bold.
pub fn title(palette: &Palette) -> Style {
    fg(palette.foreground).add_modifier(Modifier::BOLD)
}

/// Accent text.
pub fn accent(palette: &Palette) -> Style {
    fg(palette.accent)
}

/// Muted accent text.
pub fn accent_dim(palette: &Palette) -> Style {
    fg(palette.accent_dim)
}

/// Vivid accent text.
pub fn accent_vivid(palette: &Palette) -> Style {
    fg(palette.accent_vivid)
}

/// A shortcut key: accent, bold.
pub fn key(palette: &Palette) -> Style {
    fg(palette.accent).add_modifier(Modifier::BOLD)
}

/// The selected-row / active background.
pub fn selected(palette: &Palette) -> Style {
    bg(palette.selection)
}

/// The block-cursor background.
pub fn cursor(palette: &Palette) -> Style {
    bg(palette.cursor)
}

/// A border/line.
pub fn border(palette: &Palette) -> Style {
    fg(palette.border)
}

/// The border of a focused box: a lifted `border` that keeps its contrast
/// against the brighter fill a focused field draws.
pub fn border_focus(palette: &Palette) -> Style {
    fg(palette.border_focus)
}

/// A disabled/unavailable item.
pub fn disabled(palette: &Palette) -> Style {
    fg(palette.foreground_dim)
}

/// Success text.
pub fn success(palette: &Palette) -> Style {
    fg(palette.success)
}

/// Warning text.
pub fn warning(palette: &Palette) -> Style {
    fg(palette.warning)
}

/// Error text.
pub fn error(palette: &Palette) -> Style {
    fg(palette.error)
}

/// Info text.
pub fn info(palette: &Palette) -> Style {
    fg(palette.info)
}

/// Darkens a ratatui color toward black by `factor` (`0.0` = black, `1.0` =
/// unchanged). Only `Rgb` colors are scaled; every other variant (e.g.
/// `Reset` or a named ANSI color) has no known RGB base and is returned as-is.
pub fn darken(color: RatColor, factor: f32) -> RatColor {
    let scale = |channel: u8| (f32::from(channel) * factor).round() as u8;
    match color {
        RatColor::Rgb(red, green, blue) => {
            RatColor::Rgb(scale(red), scale(green), scale(blue))
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darken_scales_rgb_channels() {
        assert_eq!(
            darken(RatColor::Rgb(100, 200, 50), 0.5),
            RatColor::Rgb(50, 100, 25),
        );
    }

    #[test]
    fn darken_leaves_non_rgb_unchanged() {
        assert_eq!(darken(RatColor::Reset, 0.5), RatColor::Reset);
        assert_eq!(darken(RatColor::Red, 0.5), RatColor::Red);
    }
}
