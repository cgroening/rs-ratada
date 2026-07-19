//! A framework-agnostic color type plus parsing and OKLCH-based variants.
//!
//! Lightness/chroma operations ([`Color::darken`],
//! [`lighten`](Color::lighten), [`vivid`](Color::vivid), [`dim`](Color::dim),
//! [`shade`](Color::shade), [`mix`](Color::mix)) go through the perceptual
//! OKLCH color space, so shades stay hue-stable and evenly spaced instead of
//! shifting or muddying as naive RGB scaling would.

// The OKLab matrices are copied reference constants at full published
// precision, and the docs name color spaces (OKLCH, OKLab, sRGB) as prose;
// these readability lints add no value in this math module.
#![allow(
    clippy::excessive_precision,
    clippy::unreadable_literal,
    clippy::doc_markdown
)]

mod parse;
mod space;

use parse::nibble;
pub use parse::parse_color;
use space::{Hsl, Oklab, Oklch};

/// One discrete [`Color::shade`] step, in OKLab lightness.
const SHADE_STEP: f32 = 0.08;
/// Minimum OKLab lightness gap for [`Color::readable_on`] to keep `self`.
const READABLE_CONTRAST: f32 = 0.4;
/// The dark/light fallbacks [`Color::readable_on`] returns when `self` is too
/// low-contrast for the background. These are the generic contrast inks, kept
/// independent of any theme's `background`/`foreground` on purpose.
const READABLE_DARK: Color = Color::hex("#151515");
const READABLE_LIGHT: Color = Color::hex("#e5e5e5");
/// The luminance midpoint above which [`Color::readable_on`] treats a
/// background as light and picks the dark ink.
const READABLE_MID_LUMINANCE: f32 = 0.5;

/// A color value, independent of any UI framework. `Default` means "use the
/// surrounding default" (e.g. the terminal background).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// Inherit the surrounding default (e.g. the terminal background).
    Default,
    /// A 24-bit `(red, green, blue)` color.
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
    /// worth `SHADE_STEP` OKLab lightness. `Default` is unchanged.
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

    /// The perceptual distance to `other`, Euclidean in the OKLab space.
    /// `f32::INFINITY` when either side is `Default` (no comparable point).
    #[must_use]
    pub fn distance(self, other: Color) -> f32 {
        match (self.oklab(), other.oklab()) {
            (Some(first), Some(second)) => {
                let lightness = first.lightness - second.lightness;
                let green_red = first.a - second.a;
                let blue_yellow = first.b - second.b;
                (lightness * lightness
                    + green_red * green_red
                    + blue_yellow * blue_yellow)
                    .sqrt()
            }
            _ => f32::INFINITY,
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
        if background >= READABLE_MID_LUMINANCE {
            READABLE_DARK
        } else {
            READABLE_LIGHT
        }
    }

    /// The HSL components `(hue, saturation, lightness)` with `hue` in degrees
    /// (`0..360`) and `saturation`/`lightness` in `0..=1`, or `None` for
    /// [`Color::Default`].
    pub fn to_hsl(self) -> Option<(f32, f32, f32)> {
        self.rgb().map(|rgb| {
            let hsl = Hsl::from_rgb(rgb);
            (hsl.h, hsl.s, hsl.l)
        })
    }

    /// A color from HSL: `hue` in degrees, `saturation` and `lightness` in
    /// `0..=1` (values are wrapped/clamped into range).
    #[must_use]
    pub fn from_hsl(hue: f32, saturation: f32, lightness: f32) -> Color {
        Hsl {
            h: hue,
            s: saturation,
            l: lightness,
        }
        .to_color()
    }

    /// The OKLCH components `(lightness, chroma, hue)` with `lightness` in
    /// `0..=1`, `chroma >= 0` and `hue` in degrees (`0..360`), or `None` for
    /// [`Color::Default`].
    pub fn to_oklch(self) -> Option<(f32, f32, f32)> {
        self.oklch().map(|oklch| {
            (
                oklch.lightness,
                oklch.chroma,
                oklch.hue.to_degrees().rem_euclid(360.0),
            )
        })
    }

    /// A color from OKLCH: `lightness` in `0..=1`, `chroma >= 0`, `hue` in
    /// degrees. Out-of-gamut results are clamped to sRGB.
    #[must_use]
    pub fn from_oklch(lightness: f32, chroma: f32, hue: f32) -> Color {
        Oklch {
            lightness,
            chroma,
            hue: hue.to_radians(),
        }
        .to_color()
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
    fn hsl_roundtrip_is_near_identity() {
        for color in [
            Color::hex("#8bd3cd"),
            Color::hex("#151515"),
            Color::hex("#e5e5e5"),
            Color::hex("#d57b76"),
            Color::hex("#808080"),
        ] {
            let (hue, sat, light) = color.to_hsl().unwrap();
            let (red, green, blue) = channels(color);
            let (rr, gg, bb) = channels(Color::from_hsl(hue, sat, light));
            assert!(red.abs_diff(rr) <= 1, "{red} vs {rr}");
            assert!(green.abs_diff(gg) <= 1, "{green} vs {gg}");
            assert!(blue.abs_diff(bb) <= 1, "{blue} vs {bb}");
        }
    }

    #[test]
    fn hsl_has_zero_saturation_for_gray() {
        let (_, saturation, lightness) =
            Color::hex("#808080").to_hsl().unwrap();
        assert!(saturation.abs() < 1e-4, "gray saturation {saturation}");
        assert!(
            (lightness - 0.502).abs() < 0.01,
            "gray lightness {lightness}"
        );
    }

    #[test]
    fn hsl_hue_tracks_the_primaries() {
        let (red, _, _) = Color::Rgb(255, 0, 0).to_hsl().unwrap();
        let (green, _, _) = Color::Rgb(0, 255, 0).to_hsl().unwrap();
        let (blue, _, _) = Color::Rgb(0, 0, 255).to_hsl().unwrap();
        assert!(red.abs() < 1.0, "red hue {red}");
        assert!((green - 120.0).abs() < 1.0, "green hue {green}");
        assert!((blue - 240.0).abs() < 1.0, "blue hue {blue}");
    }

    #[test]
    fn oklch_roundtrip_is_near_identity() {
        for color in [
            Color::hex("#8bd3cd"),
            Color::hex("#151515"),
            Color::hex("#d57b76"),
            Color::hex("#7fb3d4"),
        ] {
            let (light, chroma, hue) = color.to_oklch().unwrap();
            let (red, green, blue) = channels(color);
            let (rr, gg, bb) = channels(Color::from_oklch(light, chroma, hue));
            assert!(red.abs_diff(rr) <= 1, "{red} vs {rr}");
            assert!(green.abs_diff(gg) <= 1, "{green} vs {gg}");
            assert!(blue.abs_diff(bb) <= 1, "{blue} vs {bb}");
        }
    }

    #[test]
    fn hsl_and_oklch_leave_default_none() {
        assert_eq!(Color::Default.to_hsl(), None);
        assert_eq!(Color::Default.to_oklch(), None);
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
    fn distance_is_zero_to_self_and_grows_with_difference() {
        let teal = Color::hex("#8bd3cd");
        assert!(teal.distance(teal) < 1e-6);
        let black = Color::hex("#000000");
        let white = Color::hex("#ffffff");
        let near = Color::hex("#111111");
        assert!(black.distance(near) < black.distance(white));
        assert!(Color::Default.distance(teal).is_infinite());
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
