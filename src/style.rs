//! ratatui adapter for framework-agnostic [`theme::Color`] values.
//!
//! This is the single place that maps theme colors to ratatui styles, so the
//! rest of the kit never converts colors inline.

use ratatui::style::{Color as RatColor, Modifier, Style};

use crate::theme::Color;

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

/// Dimmed style for secondary text.
pub fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
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
