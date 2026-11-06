//! Resolving the terminal `DIM` attribute to a chosen colour.
//!
//! How `DIM` looks is up to the terminal, so an app cannot theme it. With a
//! [`DimRender`] set, [`crate::Tui::draw`] rewrites every dimmed cell of a
//! finished frame: text in the default colour takes the configured
//! foreground, RGB text is darkened and keeps its hue. A named or indexed
//! colour has no known RGB base and keeps the attribute.
//!
//! The setting is per thread, like [`crate::quit::set_confirm`], so it is set
//! once where the app builds its theme and reaches every draw, including the
//! modals this crate opens itself.

use std::cell::Cell;

use ratatui::{
    buffer::Buffer,
    style::{Color as RatColor, Modifier},
};

use crate::{
    style,
    theme::{Color, Palette},
};

/// How dimmed cells are drawn instead of with the terminal `DIM` attribute.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DimRender {
    /// The colour of dimmed text drawn in the terminal's default colour.
    pub foreground: Color,
    /// How far dimmed RGB text is darkened toward black, from `0.0`
    /// (unchanged) to `1.0` (black).
    pub amount: f32,
}

thread_local! {
    static DIM_RENDER: Cell<Option<DimRender>> = const { Cell::new(None) };
}

/// Sets how dimmed cells are drawn on this thread; `None` (the default) leaves
/// the terminal `DIM` attribute alone.
///
/// # Examples
///
/// ```
/// use ratada::dim::{DimRender, dim_render, set_dim_render};
/// use ratada::theme::Color;
///
/// assert_eq!(dim_render(), None);
/// let render = DimRender { foreground: Color::hex("#6e6a78"), amount: 0.4 };
/// set_dim_render(Some(render));
/// assert_eq!(dim_render(), Some(render));
/// ```
pub fn set_dim_render(render: Option<DimRender>) {
    DIM_RENDER.with(|setting| setting.set(render));
}

/// Sets this thread's dim rendering from `palette`: its `dim` colour, with
/// coloured dimmed text darkened by `amount` (`0.0`-`1.0`). A `dim` of
/// [`Color::Default`] leaves the attribute to the terminal.
///
/// # Examples
///
/// ```
/// use ratada::dim::{apply_palette, dim_render};
/// use ratada::theme::{ColorOverrides, Palette, ThemeRegistry};
///
/// let base = ThemeRegistry::builtin().resolve("default");
/// let palette = Palette::resolve(base, &ColorOverrides::default());
/// apply_palette(&palette, 0.4);
/// assert_eq!(dim_render(), None, "the default theme keeps the terminal DIM");
/// ```
pub fn apply_palette(palette: &Palette, amount: f32) {
    let render = (palette.dim != Color::Default).then_some(DimRender {
        foreground: palette.dim,
        amount,
    });
    set_dim_render(render);
}

/// The dim rendering set on this thread, if any.
#[must_use]
pub fn dim_render() -> Option<DimRender> {
    DIM_RENDER.with(Cell::get)
}

/// Applies this thread's [`DimRender`] to a finished frame; does nothing when
/// none is set.
pub(crate) fn apply(buffer: &mut Buffer) {
    if let Some(render) = dim_render() {
        resolve(buffer, render);
    }
}

/// Rewrites every dimmed cell of `buffer` according to `render`.
fn resolve(buffer: &mut Buffer, render: DimRender) {
    let foreground = style::to_ratatui(render.foreground);
    let keep = 1.0 - render.amount.clamp(0.0, 1.0);
    for cell in &mut buffer.content {
        if !cell.modifier.contains(Modifier::DIM) {
            continue;
        }
        match cell.fg {
            RatColor::Reset => cell.fg = foreground,
            RatColor::Rgb(..) => cell.fg = style::darken(cell.fg, keep),
            // No RGB base to darken: the terminal keeps dimming it.
            _ => continue,
        }
        cell.modifier.remove(Modifier::DIM);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::*;

    const RENDER: DimRender = DimRender {
        foreground: Color::Rgb(110, 106, 120),
        amount: 0.5,
    };

    /// A one-cell buffer with `fg` and `modifier`.
    fn cell_buffer(fg: RatColor, modifier: Modifier) -> Buffer {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        buffer.content[0].fg = fg;
        buffer.content[0].modifier = modifier;
        buffer
    }

    #[test]
    fn dimmed_default_text_takes_the_configured_colour() {
        let mut buffer = cell_buffer(RatColor::Reset, Modifier::DIM);
        resolve(&mut buffer, RENDER);
        assert_eq!(buffer.content[0].fg, RatColor::Rgb(110, 106, 120));
        assert!(!buffer.content[0].modifier.contains(Modifier::DIM));
    }

    #[test]
    fn dimmed_rgb_text_is_darkened_and_keeps_its_other_modifiers() {
        let mut buffer = cell_buffer(
            RatColor::Rgb(200, 100, 50),
            Modifier::DIM | Modifier::CROSSED_OUT,
        );
        resolve(&mut buffer, RENDER);
        assert_eq!(buffer.content[0].fg, RatColor::Rgb(100, 50, 25));
        assert_eq!(buffer.content[0].modifier, Modifier::CROSSED_OUT);
    }

    #[test]
    fn dimmed_named_colours_keep_the_attribute() {
        let mut buffer = cell_buffer(RatColor::Green, Modifier::DIM);
        resolve(&mut buffer, RENDER);
        assert_eq!(buffer.content[0].fg, RatColor::Green);
        assert!(buffer.content[0].modifier.contains(Modifier::DIM));
    }

    #[test]
    fn cells_without_dim_are_untouched() {
        let mut buffer = cell_buffer(RatColor::Reset, Modifier::BOLD);
        resolve(&mut buffer, RENDER);
        assert_eq!(buffer.content[0].fg, RatColor::Reset);
        assert_eq!(buffer.content[0].modifier, Modifier::BOLD);
    }
}
