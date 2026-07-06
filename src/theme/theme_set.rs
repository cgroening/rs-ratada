//! Named color themes and the registry that resolves a theme name to colors.
//!
//! A theme is the color axis of the UI, independent of the layout
//! [`Mode`](super::Mode). Built-in themes live here; the host may add custom
//! themes from config, all reachable by name through a [`ThemeRegistry`]. Each
//! theme provides a [`ThemeColors`] base that the [`Palette`](super::Palette)
//! builds on. A custom theme may set only some colors; the rest are derived
//! from `accent`/`foreground`/`background` via [`ThemeColors::derived`].

use super::color::Color;

/// The default theme name, used as the fallback when a name is unknown.
pub const DEFAULT_THEME: &str = "default";

/// Universal defaults for colors a (custom) theme omits.
const DEFAULT_ACCENT: Color = Color::Rgb(0x8b, 0xd3, 0xcd);
const DEFAULT_BACKGROUND: Color = Color::Rgb(0x15, 0x15, 0x15);
const DEFAULT_FOREGROUND: Color = Color::Rgb(0xe5, 0xe5, 0xe5);
const DEFAULT_SUCCESS: Color = Color::Rgb(0xa3, 0xc9, 0x95);
const DEFAULT_WARNING: Color = Color::Rgb(0xde, 0xd4, 0x83);
const DEFAULT_ERROR: Color = Color::Rgb(0xd5, 0x7b, 0x76);
const DEFAULT_INFO: Color = Color::Rgb(0x7f, 0xb3, 0xd4);

/// How omitted neutral backgrounds are derived from `background` (`OKLab` L step).
const HEADER_DARKEN: f32 = 0.03;
const PANEL_LIGHTEN: f32 = 0.03;
const SURFACE_LIGHTEN: f32 = 0.10;
const BORDER_LIGHTEN: f32 = 0.12;

/// The base colors a theme contributes before any config override is applied.
/// Every built-in theme sets all of them explicitly; a custom theme may leave
/// some to [`ThemeColors::derived`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    pub accent: Color,
    pub foreground: Color,
    pub background: Color,
    pub header: Color,
    pub footer: Color,
    pub panel: Color,
    pub surface: Color,
    pub border: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
}

impl ThemeColors {
    /// A theme built from just `accent`, `foreground` and `background`: the
    /// neutral backgrounds and the border are derived from `background`, and the
    /// semantic colors fall back to universal defaults. Used for custom themes
    /// that set only a few colors.
    pub fn derived(
        accent: Color,
        foreground: Color,
        background: Color,
    ) -> Self {
        Self {
            accent,
            foreground,
            background,
            header: background.darken(HEADER_DARKEN),
            footer: background.darken(HEADER_DARKEN),
            panel: background.lighten(PANEL_LIGHTEN),
            surface: background.lighten(SURFACE_LIGHTEN),
            border: background.lighten(BORDER_LIGHTEN),
            success: DEFAULT_SUCCESS,
            warning: DEFAULT_WARNING,
            error: DEFAULT_ERROR,
            info: DEFAULT_INFO,
        }
    }

    /// A theme derived from `accent`/`background` with the universal light
    /// [`DEFAULT_FOREGROUND`].
    pub fn from_accent(accent: Color, background: Color) -> Self {
        Self::derived(accent, DEFAULT_FOREGROUND, background)
    }

    /// Builds theme colors from a `name -> color` lookup: `accent`/`foreground`/
    /// `background` (or universal defaults) seed the derived neutrals and
    /// semantics via [`ThemeColors::derived`], then any other present base color
    /// overrides its derived value. Lets a custom theme set only a few colors.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<Color>) -> Self {
        let accent = lookup("accent").unwrap_or(DEFAULT_ACCENT);
        let foreground = lookup("foreground").unwrap_or(DEFAULT_FOREGROUND);
        let background = lookup("background").unwrap_or(DEFAULT_BACKGROUND);
        let mut colors = Self::derived(accent, foreground, background);
        let set = |name: &str, field: &mut Color| {
            if let Some(color) = lookup(name) {
                *field = color;
            }
        };
        set("header", &mut colors.header);
        set("footer", &mut colors.footer);
        set("panel", &mut colors.panel);
        set("surface", &mut colors.surface);
        set("border", &mut colors.border);
        set("success", &mut colors.success);
        set("warning", &mut colors.warning);
        set("error", &mut colors.error);
        set("info", &mut colors.info);
        colors
    }
}

