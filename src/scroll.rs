//! Scrollbar rendering (vertical and horizontal).

use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use super::style;
use crate::theme::Skin;

/// Draws a vertical scrollbar at the right edge of `area`, but only when the
/// content overflows the viewport. The thumb uses the dimmed foreground and the
/// track the border color, so both stay visible.
pub fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    total: usize,
    offset: usize,
    viewport: usize,
) {
    render(
        frame,
        area,
        skin,
        ScrollbarOrientation::VerticalRight,
        total,
        offset,
        viewport,
    );
}

/// Draws a horizontal scrollbar along the bottom edge of `area`, but only when
/// the content overflows the viewport. Mirrors [`render_scrollbar`] so the two
/// bars read alike; here `total`/`offset`/`viewport` are measured in columns.
pub fn render_hscrollbar(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    total: usize,
    offset: usize,
    viewport: usize,
) {
    render(
        frame,
        area,
        skin,
        ScrollbarOrientation::HorizontalBottom,
        total,
        offset,
        viewport,
    );
}

/// Shared scrollbar rendering for both orientations.
fn render(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    orientation: ScrollbarOrientation,
    total: usize,
    offset: usize,
    viewport: usize,
) {
    // An empty area (e.g. a boxed widget squeezed into a tiny viewport) has no
    // room for a track; ratatui's scrollbar panics on it, so skip it.
    if area.width == 0 || area.height == 0 || total <= viewport {
        return;
    }
    // ratatui only reports the end once position == content_length - 1, so the
    // scrollable range is total - viewport + 1, not total.
    let content_length = total.saturating_sub(viewport).saturating_add(1);
    let mut state = ScrollbarState::new(content_length).position(offset);
    let scrollbar = Scrollbar::new(orientation)
        .begin_symbol(None)
        .end_symbol(None)
        .thumb_style(style::fg(skin.palette.foreground_dim))
        .track_style(style::fg(skin.palette.border));
    frame.render_stateful_widget(scrollbar, area, &mut state);
}
