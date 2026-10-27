//! Building the swatch cells each mode shows: the palette entries, the
//! hue/lightness grid and the grayscale ramp.

use super::{
    GRAY_STEPS, GRID_COLS, GRID_ROWS, Mode, Swatch,
    named::{NAMED_COLORS, named_cells},
};
use crate::{
    color_picker::Clearing,
    theme::{Color, Palette},
};

/// The name of the entry that removes the color.
pub(super) const NONE_NAME: &str = "(none)";

/// A full turn of hue in degrees, spread across the grid's columns.
const HUE_DEGREES: f32 = 360.0;

/// The maximum 8-bit channel value, for spreading the gray ramp.
const MAX_CHANNEL: f32 = 255.0;

/// The swatch cells for `mode` and the grid column count used to lay them out
/// (`1` for the single-column list modes). With `clearing` on, the unfiltered
/// lists start with the `(none)` entry.
pub(super) fn mode_cells(
    mode: Mode,
    palette: &[(&'static str, Color)],
    grid_light: f32,
    filter: &str,
    clearing: Clearing,
) -> (Vec<Swatch>, usize) {
    let (cells, cols) = match mode {
        Mode::Names => (named_cells(filter), 1),
        Mode::Palette => (palette_cells(palette), 1),
        Mode::Grid => (grid_cells(grid_light), GRID_COLS),
        Mode::Grays => (gray_cells(), GRAY_STEPS),
    };
    let heads_with_none =
        clearing == Clearing::On && mode.is_list() && filter.is_empty();
    if !heads_with_none {
        return (cells, cols);
    }
    let none = Swatch {
        color: Color::Default,
        name: Some(NONE_NAME.to_string()),
    };
    (std::iter::once(none).chain(cells).collect(), cols)
}

impl Swatch {
    /// Whether this is the `(none)` entry that removes the color. Matched by
    /// name too, since a palette entry may itself be `Color::Default`.
    pub(super) fn is_none_entry(&self) -> bool {
        self.color == Color::Default && self.name.as_deref() == Some(NONE_NAME)
    }
}

/// Whether `color` is exactly one of the named colors or palette entries.
pub(super) fn is_listed(
    color: Color,
    palette: &[(&'static str, Color)],
) -> bool {
    NAMED_COLORS
        .iter()
        .chain(palette)
        .any(|(_, listed)| *listed == color)
}

/// The current theme palette entries as named swatches.
pub(super) fn palette_cells(palette: &[(&'static str, Color)]) -> Vec<Swatch> {
    palette
        .iter()
        .map(|(name, color)| Swatch {
            color: *color,
            name: Some((*name).to_string()),
        })
        .collect()
}

/// A hue x saturation grid at the `grid_light` lightness plane.
pub(super) fn grid_cells(grid_light: f32) -> Vec<Swatch> {
    let mut cells = Vec::with_capacity(GRID_COLS * GRID_ROWS);
    for row in 0..GRID_ROWS {
        let saturation = 1.0 - row as f32 / (GRID_ROWS - 1) as f32;
        for col in 0..GRID_COLS {
            let hue = col as f32 / GRID_COLS as f32 * HUE_DEGREES;
            cells.push(Swatch {
                color: Color::from_hsl(hue, saturation, grid_light),
                name: None,
            });
        }
    }
    cells
}

/// An evenly spaced black-to-white gray ramp.
pub(super) fn gray_cells() -> Vec<Swatch> {
    (0..GRAY_STEPS)
        .map(|step| {
            let value = (step as f32 / (GRAY_STEPS - 1) as f32 * MAX_CHANNEL)
                .round() as u8;
            Swatch {
                color: Color::Rgb(value, value, value),
                name: None,
            }
        })
        .collect()
}

/// The index of the cell perceptually closest to `color`, never the `(none)`
/// entry.
pub(super) fn nearest(cells: &[Swatch], color: Color) -> usize {
    cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| !cell.is_none_entry())
        .min_by(|(_, first), (_, second)| {
            first
                .color
                .distance(color)
                .total_cmp(&second.color.distance(color))
        })
        .map_or(0, |(index, _)| index)
}

/// The semantic theme colors offered by the palette mode.
pub(super) fn palette_entries(palette: &Palette) -> Vec<(&'static str, Color)> {
    vec![
        ("accent", palette.accent),
        ("accent_vivid", palette.accent_vivid),
        ("success", palette.success),
        ("warning", palette.warning),
        ("error", palette.error),
        ("info", palette.info),
        ("foreground", palette.foreground),
        ("border", palette.border),
        ("surface", palette.surface),
        ("background", palette.background),
    ]
}
