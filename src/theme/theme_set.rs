//! Named color themes and the registry that resolves a theme name to colors.
//!
//! A theme is the color axis of the UI. Built-in themes live here; the host may
//! add custom themes from config, all reachable by name through a
//! [`ThemeRegistry`]. Each
//! theme provides a [`ThemeColors`] base that the [`Palette`](super::Palette)
//! builds on. A custom theme may set only some colors; the rest are derived
//! from `accent`/`foreground`/`background` via [`ThemeColors::derived`].

use super::color::Color;

/// The default theme name, used as the fallback when a name is unknown.
pub const DEFAULT_THEME: &str = "default";

/// Universal defaults for colors a (custom) theme omits. These reuse the
/// `default` theme's values ([`DEFAULT_COLORS`]) as the single source of truth,
/// so the fallbacks never drift from the built-in default.
const DEFAULT_ACCENT: Color = DEFAULT_COLORS.accent;
const DEFAULT_BACKGROUND: Color = DEFAULT_COLORS.background;
const DEFAULT_FOREGROUND: Color = DEFAULT_COLORS.foreground;
const DEFAULT_SUCCESS: Color = DEFAULT_COLORS.success;
const DEFAULT_WARNING: Color = DEFAULT_COLORS.warning;
const DEFAULT_ERROR: Color = DEFAULT_COLORS.error;
const DEFAULT_INFO: Color = DEFAULT_COLORS.info;

/// How omitted neutral backgrounds are derived from `background` (`OKLab` L
/// step). `CHROME_DARKEN` darkens the header and footer bars alike; the others
/// lighten the panel/surface/border layers.
const CHROME_DARKEN: f32 = 0.03;
const PANEL_LIGHTEN: f32 = 0.03;
const SURFACE_LIGHTEN: f32 = 0.10;
const BORDER_LIGHTEN: f32 = 0.12;

/// How far an omitted `border_focus` is lifted above `border` (`OKLab` L step).
///
/// A focused field usually brightens its own fill, and a fixed border loses
/// most of its contrast against it. Lifting the border with the fill keeps the
/// frame legible in both states. [`Palette::resolve`](super::Palette::resolve)
/// applies the same step when a host overrides `border` alone.
pub(super) const BORDER_FOCUS_LIGHTEN: f32 = 0.15;

/// The base colors a theme contributes before any config override is applied.
/// Every built-in theme sets all of them explicitly; a custom theme may leave
/// some to [`ThemeColors::derived`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    /// The single accent hue for headers, active tabs and highlights.
    pub accent: Color,
    /// The primary text color.
    pub foreground: Color,
    /// The base window background.
    pub background: Color,
    /// The header bar background (a darkened `background`).
    pub header: Color,
    /// The footer bar background (a darkened `background`).
    pub footer: Color,
    /// The panel background, one step lighter than `background`.
    pub panel: Color,
    /// The raised surface background (selection/focus tints build on it).
    pub surface: Color,
    /// The border color for boxes and separators.
    pub border: Color,
    /// The border color of a *focused* box, lifted above `border` so the frame
    /// keeps its contrast against the brighter fill a focused field draws.
    /// Omitted, it follows `border`; see [`ThemeColors::from_lookup`].
    pub border_focus: Color,
    /// The semantic color for success/positive states.
    pub success: Color,
    /// The semantic color for warnings.
    pub warning: Color,
    /// The semantic color for errors/negative states.
    pub error: Color,
    /// The semantic color for informational accents.
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
        let border = background.lighten(BORDER_LIGHTEN);
        Self {
            accent,
            foreground,
            background,
            header: background.darken(CHROME_DARKEN),
            footer: background.darken(CHROME_DARKEN),
            panel: background.lighten(PANEL_LIGHTEN),
            surface: background.lighten(SURFACE_LIGHTEN),
            border,
            border_focus: border.lighten(BORDER_FOCUS_LIGHTEN),
            success: DEFAULT_SUCCESS,
            warning: DEFAULT_WARNING,
            error: DEFAULT_ERROR,
            info: DEFAULT_INFO,
        }
    }

    /// A theme derived from `accent`/`background` with the universal light
    /// `DEFAULT_FOREGROUND`.
    pub fn from_accent(accent: Color, background: Color) -> Self {
        Self::derived(accent, DEFAULT_FOREGROUND, background)
    }

    /// Builds theme colors from a `name -> color` lookup: `accent`/`foreground`/
    /// `background` (or universal defaults) seed the derived neutrals and
    /// semantics via [`ThemeColors::derived`], then any other present base color
    /// overrides its derived value. Lets a custom theme set only a few colors.
    ///
    /// `border_focus` is special: a theme that sets `border` but leaves the
    /// focus color out gets it re-derived from *its* border, so the pair never
    /// drifts apart. An explicit `border_focus` always wins.
    ///
    /// [`ThemeColors::KEYS`] lists exactly the names read here.
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
        colors.border_focus = lookup("border_focus")
            .unwrap_or_else(|| colors.border.lighten(BORDER_FOCUS_LIGHTEN));
        set("success", &mut colors.success);
        set("warning", &mut colors.warning);
        set("error", &mut colors.error);
        set("info", &mut colors.info);
        colors
    }

    /// Every color name [`ThemeColors::from_lookup`] reads, in declaration
    /// order.
    ///
    /// A host validating a `[themes.<name>]` table must check against these,
    /// not against [`Palette::KEYS`](super::Palette::KEYS): the palette carries
    /// derived colors (`selection`, `cursor`, `input_bg`, …) that a theme
    /// cannot contribute, and accepting them would silently drop the value.
    pub const KEYS: &'static [&'static str] = &[
        "accent",
        "foreground",
        "background",
        "header",
        "footer",
        "panel",
        "surface",
        "border",
        "border_focus",
        "success",
        "warning",
        "error",
        "info",
    ];
}

