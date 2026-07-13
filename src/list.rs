//! A selectable, vertically scrollable list with a scrollbar.
//!
//! The `position/total` indicator never overlays a row. Where there is a frame
//! it lives in its bottom border: [`render_boxed`] hands it to its own box, and
//! a list inside a popup gets it from the popup's frame (see
//! [`crate::chrome::render_badge`]). Where there is none, [`render_counted`]
//! keeps the bottom row free for it. Plain [`render`] draws no indicator at all.

use std::cell::Cell;

use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

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
/// keep it visible. Returns the viewport height (the number of visible rows),
/// so a stateful caller can drive page-wise navigation.
///
/// Callers build each row's content (and any per-row styling such as dimming);
/// this widget overlays the selection highlight (a subtle `selection` tint) and
/// the scrollbar. Whoever owns the surrounding frame owns the position badge.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    view: ListView,
) -> usize {
    render_core(frame, area, skin, view)
}

/// Like [`render`], but keeps the bottom row free for a `position/total` badge
/// in its right-hand corner — for a list with no frame to hang one on. The
/// list's viewport is therefore one row shorter than `area`. Returns that
/// viewport height (the badge row already subtracted).
///
/// Content wins over the badge: an area too short to spare a row (one row or
/// less) renders like plain [`render`]. An empty list gets no badge either.
pub fn render_counted(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    view: ListView,
) -> usize {
    // A badge row would leave no room for a single entry.
    if area.height <= 1 {
        return render_core(frame, area, skin, view);
    }
    let badge = chrome::position_badge(view.selected, view.rows.len());
    let rows = Rect {
        height: area.height - 1,
        ..area
    };
    let viewport = render_core(frame, rows, skin, view);
    chrome::render_corner_badge(frame, area, skin, &badge);
    viewport
}

/// Like [`render`], but wrapped in a rounded box (see [`chrome::BoxDecor`]) when
/// `force` is set; the box's bottom-right badge then shows `position/total`.
/// Without `force` it behaves exactly like [`render`]. Returns the inner
/// viewport height (the visible row count).
pub fn render_boxed(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    view: ListView,
    decor: &chrome::BoxDecor,
    force: bool,
) -> usize {
    if force {
        let badge = chrome::position_badge(view.selected, view.rows.len());
        let inner = chrome::framed_decor(frame, area, skin, decor, &badge);
        render_core(frame, inner, skin, view)
    } else {
        render(frame, area, skin, view)
    }
}

/// Draws the list rows (with the selection highlight) and the scrollbar,
/// returning the viewport height (the number of visible rows).
fn render_core(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    view: ListView,
) -> usize {
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
    viewport
}
