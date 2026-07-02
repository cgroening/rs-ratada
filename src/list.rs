//! A selectable, vertically scrollable list with a scrollbar.

use std::cell::Cell;

use ratatui::{
    Frame, layout::Rect, style::Modifier, text::Line, widgets::Paragraph,
};

use super::{nav, scroll, style};
use crate::theme::Skin;

/// Renders `rows` in `area`, highlighting the `selected` row and scrolling to
/// keep it visible. `offset` persists the scroll position across frames.
///
/// Callers build each row's content (and any per-row styling such as dimming);
/// this widget overlays the selection highlight and the scrollbar. The
/// highlight follows the [`Mode`](crate::theme::Mode): `Minimal` uses a subtle
/// `selection_bg` tint, `Fancy` a bold accent-tinted bar.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    rows: Vec<Line<'static>>,
    selected: usize,
    offset: &Cell<usize>,
) {
    let viewport = area.height as usize;
    let total = rows.len();
    let visible_offset =
        nav::keep_visible(offset.get(), selected, viewport, total);
    offset.set(visible_offset);

    let highlight = if skin.is_fancy() {
        style::bg(skin.palette.accent_dark)
            .fg(style::to_ratatui(skin.palette.accent))
            .add_modifier(Modifier::BOLD)
    } else {
        style::bg(skin.palette.selection_bg)
    };

    let visible: Vec<Line> = rows
        .into_iter()
        .enumerate()
        .skip(visible_offset)
        .take(viewport)
        .map(|(index, line)| {
            if index == selected {
                line.style(highlight)
            } else {
                line
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(visible), area);
    scroll::render_scrollbar(frame, area, total, visible_offset, viewport);
}
