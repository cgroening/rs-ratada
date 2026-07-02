//! Named color themes and the registry that resolves a theme name to colors.
//!
//! A theme is the color axis of the UI, independent of the layout
//! [`Mode`](super::Mode). Built-in themes live here; the host may add custom
//! themes from config, all reachable by name through a [`ThemeRegistry`]. Each
//! theme provides a [`ThemeColors`] base that the [`Palette`](super::Palette)
//! builds on.

use super::color::{Color, lighten};

/// The default theme name, used as the fallback when a name is unknown.
pub const DEFAULT_THEME: &str = "default";

/// Universal semantic colors, used when a theme omits its own.
const DEFAULT_ERROR: Color = Color::Rgb(0xf3, 0x8b, 0x8b);
const DEFAULT_WARNING: Color = Color::Rgb(0xff, 0xb9, 0x54);
const DEFAULT_SUCCESS: Color = Color::Rgb(0x8c, 0xc8, 0x8c);
const DEFAULT_INFO: Color = Color::Rgb(0x6d, 0xa8, 0xff);

/// How far each surface is lightened from the theme's `background`. The bars
/// and the sidebar sit progressively above the content surface so the
/// borderless `Panels` layout reads as distinct regions.
const SURFACE_FACTOR: f32 = 0.06;
const SURFACE_ALT_FACTOR: f32 = 0.12;
const SURFACE_BAR_FACTOR: f32 = 0.18;

/// The panel background colors used by the borderless `Panels` mode: `surface`
/// for the gallery widget-list panel, `surface_alt` for the content/body,
/// `surface_bar` for the header and status bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surfaces {
    pub surface: Color,
    pub surface_alt: Color,
    pub surface_bar: Color,
}

/// Derives the three panel surfaces from a theme's `background` by lightening it
/// in three steps. A [`Color::Default`] background yields all-`Default` surfaces
/// (the terminal background shows through, so the panels do not stand apart).
pub fn derive_surfaces(background: Color) -> Surfaces {
    Surfaces {
        surface: lighten(background, SURFACE_FACTOR),
        surface_alt: lighten(background, SURFACE_ALT_FACTOR),
        surface_bar: lighten(background, SURFACE_BAR_FACTOR),
    }
}

/// The base colors a theme contributes before any config override is applied.
///
/// The first four are the core roles; the semantic colors carry meaning
/// (errors, warnings, success, info) and default to universal values when a
/// theme does not set them via [`ThemeColors::with_semantics`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    pub accent: Color,
    pub selection_bg: Color,
    pub cursor: Color,
    pub background: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub info: Color,
    /// Panel backgrounds for the `Panels` mode; derived from `background` by
    /// default, overridable per theme via [`ThemeColors::with_surfaces`].
    pub surface: Color,
    pub surface_alt: Color,
    pub surface_bar: Color,
}

impl ThemeColors {
    /// Builds a theme from its four core colors, with universal semantic colors
    /// and panel surfaces derived from `background`.
    pub fn new(
        accent: Color,
        selection_bg: Color,
        cursor: Color,
        background: Color,
    ) -> Self {
        let surfaces = derive_surfaces(background);
        Self {
            accent,
            selection_bg,
            cursor,
            background,
            error: DEFAULT_ERROR,
            warning: DEFAULT_WARNING,
            success: DEFAULT_SUCCESS,
            info: DEFAULT_INFO,
            surface: surfaces.surface,
            surface_alt: surfaces.surface_alt,
            surface_bar: surfaces.surface_bar,
        }
    }

    /// Returns a copy with explicit semantic colors.
    #[must_use]
    pub fn with_semantics(
        mut self,
        error: Color,
        warning: Color,
        success: Color,
        info: Color,
    ) -> Self {
        self.error = error;
        self.warning = warning;
        self.success = success;
        self.info = info;
        self
    }

    /// Returns a copy with explicit panel surfaces, replacing the derived ones.
    #[must_use]
    pub fn with_surfaces(mut self, surfaces: Surfaces) -> Self {
        self.surface = surfaces.surface;
        self.surface_alt = surfaces.surface_alt;
        self.surface_bar = surfaces.surface_bar;
        self
    }
}

/// The built-in themes, in cycling order. The first entry is the default.
fn builtin_themes() -> Vec<(String, ThemeColors)> {
    vec![
        (
            DEFAULT_THEME.to_string(),
            ThemeColors::new(
                Color::Rgb(0xe6, 0x8f, 0xff),
                Color::Rgb(0x33, 0x38, 0x4a),
                Color::Rgb(0xf3, 0x8b, 0x8b),
                Color::Rgb(0x0d, 0x0d, 0x1e),
            ),
        ),
        (
            "nord".to_string(),
            ThemeColors::new(
                Color::Rgb(0x88, 0xc0, 0xd0),
                Color::Rgb(0x3b, 0x42, 0x52),
                Color::Rgb(0xbf, 0x61, 0x6a),
                Color::Rgb(0x2e, 0x34, 0x40),
            )
            .with_semantics(
                Color::Rgb(0xbf, 0x61, 0x6a),
                Color::Rgb(0xeb, 0xcb, 0x8b),
                Color::Rgb(0xa3, 0xbe, 0x8c),
                Color::Rgb(0x88, 0xc0, 0xd0),
            ),
        ),
        (
            "monochrome".to_string(),
            ThemeColors::new(
                Color::Rgb(0xc0, 0xc0, 0xc0),
                Color::Rgb(0x2a, 0x2a, 0x2a),
                Color::Rgb(0xff, 0xff, 0xff),
                Color::Rgb(0x10, 0x10, 0x10),
            ),
        ),
    ]
}

