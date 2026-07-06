//! The resolved color palette and its construction from a theme plus overrides.
//!
//! The full color set is declared once via [`define_palette!`], which generates
//! the [`Palette`] struct, the [`ColorOverrides`] struct, [`Palette::entries`]
//! and [`Palette::KEYS`] from that single list, so the set cannot drift across
//! the struct, the config keys and the preview. [`Palette::resolve`] holds the
//! (hand-written) derivation logic.

use super::{color::Color, color::parse_color, theme_set::ThemeColors};

/// `OKLab` lightness drop for the muted accent.
const ACCENT_DIM: f32 = 0.15;
/// Chroma boost for the vivid accent.
const ACCENT_VIVID: f32 = 0.25;
/// `OKLab` lightness drop for secondary (dimmed) text.
const FOREGROUND_DIM: f32 = 0.30;
/// How far the selection background is mixed from `surface` toward `accent`, so
/// a selected row stands out against the content it sits on.
const SELECTION_MIX: f32 = 0.35;
/// `OKLab` lightness rise from `surface` for the resting / active input fills,
/// so a text field stands out from the content it sits on (more so when active).
const INPUT_BG: f32 = 0.10;
const INPUT_BG_ACTIVE: f32 = 0.22;

/// Declares the full palette color set once and generates the [`Palette`] and
/// [`ColorOverrides`] structs plus [`Palette::entries`]/[`Palette::KEYS`]. The
/// derivation of each color lives in [`Palette::resolve`].
macro_rules! define_palette {
    ($( $(#[$meta:meta])* $field:ident ),+ $(,)?) => {
        /// Resolved UI colors, built once and shared so no view reaches for a
        /// global or a raw literal. Framework-agnostic: a UI layer maps these to
        /// its own types.
        #[derive(Debug, Clone, Copy)]
        pub struct Palette {
            $( $(#[$meta])* pub $field: Color, )+
        }

        /// Optional per-color overrides layered over a theme. An empty string
        /// keeps the theme/derived color; a parseable value replaces it.
        #[derive(Debug, Default, Clone)]
        pub struct ColorOverrides<'a> {
            $( pub $field: &'a str, )+
        }

        impl<'a> ColorOverrides<'a> {
            /// Builds overrides from a `name -> value` lookup (an unknown name
            /// yields the empty string, i.e. "no override"). Keeps the config
            /// side free of a per-field list — the field set lives only here.
            pub fn from_lookup(lookup: impl Fn(&str) -> &'a str) -> Self {
                Self { $( $field: lookup(stringify!($field)), )+ }
            }
        }

        impl Palette {
            /// Every palette color as `(name, color)`, in declaration order.
            pub fn entries(&self) -> Vec<(&'static str, Color)> {
                vec![ $( (stringify!($field), self.$field), )+ ]
            }

            /// All palette color names, in declaration order.
            pub const KEYS: &'static [&'static str] =
                &[ $( stringify!($field), )+ ];
        }
    };
}

define_palette! {
    accent,
    /// Muted accent, derived from `accent`.
    accent_dim,
    /// Saturated accent, derived from `accent`.
    accent_vivid,
    /// Primary text color.
    foreground,
    /// Secondary/dimmed text, derived from `foreground`.
    foreground_dim,
    /// Full-screen background.
    background,
    /// Header bar background.
    header,
    /// Footer bar background.
    footer,
    /// Elevated panel background (e.g. table headers).
    panel,
    /// Content surface for tables and lists.
    surface,
    /// Selected-row / active background, derived from `surface` + `accent`.
    selection,
    /// Block-cursor color, derived from `accent`.
    cursor,
    /// Resting text-input fill, derived from `surface`.
    input_bg,
    /// Active (editing) text-input fill, derived from `surface`.
    input_bg_active,
    /// Border/line color.
    border,
    success,
    warning,
    error,
    info,
}

impl Palette {
    /// Builds a palette from a theme's [`ThemeColors`] with `overrides` layered
    /// on top: each non-empty, parseable override replaces the theme/derived
    /// color. The variant, selection, cursor and input colors are derived from
    /// the resolved base colors before any explicit override is applied.
    pub fn resolve(base: ThemeColors, overrides: &ColorOverrides<'_>) -> Self {
        let get = |value: &str, fallback: Color| {
            parse_color(value).unwrap_or(fallback)
        };

        let accent = get(overrides.accent, base.accent);
        let foreground = get(overrides.foreground, base.foreground);
        let background = get(overrides.background, base.background);
        let surface = get(overrides.surface, base.surface);

        Self {
            accent,
            accent_dim: get(overrides.accent_dim, accent.darken(ACCENT_DIM)),
            accent_vivid: get(
                overrides.accent_vivid,
                accent.vivid(ACCENT_VIVID),
            ),
            foreground,
            foreground_dim: get(
                overrides.foreground_dim,
                foreground.darken(FOREGROUND_DIM),
            ),
            background,
            header: get(overrides.header, base.header),
            footer: get(overrides.footer, base.footer),
            panel: get(overrides.panel, base.panel),
            surface,
            selection: get(
                overrides.selection,
                surface.mix(accent, SELECTION_MIX),
            ),
            cursor: get(overrides.cursor, accent),
            input_bg: get(overrides.input_bg, surface.lighten(INPUT_BG)),
            input_bg_active: get(
                overrides.input_bg_active,
                surface.lighten(INPUT_BG_ACTIVE),
            ),
            border: get(overrides.border, base.border),
            success: get(overrides.success, base.success),
            warning: get(overrides.warning, base.warning),
            error: get(overrides.error, base.error),
            info: get(overrides.info, base.info),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{super::theme_set::ThemeRegistry, *};

    fn base() -> ThemeColors {
        ThemeRegistry::builtin().resolve("default")
    }

    #[test]
    fn resolve_without_overrides_uses_theme_colors() {
        let palette = Palette::resolve(base(), &ColorOverrides::default());
        assert_eq!(palette.accent, base().accent);
        assert_eq!(palette.background, base().background);
        assert_eq!(palette.header, base().header);
        assert_eq!(palette.surface, base().surface);
    }

    #[test]
    fn non_empty_override_wins_over_theme() {
        let overrides = ColorOverrides {
            accent: "#010203",
            ..ColorOverrides::default()
        };
        let palette = Palette::resolve(base(), &overrides);
        assert_eq!(palette.accent, Color::Rgb(1, 2, 3));
        assert_eq!(palette.foreground, base().foreground);
    }

    #[test]
    fn empty_or_invalid_override_keeps_theme_color() {
        let overrides = ColorOverrides {
            accent: "",
            header: "nope",
            ..ColorOverrides::default()
        };
        let palette = Palette::resolve(base(), &overrides);
        assert_eq!(palette.accent, base().accent);
        assert_eq!(palette.header, base().header);
    }

    #[test]
    fn semantic_colors_carry_through_from_the_theme() {
        let palette = Palette::resolve(base(), &ColorOverrides::default());
        assert_eq!(palette.error, base().error);
        assert_eq!(palette.success, base().success);
        assert_eq!(palette.info, base().info);
    }

    #[test]
    fn variant_colors_are_derived_from_the_resolved_base() {
        let palette = Palette::resolve(base(), &ColorOverrides::default());
        assert_eq!(palette.accent_dim, base().accent.darken(ACCENT_DIM));
        assert_eq!(palette.accent_vivid, base().accent.vivid(ACCENT_VIVID));
        assert_eq!(
            palette.foreground_dim,
            base().foreground.darken(FOREGROUND_DIM),
        );
        assert_eq!(palette.cursor, base().accent);
        assert_ne!(palette.accent_dim, palette.accent);
    }

    #[test]
    fn selection_is_between_background_and_accent() {
        let palette = Palette::resolve(base(), &ColorOverrides::default());
        assert_ne!(palette.selection, palette.background);
        assert_ne!(palette.selection, palette.accent);
    }

    #[test]
    fn derived_colors_honor_overrides() {
        let overrides = ColorOverrides {
            selection: "#010203",
            cursor: "#040506",
            ..ColorOverrides::default()
        };
        let palette = Palette::resolve(base(), &overrides);
        assert_eq!(palette.selection, Color::Rgb(1, 2, 3));
        assert_eq!(palette.cursor, Color::Rgb(4, 5, 6));
    }

    #[test]
    fn entries_cover_every_field_once() {
        let palette = Palette::resolve(base(), &ColorOverrides::default());
        let entries = palette.entries();
        assert_eq!(entries.len(), Palette::KEYS.len());
        let mut names: Vec<&str> =
            entries.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Palette::KEYS.len());
    }
}
