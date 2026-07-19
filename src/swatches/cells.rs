//! Building the swatch cells each mode shows: the palette entries, the
//! hue/lightness grid and the grayscale ramp.

use super::{
    GRAY_STEPS, GRID_COLS, GRID_ROWS, Mode, Swatch, named::named_cells,
};
use crate::theme::{Color, Palette};

/// A full turn of hue in degrees, spread across the grid's columns.
const HUE_DEGREES: f32 = 360.0;

/// The maximum 8-bit channel value, for spreading the gray ramp.
const MAX_CHANNEL: f32 = 255.0;

/// Builds the cells and column count for `mode`.
/// The swatch cells for `mode` and the grid column count used to lay them out
/// (`1` for the single-column list modes).
pub(super) fn mode_cells(
    mode: Mode,
    palette: &[(&'static str, Color)],
    grid_light: f32,
    filter: &str,
) -> (Vec<Swatch>, usize) {
    match mode {
        Mode::Names => (named_cells(filter), 1),
        Mode::Palette => (palette_cells(palette), 1),
        Mode::Grid => (grid_cells(grid_light), GRID_COLS),
        Mode::Grays => (gray_cells(), GRAY_STEPS),
    }
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

/// The index of the cell perceptually closest to `color`.
pub(super) fn nearest(cells: &[Swatch], color: Color) -> usize {
    cells
        .iter()
        .enumerate()
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
