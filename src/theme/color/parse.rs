//! Parsing a [`super::Color`] from text: hex, `rgb(r, g, b)` and a small
//! set of names.
//!
//! Kept apart from the color API and the conversion math: this is the one
//! part that takes untrusted input, so its failure modes are worth reading
//! in isolation.

use super::Color;

/// The value of a single hex digit; panics on a non-hex byte (see
/// [`Color::hex`]).
pub(super) const fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => panic!("hex color has a non-hex digit"),
    }
}

/// Parses a color from a hex string (`#rgb` or `#rrggbb`), an `rgb(r, g, b)`
/// triple, an `ansi:<name>` terminal colour, `default` (the terminal's own
/// colour, what [`Color::to_hex`] writes for [`Color::Default`]) or a named
/// palette entry. Returns `None` for anything else.
///
/// # Examples
///
/// ```
/// use ratada::theme::{AnsiColor, Color, parse_color};
/// assert_eq!(parse_color("ansi:green"), Some(Color::Ansi(AnsiColor::Green)));
/// assert_eq!(parse_color("#000"), Some(Color::Rgb(0, 0, 0)));
/// ```
pub fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.eq_ignore_ascii_case("default") {
        return Some(Color::Default);
    }
    parse_hex(value)
        .or_else(|| parse_rgb(value))
        .or_else(|| super::ansi::parse(value).map(Color::Ansi))
        .or_else(|| parse_named(value))
}

fn parse_hex(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#')?;
    let (red, green, blue) = match hex.len() {
        3 => {
            let expand = |index: usize| {
                let digit = &hex[index..=index];
                u8::from_str_radix(&digit.repeat(2), 16).ok()
            };
            (expand(0)?, expand(1)?, expand(2)?)
        }
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ),
        _ => return None,
    };
    Some(Color::Rgb(red, green, blue))
}

/// Parses `rgb(r, g, b)` or `rgb(r g b)` with 0-255 channels.
fn parse_rgb(value: &str) -> Option<Color> {
    let inner = value
        .strip_prefix("rgb(")
        .or_else(|| value.strip_prefix("RGB("))?
        .strip_suffix(')')?;
    let mut channels = inner
        .split([',', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| part.trim().parse::<u8>().ok());
    let red = channels.next()??;
    let green = channels.next()??;
    let blue = channels.next()??;
    if channels.next().is_some() {
        return None;
    }
    Some(Color::Rgb(red, green, blue))
}

/// Parses one of the eight soft ANSI-style color names. These are deliberately
/// muted RGB values (not the literal CSS/ANSI primaries) so `parse_color` inputs
/// blend with the toolkit's palette; they are independent of the semantic theme
/// colors, which carry meaning rather than naming a hue.
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
    fn parses_three_and_six_digit_hex() {
        assert_eq!(parse_color("#6dd0ff"), Some(Color::Rgb(109, 208, 255)));
        assert_eq!(parse_color("#fff"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(parse_color("#8bd3cd"), Some(Color::Rgb(139, 211, 205)));
    }

    #[test]
    fn parses_rgb_triples() {
        assert_eq!(
            parse_color("rgb(139, 211, 205)"),
            Some(Color::Rgb(139, 211, 205))
        );
        assert_eq!(parse_color("rgb(0 0 0)"), Some(Color::Rgb(0, 0, 0)));
        assert_eq!(parse_color("rgb(1,2)"), None);
    }

    #[test]
    fn rejects_malformed_hex() {
        assert_eq!(parse_color("#xyz"), None);
        assert_eq!(parse_color("#12345"), None);
    }

    #[test]
    fn parses_ansi_tokens_and_writes_them_back() {
        let green = parse_color("ansi:green");
        assert_eq!(green, Some(Color::Ansi(super::super::AnsiColor::Green)));
        assert_eq!(green.map(Color::to_hex).as_deref(), Some("ansi:green"));
    }

    #[test]
    fn default_reads_back_what_to_hex_writes() {
        assert_eq!(parse_color("default"), Some(Color::Default));
        assert_eq!(Color::Default.to_hex(), "default");
    }

    #[test]
    fn resolves_named_color() {
        assert_eq!(parse_color("green"), Some(Color::Rgb(140, 200, 140)));
    }
}
