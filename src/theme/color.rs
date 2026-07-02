//! A framework-agnostic color type plus parsing and dimming.

/// A color value, independent of any UI framework. `Default` means "use the
/// surrounding default" (e.g. the terminal background).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Default,
    Rgb(u8, u8, u8),
}

impl Color {
    /// The `(r, g, b)` channels, or `None` for [`Color::Default`].
    pub fn rgb(self) -> Option<(u8, u8, u8)> {
        match self {
            Color::Rgb(red, green, blue) => Some((red, green, blue)),
            Color::Default => None,
        }
    }

    /// A `#rrggbb` hex string, or `"default"` for [`Color::Default`].
    pub fn to_hex(self) -> String {
        match self {
            Color::Rgb(red, green, blue) => {
                format!("#{red:02x}{green:02x}{blue:02x}")
            }
            Color::Default => "default".to_string(),
        }
    }
}

/// Parses a color from a `#rrggbb` hex string or a named palette entry.
pub fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    parse_hex(value).or_else(|| parse_named(value))
}

/// Returns a darker variant of `color` by scaling its RGB channels by `factor`
/// (`0.0` = black, `1.0` = unchanged). `Default` is returned unchanged.
pub fn dim_color(color: Color, factor: f32) -> Color {
    let scale = |channel: u8| (f32::from(channel) * factor).round() as u8;
    match color {
        Color::Rgb(red, green, blue) => {
            Color::Rgb(scale(red), scale(green), scale(blue))
        }
        Color::Default => Color::Default,
    }
}

/// Returns a lighter variant of `color` by moving each RGB channel toward white
/// by `factor` (`0.0` = unchanged, `1.0` = white). `Default` is returned
/// unchanged, since there is no known base to lighten.
pub fn lighten(color: Color, factor: f32) -> Color {
    let raise = |channel: u8| {
        let channel = f32::from(channel);
        (channel + (255.0 - channel) * factor).round() as u8
    };
    match color {
        Color::Rgb(red, green, blue) => {
            Color::Rgb(raise(red), raise(green), raise(blue))
        }
        Color::Default => Color::Default,
    }
}

fn parse_hex(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(red, green, blue))
}

fn parse_named(value: &str) -> Option<Color> {
    let (red, green, blue) = match value.to_ascii_lowercase().as_str() {
        "red" => (243, 139, 139),
        "green" => (140, 200, 140),
        "yellow" => (255, 185, 84),
        "blue" => (109, 168, 255),
        "cyan" => (109, 208, 255),
        "magenta" | "purple" => (197, 160, 255),
        "gray" | "grey" => (150, 150, 150),
        _ => return None,
    };
    Some(Color::Rgb(red, green, blue))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_six_digit_hex() {
        assert_eq!(parse_color("#6dd0ff"), Some(Color::Rgb(109, 208, 255)));
    }

    #[test]
    fn rejects_malformed_hex() {
        assert_eq!(parse_color("#xyz"), None);
        assert_eq!(parse_color("#12345"), None);
    }

    #[test]
    fn resolves_named_color() {
        assert_eq!(parse_color("green"), Some(Color::Rgb(140, 200, 140)));
    }

    #[test]
    fn dim_color_scales_rgb_channels() {
        assert_eq!(
            dim_color(Color::Rgb(100, 200, 50), 0.5),
            Color::Rgb(50, 100, 25),
        );
    }

    #[test]
    fn dim_color_leaves_default_unchanged() {
        assert_eq!(dim_color(Color::Default, 0.5), Color::Default);
    }

    #[test]
    fn lighten_moves_channels_toward_white() {
        assert_eq!(
            lighten(Color::Rgb(100, 200, 50), 0.5),
            Color::Rgb(178, 228, 153),
        );
    }

    #[test]
    fn lighten_leaves_default_unchanged() {
        assert_eq!(lighten(Color::Default, 0.5), Color::Default);
    }
}
