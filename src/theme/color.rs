//! A framework-agnostic color type plus parsing and OKLCH-based variants.
//!
//! Lightness/chroma operations ([`Color::darken`], [`lighten`](Color::lighten),
//! [`vivid`](Color::vivid), [`dim`](Color::dim), [`shade`](Color::shade),
//! [`mix`](Color::mix)) go through the perceptual OKLCH color space, so shades
//! stay hue-stable and evenly spaced instead of shifting or muddying as naive
//! RGB scaling would.

// The OKLab matrices are copied reference constants at full published
// precision, and the docs name color spaces (OKLCH, OKLab, sRGB) as prose;
// these readability lints add no value in this math module.
#![allow(
    clippy::excessive_precision,
    clippy::unreadable_literal,
    clippy::doc_markdown
)]

/// One discrete [`Color::shade`] step, in OKLab lightness.
const SHADE_STEP: f32 = 0.08;
/// Minimum OKLab lightness gap for [`Color::readable_on`] to keep `self`.
const READABLE_CONTRAST: f32 = 0.4;
/// The dark/light fallbacks [`Color::readable_on`] returns when `self` is too
/// low-contrast for the background.
const READABLE_DARK: Color = Color::hex("#151515");
const READABLE_LIGHT: Color = Color::hex("#e5e5e5");

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

    /// Builds a color from an HTML-style `"#rrggbb"` string. Being `const`, it
    /// works in theme literals; a malformed literal fails at compile time (in
    /// const position) or panics at runtime otherwise. For the short `#rgb`
    /// form, `rgb(...)` or named colors, use [`parse_color`] instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use ratada::theme::Color;
    /// const ACCENT: Color = Color::hex("#8bd3cd");
    /// assert_eq!(ACCENT.to_hex(), "#8bd3cd");
    /// ```
    ///
    /// # Panics
    ///
    /// Panics when `value` is not exactly `"#rrggbb"` with six hex digits.
    #[must_use]
    pub const fn hex(value: &str) -> Color {
        let bytes = value.as_bytes();
        assert!(
            bytes.len() == 7 && bytes[0] == b'#',
            "hex color must be \"#rrggbb\"",
        );
        Color::Rgb(
            (nibble(bytes[1]) << 4) | nibble(bytes[2]),
            (nibble(bytes[3]) << 4) | nibble(bytes[4]),
            (nibble(bytes[5]) << 4) | nibble(bytes[6]),
        )
    }

    /// A darker variant: lowers OKLab lightness by `amount` (0..1). `Default` is
    /// unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// use ratada::theme::Color;
    /// let base = Color::hex("#8bd3cd");
    /// assert!(base.darken(0.2).luminance() < base.luminance());
    /// ```
    #[must_use]
    pub fn darken(self, amount: f32) -> Color {
        self.adjust(|color| {
            color.lightness = (color.lightness - amount).max(0.0);
        })
    }

    /// A lighter variant: raises OKLab lightness by `amount` (0..1). `Default`
    /// is unchanged.
    #[must_use]
    pub fn lighten(self, amount: f32) -> Color {
        self.adjust(|color| {
            color.lightness = (color.lightness + amount).min(1.0);
        })
    }

    /// A more saturated variant: scales chroma by `1 + amount`. `Default` is
    /// unchanged.
    #[must_use]
    pub fn vivid(self, amount: f32) -> Color {
        self.adjust(|color| color.chroma *= 1.0 + amount)
    }

    /// A muted variant toward gray: scales chroma by `1 - amount` (clamped at
    /// 0). `Default` is unchanged.
    #[must_use]
    pub fn dim(self, amount: f32) -> Color {
        self.adjust(|color| color.chroma *= (1.0 - amount).max(0.0))
    }

    /// A discrete lightness step: negative darkens, positive lightens, each step
    /// worth [`SHADE_STEP`] OKLab lightness. `Default` is unchanged.
    #[must_use]
    pub fn shade(self, step: i8) -> Color {
        let delta = f32::from(step) * SHADE_STEP;
        self.adjust(|color| {
            color.lightness = (color.lightness + delta).clamp(0.0, 1.0);
        })
    }

    /// Interpolates toward `other` by `t` (0..1) in the OKLab space. If either
    /// side is `Default`, the other is returned (or `Default` if both are).
    #[must_use]
    pub fn mix(self, other: Color, t: f32) -> Color {
        match (self.oklab(), other.oklab()) {
            (Some(a), Some(b)) => Oklab {
                lightness: a.lightness + (b.lightness - a.lightness) * t,
                a: a.a + (b.a - a.a) * t,
                b: a.b + (b.b - a.b) * t,
            }
            .to_color(),
            (Some(_), None) => self,
            (None, other_lab) => {
                if other_lab.is_some() {
                    other
                } else {
                    Color::Default
                }
            }
        }
    }

    /// The perceptual lightness (OKLab L, 0..1). `Default` yields `0.0`.
    pub fn luminance(self) -> f32 {
        self.oklab().map_or(0.0, |lab| lab.lightness)
    }

    /// A foreground readable on `bg`: keeps `self` when it contrasts enough,
    /// else returns a near-black or near-white fallback matching `bg`.
    #[must_use]
    pub fn readable_on(self, bg: Color) -> Color {
        let background = bg.luminance();
        if self != Color::Default
            && bg != Color::Default
            && (self.luminance() - background).abs() >= READABLE_CONTRAST
        {
            return self;
        }
        if background >= 0.5 {
            READABLE_DARK
        } else {
            READABLE_LIGHT
        }
    }

    /// Applies `edit` to the OKLCH form and converts back. `Default` is returned
    /// unchanged (there is no base to transform).
    fn adjust(self, edit: impl Fn(&mut Oklch)) -> Color {
        match self.oklch() {
            Some(mut oklch) => {
                edit(&mut oklch);
                oklch.to_color()
            }
            None => Color::Default,
        }
    }

    fn oklab(self) -> Option<Oklab> {
        self.rgb().map(Oklab::from_rgb)
    }

    fn oklch(self) -> Option<Oklch> {
        self.oklab().map(Oklab::to_oklch)
    }
}

