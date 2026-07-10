//! Scrollbar rendering (vertical and horizontal).
//!
//! [`render_scrollbar`] owns a whole [`Rect`] and is the usual way in. A box
//! that wraps its own text has no spare column to give away: [`row_indicator`]
//! hands back the thumb/track cell for one visual row, to ride along inside a
//! `Line` instead of overdrawing the content.

use ratatui::{
    Frame,
    layout::Rect,
    text::Span,
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use super::nav::ScrollView;
use super::style;
use crate::theme::Skin;

/// Draws a vertical scrollbar at the right edge of `area`, but only when the
/// content overflows the viewport. The thumb uses the dimmed foreground and the
/// track the border color, so both stay visible.
pub fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    view: ScrollView,
) {
    render(frame, area, skin, ScrollbarOrientation::VerticalRight, view);
}

/// Draws a horizontal scrollbar along the bottom edge of `area`, but only when
/// the content overflows the viewport. Mirrors [`render_scrollbar`] so the two
/// bars read alike; here `view` is measured in columns.
pub fn render_hscrollbar(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    view: ScrollView,
) {
    render(
        frame,
        area,
        skin,
        ScrollbarOrientation::HorizontalBottom,
        view,
    );
}

/// The thumb and track glyphs of an inline row indicator.
const THUMB: &str = "\u{2588}";
const TRACK: &str = "\u{2502}";

/// The right-edge indicator cell for one visual `row` of a scrollable box: the
/// thumb where `row` maps onto the current offset, a track elsewhere, and a
/// blank when the content fits.
///
/// A [`Scrollbar`] owns a whole [`Rect`]; a box that word-wraps its own text
/// instead builds each row as a [`Line`](ratatui::text::Line) and appends this
/// cell to it, so the indicator rides along with the content rather than
/// overdrawing it.
///
/// `row` is the visual row, `view.offset` the first content row shown and
/// `view.viewport` the number of rows on screen.
///
/// # Examples
///
/// ```
/// use ratada::nav::ScrollView;
/// use ratada::scroll::row_indicator;
/// use ratada::theme::{
///     ColorOverrides, GlyphVariant, Glyphs, Palette, Skin, ThemeRegistry,
/// };
///
/// let base = ThemeRegistry::builtin().resolve("default");
/// let palette = Palette::resolve(base, &ColorOverrides::default());
/// let skin = Skin::new(palette, Glyphs::new(GlyphVariant::Unicode));
///
/// let fits = ScrollView { total: 2, offset: 0, viewport: 4 };
/// assert_eq!(row_indicator(0, fits, &skin).content.as_ref(), " ");
///
/// // Scrolled to the top: the thumb sits on the first row.
/// let view = ScrollView { total: 10, offset: 0, viewport: 4 };
/// assert_eq!(row_indicator(0, view, &skin).content.as_ref(), "\u{2588}");
/// assert_eq!(row_indicator(1, view, &skin).content.as_ref(), "\u{2502}");
/// ```
#[must_use]
pub fn row_indicator(
    row: usize,
    view: ScrollView,
    skin: &Skin,
) -> Span<'static> {
    if scrollable_length(view).is_none() {
        return Span::raw(" ".to_string());
    }
    let rows = view.viewport.max(1);
    // Map the thumb across the visible rows proportionally to the offset.
    let max_offset = view.total.saturating_sub(rows).max(1);
    let thumb = view.offset * rows.saturating_sub(1) / max_offset;
    if row == thumb {
        Span::styled(THUMB.to_string(), style::fg(skin.palette.foreground_dim))
    } else {
        Span::styled(TRACK.to_string(), style::fg(skin.palette.border))
    }
}

/// The scrollable range for a `view`, or `None` when the content fits and no
/// scrollbar should be drawn.
///
/// ratatui only reports the end once `position == content_length - 1`, so the
/// range is `total - viewport + 1`, not `total`.
fn scrollable_length(view: ScrollView) -> Option<usize> {
    if view.total <= view.viewport {
        return None;
    }
    Some(view.total.saturating_sub(view.viewport).saturating_add(1))
}

/// Shared scrollbar rendering for both orientations.
fn render(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    orientation: ScrollbarOrientation,
    view: ScrollView,
) {
    // An empty area (e.g. a boxed widget squeezed into a tiny viewport) has no
    // room for a track; ratatui's scrollbar panics on it, so skip it.
    let Some(content_length) = scrollable_length(view) else {
        return;
    };
    if area.width == 0 || area.height == 0 {
        return;
    }
    // Without the viewport length ratatui draws a minimal thumb; feeding it
    // sizes the thumb in proportion to how much of the content is on screen.
    let mut state = ScrollbarState::new(content_length)
        .viewport_content_length(view.viewport)
        .position(view.offset);
    let scrollbar = Scrollbar::new(orientation)
        .begin_symbol(None)
        .end_symbol(None)
        .thumb_style(style::fg(skin.palette.foreground_dim))
        .track_style(style::fg(skin.palette.border));
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(total: usize, offset: usize, viewport: usize) -> ScrollView {
        ScrollView {
            total,
            offset,
            viewport,
        }
    }

    #[test]
    fn no_scrollbar_when_content_fits() {
        assert_eq!(scrollable_length(view(3, 0, 3)), None);
        assert_eq!(scrollable_length(view(2, 0, 5)), None);
    }

    #[test]
    fn scrollable_length_is_total_minus_viewport_plus_one() {
        assert_eq!(scrollable_length(view(10, 0, 3)), Some(8));
        assert_eq!(scrollable_length(view(4, 0, 3)), Some(2));
    }
}
