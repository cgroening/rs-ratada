//! The resolved color palette and its construction from a theme plus overrides.

use super::{
    color::{Color, dim_color, parse_color},
    theme_set::ThemeColors,
};

/// Brightness factor for the dimmed accent (a muted variant of the accent).
const ACCENT_DIM_FACTOR: f32 = 0.75;
/// Brightness factor for the dark accent (a deep variant of the accent).
const ACCENT_DARK_FACTOR: f32 = 0.25;

/// Optional per-color overrides layered over a theme's base colors. An empty
/// string leaves the theme color untouched; a parseable value replaces it.
#[derive(Debug, Default, Clone)]
pub struct ColorOverrides<'a> {
    pub accent: &'a str,
    pub selection_bg: &'a str,
    pub cursor: &'a str,
    pub background: &'a str,
    pub surface: &'a str,
    pub surface_alt: &'a str,
    pub surface_bar: &'a str,
}

/// Resolved UI colors, built once and shared so no view reaches for a global or
/// a raw literal. Framework-agnostic: a UI layer maps these to its own types.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub accent: Color,
    /// Muted variant of `accent`, derived automatically.
    pub accent_dim: Color,
    /// Dark variant of `accent`, derived automatically.
    pub accent_dark: Color,
    pub selection_bg: Color,
    pub cursor: Color,
    /// Full-screen background; `Color::Default` keeps the terminal background.
    pub background: Color,
    /// Panel backgrounds for the `Panels` mode: `surface` for the gallery
    /// widget-list panel, `surface_alt` for the content/body, `surface_bar` for
    /// the header/status bars.
    pub surface: Color,
    pub surface_alt: Color,
    pub surface_bar: Color,
    /// Semantic colors carried through from the theme.
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub info: Color,
}

impl Palette {
    /// Builds a palette from a theme's [`ThemeColors`] with `overrides` layered
    /// on top: each non-empty, parseable override replaces the theme's core
    /// color. `accent_dim`/`accent_dark` are derived from the resolved accent;
    /// the semantic colors come straight from the theme.
    pub fn resolve(base: ThemeColors, overrides: &ColorOverrides<'_>) -> Self {
        let accent = override_or(overrides.accent, base.accent);
        Self {
            accent,
            accent_dim: dim_color(accent, ACCENT_DIM_FACTOR),
            accent_dark: dim_color(accent, ACCENT_DARK_FACTOR),
            selection_bg: override_or(
                overrides.selection_bg,
                base.selection_bg,
            ),
            cursor: override_or(overrides.cursor, base.cursor),
            background: override_or(overrides.background, base.background),
            surface: override_or(overrides.surface, base.surface),
            surface_alt: override_or(overrides.surface_alt, base.surface_alt),
            surface_bar: override_or(overrides.surface_bar, base.surface_bar),
            error: base.error,
            warning: base.warning,
            success: base.success,
            info: base.info,
        }
    }
}

/// The parsed override color, or `fallback` when the string is empty/invalid.
fn override_or(value: &str, fallback: Color) -> Color {
    parse_color(value).unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::{super::theme_set::ThemeRegistry, *};

    fn nord() -> ThemeColors {
        ThemeRegistry::builtin().resolve("nord")
    }

    #[test]
    fn resolve_without_overrides_uses_theme_colors() {
        let palette = Palette::resolve(nord(), &ColorOverrides::default());
        assert_eq!(palette.accent, nord().accent);
        assert_eq!(palette.background, nord().background);
    }

    #[test]
    fn non_empty_override_wins_over_theme() {
        let overrides = ColorOverrides {
            accent: "#010203",
            ..ColorOverrides::default()
        };
        let palette = Palette::resolve(nord(), &overrides);
        assert_eq!(palette.accent, Color::Rgb(1, 2, 3));
        // Untouched colors still come from the theme.
        assert_eq!(palette.cursor, nord().cursor);
    }

    #[test]
    fn empty_or_invalid_override_keeps_theme_color() {
        let overrides = ColorOverrides {
            accent: "",
            cursor: "nope",
            ..ColorOverrides::default()
        };
        let palette = Palette::resolve(nord(), &overrides);
        assert_eq!(palette.accent, nord().accent);
        assert_eq!(palette.cursor, nord().cursor);
    }

    #[test]
    fn semantic_colors_carry_through_from_the_theme() {
        let palette = Palette::resolve(nord(), &ColorOverrides::default());
        assert_eq!(palette.error, nord().error);
        assert_eq!(palette.success, nord().success);
    }

    #[test]
    fn surfaces_carry_through_and_can_be_overridden() {
        let palette = Palette::resolve(nord(), &ColorOverrides::default());
        assert_eq!(palette.surface, nord().surface);
        assert_eq!(palette.surface_bar, nord().surface_bar);

        let overrides = ColorOverrides {
            surface_alt: "#010203",
            ..ColorOverrides::default()
        };
        let palette = Palette::resolve(nord(), &overrides);
        assert_eq!(palette.surface_alt, Color::Rgb(1, 2, 3));
        // Untouched surfaces still come from the theme.
        assert_eq!(palette.surface, nord().surface);
    }

    #[test]
    fn accent_dim_and_dark_are_derived_from_accent() {
        let overrides = ColorOverrides {
            accent: "#646464",
            ..ColorOverrides::default()
        };
        let palette = Palette::resolve(nord(), &overrides);
        assert_eq!(
            palette.accent_dim,
            dim_color(palette.accent, ACCENT_DIM_FACTOR),
        );
        assert_eq!(
            palette.accent_dark,
            dim_color(palette.accent, ACCENT_DARK_FACTOR),
        );
    }
}
