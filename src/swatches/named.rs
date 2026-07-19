//! The CSS-derived named-colour catalogue and the lookups over it.
//!
//! A pure data table plus the two functions that read it, kept apart from the
//! widget so the catalogue can grow without enlarging the picker.

use super::Swatch;
use crate::{fuzzy, theme::Color};

/// A curated set of named colors, independent of the active theme. CSS-derived
/// so the names are familiar.
pub const NAMED_COLORS: &[(&str, Color)] = &[
    ("Black", Color::hex("#000000")),
    ("Gray", Color::hex("#808080")),
    ("Silver", Color::hex("#c0c0c0")),
    ("White", Color::hex("#ffffff")),
    ("Slate", Color::hex("#708090")),
    ("Red", Color::hex("#e6194b")),
    ("Crimson", Color::hex("#dc143c")),
    ("Coral", Color::hex("#ff7f50")),
    ("Orange", Color::hex("#ffa500")),
    ("Gold", Color::hex("#ffd700")),
    ("Yellow", Color::hex("#ffe119")),
    ("Olive", Color::hex("#808000")),
    ("Lime", Color::hex("#bfef45")),
    ("Green", Color::hex("#3cb44b")),
    ("Mint", Color::hex("#aaffc3")),
    ("Teal", Color::hex("#469990")),
    ("Cyan", Color::hex("#22d3d3")),
    ("Sky", Color::hex("#87ceeb")),
    ("Blue", Color::hex("#4363d8")),
    ("Navy", Color::hex("#000075")),
    ("Indigo", Color::hex("#4b0082")),
    ("Violet", Color::hex("#911eb4")),
    ("Magenta", Color::hex("#f032e6")),
    ("Pink", Color::hex("#fabed4")),
    ("Rose", Color::hex("#e6007e")),
    ("Brown", Color::hex("#9a6324")),
    ("Tan", Color::hex("#d2b48c")),
    ("Beige", Color::hex("#fffac8")),
];

/// The named colors matching `filter` (all of them when it is empty).
pub(super) fn named_cells(filter: &str) -> Vec<Swatch> {
    NAMED_COLORS
        .iter()
        .filter(|(name, _)| {
            filter.is_empty() || fuzzy::score(name, filter).is_some()
        })
        .map(|(name, color)| Swatch {
            color: *color,
            name: Some((*name).to_string()),
        })
        .collect()
}

/// The name of the nearest `NAMED_COLORS` entry, with `=` for an exact match or
/// `≈` for an approximation.
pub(super) fn nearest_name(color: Color) -> (&'static str, &'static str) {
    let (distance, name) = NAMED_COLORS
        .iter()
        .map(|(name, candidate)| (candidate.distance(color), *name))
        .min_by(|first, second| first.0.total_cmp(&second.0))
        .unwrap_or((f32::INFINITY, "?"));
    let marker = if distance < 1e-4 { "=" } else { "\u{2248}" };
    (marker, name)
}
