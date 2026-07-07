//! A selectable, vertically scrollable list with a scrollbar and a bottom-right
//! `position/total` badge.

use std::cell::Cell;

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{chrome, nav, scroll, style};
use crate::theme::Skin;

/// The content and cursor state a list renders: the built `rows`, the
/// `selected` index to highlight, and a `Cell` persisting the scroll `offset`
/// across frames.
pub struct ListView<'a> {
    /// The pre-built row content (callers apply any per-row styling).
    pub rows: Vec<Line<'static>>,
    /// The index of the row to highlight.
    pub selected: usize,
    /// The scroll offset, kept across frames so the list scrolls smoothly.
    pub offset: &'a Cell<usize>,
}

/// Renders `view` in `area`, highlighting the selected row and scrolling to
/// keep it visible. A `position/total` badge is drawn in the bottom-right
/// corner.
///
/// Callers build each row's content (and any per-row styling such as dimming);
/// this widget overlays the selection highlight (a subtle `selection` tint), the
/// scrollbar and the position badge.
pub fn render(frame: &mut Frame, area: Rect, skin: &Skin, view: ListView) {
    let total = view.rows.len();
    let selected = view.selected;
    render_core(frame, area, skin, view);
    render_position(frame, area, skin, selected, total);
}

/// Like [`render`], but wrapped in a rounded box (see [`chrome::BoxDecor`]) when
/// `force` is set; the box's bottom-right badge then shows `position/total`.
/// Without `force` it behaves exactly like [`render`].
pub fn render_boxed(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    view: ListView,
    decor: &chrome::BoxDecor,
    force: bool,
) {
    if force {
        // The box carries the badge in its border, so the inner list skips its
        // own overlay (`render_core`) to avoid drawing it twice.
        let badge = position_text(view.selected, view.rows.len());
        let inner = chrome::framed_decor(frame, area, skin, decor, &badge);
        render_core(frame, inner, skin, view);
    } else {
        render(frame, area, skin, view);
    }
}

/// Draws the list rows (with the selection highlight) and the scrollbar.
fn render_core(frame: &mut Frame, area: Rect, skin: &Skin, view: ListView) {
    let viewport = area.height as usize;
    let total = view.rows.len();
    let selected = view.selected;
    let scroll = nav::ScrollView {
        total,
        offset: view.offset.get(),
        viewport,
    };
    let visible_offset = nav::keep_visible(scroll, selected);
    view.offset.set(visible_offset);

    let highlight = style::bg(skin.palette.selection);

    let visible: Vec<Line> = view
        .rows
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
    scroll::render_scrollbar(
        frame,
        area,
        skin,
        nav::ScrollView {
            total,
            offset: visible_offset,
            viewport,
        },
    );
}

/// The `position/total` label (1-based), e.g. `"3/12"`.
fn position_text(selected: usize, total: usize) -> String {
    format!("{}/{}", selected + 1, total)
}

/// Overlays a `position/total` chip in the bottom-right of `area`, left of the
/// scrollbar column. Nothing is drawn for an empty list or a too-narrow area.
fn render_position(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    selected: usize,
    total: usize,
) {
    if total == 0 || area.height == 0 || area.width == 0 {
        return;
    }
    let label = format!(" {} ", position_text(selected, total));
    let width = label.len() as u16;
    // Leave the rightmost column for the scrollbar when the list overflows.
    let reserve = u16::from(total > area.height as usize);
    if width + reserve > area.width {
        return;
    }
    let chip = Rect {
        x: area.x + area.width - width - reserve,
        y: area.y + area.height - 1,
        width,
        height: 1,
    };
    let style = style::fg(skin.palette.foreground)
        .bg(style::to_ratatui(skin.palette.panel));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(label, style))),
        chip,
    );
}