/// The built-in `default` theme (teal accent on a near-black background).
const DEFAULT_COLORS: ThemeColors = ThemeColors {
    accent: Color::hex("#8bd3cd"),
    foreground: Color::hex("#e5e5e5"),
    background: Color::hex("#151515"),
    header: Color::hex("#101010"),
    footer: Color::hex("#101010"),
    panel: Color::hex("#1c1c1c"),
    surface: Color::hex("#303030"),
    border: Color::hex("#606060"),
    // `border.lighten(BORDER_FOCUS_LIGHTEN)`, spelled out because `lighten` is
    // not `const`. Pinned by `built_in_focus_borders_match_the_derivation`.
    border_focus: Color::hex("#8c8c8c"),
    success: Color::hex("#a3c995"),
    warning: Color::hex("#ded483"),
    error: Color::hex("#d57b76"),
    info: Color::hex("#7fb3d4"),
};

/// The built-in `monochrome` theme (grayscale).
const MONOCHROME_COLORS: ThemeColors = ThemeColors {
    accent: Color::hex("#c0c0c0"),
    foreground: Color::hex("#e5e5e5"),
    background: Color::hex("#101010"),
    header: Color::hex("#0a0a0a"),
    footer: Color::hex("#0a0a0a"),
    panel: Color::hex("#1a1a1a"),
    surface: Color::hex("#333333"),
    border: Color::hex("#5a5a5a"),
    border_focus: Color::hex("#858585"),
    success: Color::hex("#b8b8b8"),
    warning: Color::hex("#d0d0d0"),
    error: Color::hex("#9a9a9a"),
    info: Color::hex("#a8a8a8"),
};

/// The built-in themes, in cycling order. The first entry is the default.
fn builtin_themes() -> Vec<(String, ThemeColors)> {
    vec![
        (DEFAULT_THEME.to_string(), DEFAULT_COLORS),
        ("monochrome".to_string(), MONOCHROME_COLORS),
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
        if let Some(colors) = self.get(name) {
            return colors;
        }
        log::warn!("unknown theme {name:?}, falling back to the default");
        self.get(DEFAULT_THEME)
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
        assert_eq!(default.accent, Color::hex("#8bd3cd"));
        assert_eq!(default.foreground, Color::hex("#e5e5e5"));
        assert_eq!(default.background, Color::hex("#151515"));
        assert_eq!(default.header, Color::hex("#101010"));
        assert_eq!(default.surface, Color::hex("#303030"));
        assert_eq!(default.error, Color::hex("#d57b76"));
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
    fn an_omitted_focus_border_is_lifted_from_the_border() {
        let theme = ThemeColors::from_lookup(|_| None);
        assert_eq!(
            theme.border_focus,
            theme.border.lighten(BORDER_FOCUS_LIGHTEN),
        );
        assert!(theme.border_focus.luminance() > theme.border.luminance());
    }

    #[test]
    fn a_theme_that_sets_only_border_drags_the_focus_border_with_it() {
        // Keeping the derived focus colour here would sink the focused frame
        // into the theme's own border.
        let border = Color::hex("#4a4a4a");
        let theme = ThemeColors::from_lookup(|name| match name {
            "border" => Some(border),
            _ => None,
        });
        assert_eq!(theme.border, border);
        assert_eq!(theme.border_focus, border.lighten(BORDER_FOCUS_LIGHTEN));
    }

    #[test]
    fn an_explicit_focus_border_wins_over_the_derivation() {
        let theme = ThemeColors::from_lookup(|name| match name {
            "border" => Some(Color::hex("#4a4a4a")),
            "border_focus" => Some(Color::Rgb(1, 2, 3)),
            _ => None,
        });
        assert_eq!(theme.border_focus, Color::Rgb(1, 2, 3));
    }

    #[test]
    fn built_in_focus_borders_match_the_derivation() {
        // `Color::lighten` is not `const`, so the two built-ins spell their
        // focus border out. They must not drift from the rule.
        for colors in [DEFAULT_COLORS, MONOCHROME_COLORS] {
            assert_eq!(
                colors.border_focus,
                colors.border.lighten(BORDER_FOCUS_LIGHTEN),
                "a built-in theme's border_focus drifted",
            );
        }
    }

    #[test]
    fn keys_name_exactly_the_colors_from_lookup_reads() {
        use std::cell::RefCell;

        let seen = RefCell::new(Vec::new());
        let _ = ThemeColors::from_lookup(|name| {
            seen.borrow_mut().push(name.to_string());
            None
        });

        let mut seen = seen.into_inner();
        seen.sort_unstable();
        seen.dedup();
        let mut keys: Vec<String> =
            ThemeColors::KEYS.iter().map(|k| (*k).to_string()).collect();
        keys.sort_unstable();

        assert_eq!(seen, keys, "ThemeColors::KEYS drifted from from_lookup");
    }

    #[test]
    fn keys_include_the_focus_border() {
        assert!(ThemeColors::KEYS.contains(&"border_focus"));
    }

    #[test]
    fn derived_theme_fills_neutrals_and_semantics() {
        let theme = ThemeColors::from_accent(
            Color::hex("#8bd3cd"),
            Color::hex("#151515"),
        );
        assert_eq!(theme.foreground, DEFAULT_FOREGROUND);
        assert_eq!(theme.success, DEFAULT_SUCCESS);
        // Neutral backgrounds derive from the background (distinct steps).
        assert_ne!(theme.panel, theme.background);
        assert_ne!(theme.surface, theme.panel);
    }
}