/// The built-in themes, in cycling order. The first entry is the default.
fn builtin_themes() -> Vec<(String, ThemeColors)> {
    vec![
        (
            DEFAULT_THEME.to_string(),
            ThemeColors {
                accent: Color::Rgb(0x8b, 0xd3, 0xcd),
                foreground: Color::Rgb(0xe5, 0xe5, 0xe5),
                background: Color::Rgb(0x15, 0x15, 0x15),
                header: Color::Rgb(0x10, 0x10, 0x10),
                footer: Color::Rgb(0x10, 0x10, 0x10),
                panel: Color::Rgb(0x1b, 0x1b, 0x1b),
                surface: Color::Rgb(0x3e, 0x3e, 0x3e),
                border: Color::Rgb(0x4a, 0x4a, 0x4a),
                success: Color::Rgb(0xa3, 0xc9, 0x95),
                warning: Color::Rgb(0xde, 0xd4, 0x83),
                error: Color::Rgb(0xd5, 0x7b, 0x76),
                info: Color::Rgb(0x7f, 0xb3, 0xd4),
            },
        ),
        (
            "monochrome".to_string(),
            ThemeColors {
                accent: Color::Rgb(0xc0, 0xc0, 0xc0),
                foreground: Color::Rgb(0xe5, 0xe5, 0xe5),
                background: Color::Rgb(0x10, 0x10, 0x10),
                header: Color::Rgb(0x0a, 0x0a, 0x0a),
                footer: Color::Rgb(0x0a, 0x0a, 0x0a),
                panel: Color::Rgb(0x1a, 0x1a, 0x1a),
                surface: Color::Rgb(0x33, 0x33, 0x33),
                border: Color::Rgb(0x44, 0x44, 0x44),
                success: Color::Rgb(0xb8, 0xb8, 0xb8),
                warning: Color::Rgb(0xd0, 0xd0, 0xd0),
                error: Color::Rgb(0x9a, 0x9a, 0x9a),
                info: Color::Rgb(0xa8, 0xa8, 0xa8),
            },
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
                ThemeColors::from_accent(Color::Default, Color::Default)
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
    fn default_theme_has_the_specified_colors() {
        let default = ThemeRegistry::builtin().resolve("default");
        assert_eq!(default.accent, Color::Rgb(0x8b, 0xd3, 0xcd));
        assert_eq!(default.foreground, Color::Rgb(0xe5, 0xe5, 0xe5));
        assert_eq!(default.background, Color::Rgb(0x15, 0x15, 0x15));
        assert_eq!(default.header, Color::Rgb(0x10, 0x10, 0x10));
        assert_eq!(default.surface, Color::Rgb(0x3e, 0x3e, 0x3e));
        assert_eq!(default.error, Color::Rgb(0xd5, 0x7b, 0x76));
    }

    #[test]
    fn registry_holds_two_builtins() {
        assert_eq!(
            ThemeRegistry::builtin().names(),
            vec!["default", "monochrome"],
        );
    }

    #[test]
    fn next_cycles_through_all_and_wraps() {
        let registry = ThemeRegistry::builtin();
        assert_eq!(registry.next("default"), "monochrome");
        assert_eq!(registry.next("monochrome"), "default");
    }

    #[test]
    fn resolve_falls_back_to_default_for_unknown_name() {
        let registry = ThemeRegistry::builtin();
        assert_eq!(registry.resolve("nope"), registry.resolve("default"));
    }

    #[test]
    fn custom_theme_overrides_builtin_in_place() {
        let custom = ThemeColors::from_accent(
            Color::Rgb(1, 2, 3),
            Color::Rgb(10, 11, 12),
        );
        let registry = ThemeRegistry::builtin()
            .with_custom([("monochrome".to_string(), custom)]);
        assert_eq!(registry.resolve("monochrome").accent, Color::Rgb(1, 2, 3));
        assert_eq!(registry.names(), vec!["default", "monochrome"]);
    }

    #[test]
    fn custom_theme_is_appended_and_cycles() {
        let custom = ThemeColors::from_accent(
            Color::Rgb(1, 2, 3),
            Color::Rgb(10, 11, 12),
        );
        let registry = ThemeRegistry::builtin()
            .with_custom([("dracula".to_string(), custom)]);
        assert!(registry.contains("dracula"));
        assert_eq!(registry.next("monochrome"), "dracula");
        assert_eq!(registry.next("dracula"), "default");
    }

    #[test]
    fn derived_theme_fills_neutrals_and_semantics() {
        let theme = ThemeColors::from_accent(
            Color::Rgb(0x8b, 0xd3, 0xcd),
            Color::Rgb(0x15, 0x15, 0x15),
        );
        assert_eq!(theme.foreground, DEFAULT_FOREGROUND);
        assert_eq!(theme.success, DEFAULT_SUCCESS);
        // Neutral backgrounds derive from the background (distinct steps).
        assert_ne!(theme.panel, theme.background);
        assert_ne!(theme.surface, theme.panel);
    }
}