/// An ordered set of named themes (built-ins first, then custom), resolved by
/// name. Cloned cheaply enough for the few entries a UI carries.
#[derive(Debug, Clone)]
pub struct ThemeRegistry {
    themes: Vec<(String, ThemeColors)>,
}

impl ThemeRegistry {
    /// A registry with only the built-in themes.
    pub fn builtin() -> Self {
        Self {
            themes: builtin_themes(),
        }
    }

    /// Adds custom themes, each replacing a built-in of the same name in place
    /// (keeping its position) or appended after the built-ins.
    #[must_use]
    pub fn with_custom<I>(mut self, custom: I) -> Self
    where
        I: IntoIterator<Item = (String, ThemeColors)>,
    {
        for (name, colors) in custom {
            match self.themes.iter_mut().find(|(n, _)| n == &name) {
                Some(entry) => entry.1 = colors,
                None => self.themes.push((name, colors)),
            }
        }
        self
    }

    /// The colors for `name`, if present.
    pub fn get(&self, name: &str) -> Option<ThemeColors> {
        self.themes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, colors)| *colors)
    }

    /// The colors for `name`, falling back to the default theme when unknown.
    pub fn resolve(&self, name: &str) -> ThemeColors {
        self.get(name)
            .or_else(|| self.get(DEFAULT_THEME))
            .or_else(|| self.themes.first().map(|(_, colors)| *colors))
            .unwrap_or_else(|| {
                ThemeColors::new(
                    Color::Default,
                    Color::Default,
                    Color::Default,
                    Color::Default,
                )
            })
    }

    /// Whether a theme with `name` exists.
    pub fn contains(&self, name: &str) -> bool {
        self.themes.iter().any(|(n, _)| n == name)
    }

    /// All theme names in order.
    pub fn names(&self) -> Vec<&str> {
        self.themes.iter().map(|(n, _)| n.as_str()).collect()
    }

    /// The name of the theme after `current` (wrapping). Unknown `current`
    /// starts from the first theme.
    pub fn next(&self, current: &str) -> String {
        if self.themes.is_empty() {
            return current.to_string();
        }
        let index = self
            .themes
            .iter()
            .position(|(n, _)| n == current)
            .unwrap_or(0);
        let next = (index + 1) % self.themes.len();
        self.themes[next].0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_have_distinct_accents() {
        let registry = ThemeRegistry::builtin();
        let default = registry.resolve("default").accent;
        let nord = registry.resolve("nord").accent;
        let mono = registry.resolve("monochrome").accent;
        assert_ne!(default, nord);
        assert_ne!(nord, mono);
        assert_ne!(default, mono);
    }

    #[test]
    fn next_cycles_through_all_and_wraps() {
        let registry = ThemeRegistry::builtin();
        assert_eq!(registry.next("default"), "nord");
        assert_eq!(registry.next("nord"), "monochrome");
        assert_eq!(registry.next("monochrome"), "default");
    }

    #[test]
    fn resolve_falls_back_to_default_for_unknown_name() {
        let registry = ThemeRegistry::builtin();
        assert_eq!(registry.resolve("nope"), registry.resolve("default"));
    }

    #[test]
    fn custom_theme_overrides_builtin_in_place() {
        let custom = ThemeColors::new(
            Color::Rgb(1, 2, 3),
            Color::Rgb(4, 5, 6),
            Color::Rgb(7, 8, 9),
            Color::Rgb(10, 11, 12),
        );
        let registry = ThemeRegistry::builtin()
            .with_custom([("nord".to_string(), custom)]);
        assert_eq!(registry.resolve("nord").accent, Color::Rgb(1, 2, 3));
        // Order preserved: nord stays the second entry.
        assert_eq!(registry.names(), vec!["default", "nord", "monochrome"]);
    }

    #[test]
    fn custom_theme_is_appended_and_cycles() {
        let custom = ThemeColors::new(
            Color::Rgb(1, 2, 3),
            Color::Rgb(4, 5, 6),
            Color::Rgb(7, 8, 9),
            Color::Rgb(10, 11, 12),
        );
        let registry = ThemeRegistry::builtin()
            .with_custom([("dracula".to_string(), custom)]);
        assert!(registry.contains("dracula"));
        // dracula is appended after the built-ins, so it joins the cycle.
        assert_eq!(registry.next("monochrome"), "dracula");
        assert_eq!(registry.next("dracula"), "default");
    }

    #[test]
    fn omitted_semantics_default_to_universal_values() {
        let colors = ThemeColors::new(
            Color::Rgb(1, 1, 1),
            Color::Rgb(2, 2, 2),
            Color::Rgb(3, 3, 3),
            Color::Rgb(4, 4, 4),
        );
        assert_eq!(colors.error, DEFAULT_ERROR);
        assert_eq!(colors.success, DEFAULT_SUCCESS);
    }
}
