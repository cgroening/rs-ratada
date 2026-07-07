//! Scrollbar rendering (vertical and horizontal).

use ratatui::{
    Frame,
    layout::Rect,
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
    let mut state = ScrollbarState::new(content_length).position(view.offset);
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
