//! Vertical scrollbar rendering.

use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use super::style;

/// Draws a dim vertical scrollbar at the right edge of `area`, but only when
/// the content overflows the viewport.
pub fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    total: usize,
    offset: usize,
    viewport: usize,
) {
    if total <= viewport {
        return;
    }
    // ratatui only reports the bottom once position == content_length - 1, so
    // the scrollable range is total - viewport + 1, not total.
    let content_length = total.saturating_sub(viewport).saturating_add(1);
    let mut state = ScrollbarState::new(content_length).position(offset);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .style(style::dim());
    frame.render_stateful_widget(scrollbar, area, &mut state);
}
