//! A selectable, vertically scrollable list with a scrollbar.

use std::cell::Cell;

use ratatui::{
    Frame, layout::Rect, style::Modifier, text::Line, widgets::Paragraph,
};

use super::{chrome, nav, scroll, style};
use crate::theme::Skin;

/// Renders `rows` in `area`, highlighting the `selected` row and scrolling to
/// keep it visible. `offset` persists the scroll position across frames.
///
/// Callers build each row's content (and any per-row styling such as dimming);
/// this widget overlays the selection highlight and the scrollbar. The
/// highlight follows the [`Mode`](crate::theme::Mode): `Minimal` uses a subtle
/// `selection` tint, `Boxed` a bold accent-tinted bar.
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

    let highlight = if skin.is_boxed() {
        style::bg(skin.palette.accent_dim)
            .fg(style::to_ratatui(skin.palette.accent))
            .add_modifier(Modifier::BOLD)
    } else {
        style::bg(skin.palette.selection)
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

/// Like [`render`], but wrapped in a rounded box (see [`chrome::BoxDecor`]) when
/// in `Boxed` mode or when `force` is set. The badge defaults to the row count.
#[allow(clippy::too_many_arguments)]
pub fn render_boxed(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    rows: Vec<Line<'static>>,
    selected: usize,
    offset: &Cell<usize>,
    decor: &chrome::BoxDecor,
    force: bool,
) {
    let inner = if force || skin.is_boxed() {
        chrome::framed_decor(frame, area, skin, decor, &rows.len().to_string())
    } else {
        area
    };
    render(frame, inner, skin, rows, selected, offset);
}
