//! The sixteen terminal colours, written as `ansi:<name>`.
//!
//! An ANSI colour follows the terminal's own palette, so it has no RGB value
//! of its own: the OKLCH helpers leave it unchanged, like
//! [`super::Color::Default`]. It exists so a theme can name exactly the colour
//! a terminal-native default uses.

/// The prefix that marks an ANSI colour token.
pub(super) const PREFIX: &str = "ansi:";

/// One of the sixteen terminal colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiColor {
    /// Black (colour 0).
    Black,
    /// Red (colour 1).
    Red,
    /// Green (colour 2).
    Green,
    /// Yellow (colour 3).
    Yellow,
    /// Blue (colour 4).
    Blue,
    /// Magenta (colour 5).
    Magenta,
    /// Cyan (colour 6).
    Cyan,
    /// Light gray, the terminal's normal white (colour 7).
    Gray,
    /// Dark gray, the terminal's bright black (colour 8).
    DarkGray,
    /// Bright red (colour 9).
    LightRed,
    /// Bright green (colour 10).
    LightGreen,
    /// Bright yellow (colour 11).
    LightYellow,
    /// Bright blue (colour 12).
    LightBlue,
    /// Bright magenta (colour 13).
    LightMagenta,
    /// Bright cyan (colour 14).
    LightCyan,
    /// Bright white (colour 15).
    White,
}

/// Every ANSI colour with its token name, in terminal order.
const NAMES: [(AnsiColor, &str); 16] = [
    (AnsiColor::Black, "black"),
    (AnsiColor::Red, "red"),
    (AnsiColor::Green, "green"),
    (AnsiColor::Yellow, "yellow"),
    (AnsiColor::Blue, "blue"),
    (AnsiColor::Magenta, "magenta"),
    (AnsiColor::Cyan, "cyan"),
    (AnsiColor::Gray, "gray"),
    (AnsiColor::DarkGray, "dark_gray"),
    (AnsiColor::LightRed, "light_red"),
    (AnsiColor::LightGreen, "light_green"),
    (AnsiColor::LightYellow, "light_yellow"),
    (AnsiColor::LightBlue, "light_blue"),
    (AnsiColor::LightMagenta, "light_magenta"),
    (AnsiColor::LightCyan, "light_cyan"),
    (AnsiColor::White, "white"),
];

impl AnsiColor {
    /// Every ANSI colour, in terminal order.
    #[must_use]
    pub fn all() -> [AnsiColor; 16] {
        NAMES.map(|(color, _)| color)
    }

    /// The token name without the prefix, e.g. `"dark_gray"`.
    #[must_use]
    pub fn name(self) -> &'static str {
        NAMES
            .iter()
            .find(|(color, _)| *color == self)
            .map_or("", |(_, name)| name)
    }

    /// The colour a name (without the prefix) stands for; `grey` is accepted
    /// for `gray`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<AnsiColor> {
        let name = name.trim().to_ascii_lowercase().replace("grey", "gray");
        NAMES
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(color, _)| *color)
    }
}

/// Parses an `ansi:<name>` token.
pub(super) fn parse(value: &str) -> Option<AnsiColor> {
    let name = value
        .get(..PREFIX.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(PREFIX))
        .map(|_| &value[PREFIX.len()..])?;
    AnsiColor::from_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_round_trips() {
        for color in AnsiColor::all() {
            let token = format!("{PREFIX}{}", color.name());
            assert_eq!(parse(&token), Some(color), "{token}");
        }
    }

    #[test]
    fn parsing_ignores_case_and_accepts_grey() {
        assert_eq!(parse("ANSI:Green"), Some(AnsiColor::Green));
        assert_eq!(parse("ansi:dark_grey"), Some(AnsiColor::DarkGray));
    }

    #[test]
    fn a_token_without_the_prefix_or_with_an_unknown_name_is_none() {
        assert_eq!(parse("green"), None);
        assert_eq!(parse("ansi:orange"), None);
        assert_eq!(parse("ansi:"), None);
    }
}