/// A color in the OKLab space (perceptual lightness plus two opponent axes).
#[derive(Clone, Copy)]
struct Oklab {
    lightness: f32,
    a: f32,
    b: f32,
}

/// A color in cylindrical OKLCH form (lightness, chroma, hue in radians).
#[derive(Clone, Copy)]
struct Oklch {
    lightness: f32,
    chroma: f32,
    hue: f32,
}

impl Oklab {
    fn from_rgb((red, green, blue): (u8, u8, u8)) -> Oklab {
        let r = srgb_to_linear(f32::from(red) / 255.0);
        let g = srgb_to_linear(f32::from(green) / 255.0);
        let b = srgb_to_linear(f32::from(blue) / 255.0);

        let long = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
        let medium = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
        let short = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;

        let long = long.cbrt();
        let medium = medium.cbrt();
        let short = short.cbrt();

        Oklab {
            lightness: 0.2104542553 * long + 0.7936177850 * medium
                - 0.0040720468 * short,
            a: 1.9779984951 * long - 2.4285922050 * medium
                + 0.4505937099 * short,
            b: 0.0259040371 * long + 0.7827717662 * medium
                - 0.8086757660 * short,
        }
    }

    fn to_color(self) -> Color {
        let long =
            self.lightness + 0.3963377774 * self.a + 0.2158037573 * self.b;
        let medium =
            self.lightness - 0.1055613458 * self.a - 0.0638541728 * self.b;
        let short =
            self.lightness - 0.0894841775 * self.a - 1.2914855480 * self.b;

        let long = long * long * long;
        let medium = medium * medium * medium;
        let short = short * short * short;

        let r =
            4.0767416621 * long - 3.3077115913 * medium + 0.2309699292 * short;
        let g =
            -1.2684380046 * long + 2.6097574011 * medium - 0.3413193965 * short;
        let b =
            -0.0041960863 * long - 0.7034186147 * medium + 1.7076147010 * short;

        Color::Rgb(to_channel(r), to_channel(g), to_channel(b))
    }

    fn to_oklch(self) -> Oklch {
        Oklch {
            lightness: self.lightness,
            chroma: (self.a * self.a + self.b * self.b).sqrt(),
            hue: self.b.atan2(self.a),
        }
    }
}

