//! The OKLab, OKLCH and HSL color spaces behind the [`super::Color`]
//! variants, plus the sRGB transfer-function helpers they share.
//!
//! Split out of the [`super::Color`] API: these are pure conversion math
//! with their own round-trip tests, and nothing outside this module needs
//! them.

// The OKLab matrices are copied reference constants at full published
// precision, and the docs name color spaces (OKLCH, OKLab, sRGB) as prose;
// these readability lints add no value in this math module.
#![allow(
    clippy::excessive_precision,
    clippy::unreadable_literal,
    clippy::doc_markdown
)]

use super::Color;

/// A color in the OKLab space (perceptual lightness plus two opponent axes).
#[derive(Clone, Copy)]
pub(super) struct Oklab {
    pub(super) lightness: f32,
    pub(super) a: f32,
    pub(super) b: f32,
}

/// A color in cylindrical OKLCH form (lightness, chroma, hue in radians).
#[derive(Clone, Copy)]
pub(super) struct Oklch {
    pub(super) lightness: f32,
    pub(super) chroma: f32,
    pub(super) hue: f32,
}

impl Oklab {
    pub(super) fn from_rgb((red, green, blue): (u8, u8, u8)) -> Oklab {
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

    pub(super) fn to_color(self) -> Color {
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

    pub(super) fn to_oklch(self) -> Oklch {
        Oklch {
            lightness: self.lightness,
            chroma: (self.a * self.a + self.b * self.b).sqrt(),
            hue: self.b.atan2(self.a),
        }
    }
}

impl Oklch {
    pub(super) fn to_color(self) -> Color {
        Oklab {
            lightness: self.lightness,
            a: self.chroma * self.hue.cos(),
            b: self.chroma * self.hue.sin(),
        }
        .to_color()
    }
}

/// A color in cylindrical HSL form (hue in degrees, saturation and lightness in
/// `0..=1`). Defined on gamma-encoded sRGB, so it does not go through OKLab.
#[derive(Clone, Copy)]
pub(super) struct Hsl {
    pub(super) h: f32,
    pub(super) s: f32,
    pub(super) l: f32,
}

impl Hsl {
    pub(super) fn from_rgb((red, green, blue): (u8, u8, u8)) -> Hsl {
        let red = f32::from(red) / 255.0;
        let green = f32::from(green) / 255.0;
        let blue = f32::from(blue) / 255.0;
        let max = red.max(green).max(blue);
        let min = red.min(green).min(blue);
        let delta = max - min;
        let lightness = f32::midpoint(max, min);

        if delta.abs() < f32::EPSILON {
            return Hsl {
                h: 0.0,
                s: 0.0,
                l: lightness,
            };
        }

        let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
        let hue = if (max - red).abs() < f32::EPSILON {
            ((green - blue) / delta).rem_euclid(6.0)
        } else if (max - green).abs() < f32::EPSILON {
            (blue - red) / delta + 2.0
        } else {
            (red - green) / delta + 4.0
        };
        Hsl {
            h: (hue * 60.0).rem_euclid(360.0),
            s: saturation,
            l: lightness,
        }
    }

    pub(super) fn to_color(self) -> Color {
        let hue = self.h.rem_euclid(360.0);
        let saturation = self.s.clamp(0.0, 1.0);
        let lightness = self.l.clamp(0.0, 1.0);
        let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
        let sector = hue / 60.0;
        let second = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
        let (red, green, blue) = match sector as u32 {
            0 => (chroma, second, 0.0),
            1 => (second, chroma, 0.0),
            2 => (0.0, chroma, second),
            3 => (0.0, second, chroma),
            4 => (second, 0.0, chroma),
            _ => (chroma, 0.0, second),
        };
        let base = lightness - chroma / 2.0;
        Color::Rgb(
            srgb_u8(red + base),
            srgb_u8(green + base),
            srgb_u8(blue + base),
        )
    }
}

/// Quantizes a gamma-encoded sRGB channel in `0..=1` to an 8-bit value. Unlike
/// [`to_channel`], it does no gamma conversion (HSL already lives in sRGB).
fn srgb_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
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