impl Oklch {
    fn to_color(self) -> Color {
        Oklab {
            lightness: self.lightness,
            a: self.chroma * self.hue.cos(),
            b: self.chroma * self.hue.sin(),
        }
        .to_color()
    }
}

fn srgb_to_linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(channel: f32) -> f32 {
    if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    }
}

/// Clamps a linear channel to the gamut and encodes it back to an 8-bit sRGB
/// value.
fn to_channel(linear: f32) -> u8 {
    (linear_to_srgb(linear.clamp(0.0, 1.0)) * 255.0).round() as u8
}

/// The value of a single hex digit; panics on a non-hex byte (see
/// [`Color::hex`]).
const fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => panic!("hex color has a non-hex digit"),
    }
}

/// Parses a color from a hex string (`#rgb` or `#rrggbb`), an `rgb(r, g, b)`
/// triple, or a named palette entry. Returns `None` for anything else.
pub fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    parse_hex(value)
        .or_else(|| parse_rgb(value))
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

    fn channels(color: Color) -> (u8, u8, u8) {
        color.rgb().expect("rgb")
    }

    #[test]
    fn hex_builds_from_a_six_digit_string() {
        assert_eq!(Color::hex("#8bd3cd"), Color::Rgb(139, 211, 205));
        assert_eq!(Color::hex("#FFFFFF"), Color::Rgb(255, 255, 255));
    }

    #[test]
    #[should_panic(expected = "non-hex digit")]
    fn hex_panics_on_a_bad_literal() {
        let _ = Color::hex("#zz11ff");
    }

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
    fn resolves_named_color() {
        assert_eq!(parse_color("green"), Some(Color::Rgb(140, 200, 140)));
    }

    #[test]
    fn oklab_roundtrip_is_near_identity() {
        for color in [
            Color::hex("#8bd3cd"),
            Color::hex("#151515"),
            Color::hex("#e5e5e5"),
            Color::hex("#d57b76"),
        ] {
            let (r, g, b) = channels(color);
            let (rr, gg, bb) = channels(color.oklab().unwrap().to_color());
            assert!(r.abs_diff(rr) <= 1, "{r} vs {rr}");
            assert!(g.abs_diff(gg) <= 1, "{g} vs {gg}");
            assert!(b.abs_diff(bb) <= 1, "{b} vs {bb}");
        }
    }

    #[test]
    fn darken_lowers_and_lighten_raises_luminance() {
        let base = Color::hex("#8bd3cd");
        assert!(base.darken(0.2).luminance() < base.luminance());
        assert!(base.lighten(0.2).luminance() > base.luminance());
    }

    #[test]
    fn vivid_raises_and_dim_lowers_chroma() {
        let base = Color::hex("#8bd3cd");
        assert!(
            base.vivid(0.3).oklch().unwrap().chroma
                > base.oklch().unwrap().chroma
        );
        assert!(
            base.dim(0.3).oklch().unwrap().chroma
                < base.oklch().unwrap().chroma
        );
    }

    #[test]
    fn mix_endpoints_return_the_sides() {
        let a = Color::hex("#151515");
        let b = Color::hex("#8bd3cd");
        assert_eq!(channels(a.mix(b, 0.0)), channels(a));
        assert_eq!(channels(a.mix(b, 1.0)), channels(b));
    }

    #[test]
    fn readable_on_picks_a_contrasting_fallback() {
        // Label on its own swatch: no self-contrast, so a fallback is chosen.
        let light = Color::hex("#e5e5e5");
        let dark = Color::hex("#151515");
        assert_eq!(light.readable_on(light), READABLE_DARK);
        assert_eq!(dark.readable_on(dark), READABLE_LIGHT);
    }

    #[test]
    fn transforms_leave_default_unchanged() {
        assert_eq!(Color::Default.darken(0.5), Color::Default);
        assert_eq!(Color::Default.vivid(0.5), Color::Default);
        assert!(Color::Default.luminance().abs() < 1e-6);
        // Mixing with Default yields the other side; both Default stays Default.
        assert_eq!(
            Color::Default.mix(Color::Rgb(1, 2, 3), 0.5),
            Color::Rgb(1, 2, 3)
        );
        assert_eq!(Color::Default.mix(Color::Default, 0.5), Color::Default);
    }
}
