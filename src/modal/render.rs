//! Geometry and drawing shared by the modal widgets: the picker list, its
//! badge, the hint block and the box sizing.

use std::collections::HashSet;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
};

use crate::{
    chrome,
    layout::{centered_rect, fit},
    nav,
    overlay::{self},
    scroll, shortcut_hints, style,
    theme::{Palette, Skin},
};

/// Rows a modal's hint block occupies while the hints are shown: a blank
/// spacer and the hint line itself.
pub(super) const HINT_BLOCK_ROWS: u16 = 2;

/// Renders a single-column picker and returns its viewport height (visible
/// rows), for page-wise navigation.
pub(super) fn render_picker(
    frame: &mut Frame,
    skin: &Skin,
    title: &str,
    items: &[String],
    cursor: usize,
    checked: Option<(&HashSet<usize>, &str)>,
    rect: Rect,
) -> usize {
    let inner = overlay::framed(frame, rect, skin, title);
    let entries: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let prefix = check_prefix(checked, index);
            ListItem::new(Line::from(format!("{prefix}{label}")))
        })
        .collect();
    let viewport =
        render_picker_list(frame, inner, entries, items.len(), cursor, skin);
    render_picker_badge(frame, rect, skin, items.len(), cursor);
    viewport
}

/// Renders a styled-label picker and returns its viewport height (visible
/// rows), for page-wise navigation.
pub(super) fn render_styled_picker(
    frame: &mut Frame,
    skin: &Skin,
    title: &str,
    items: &[(String, Style)],
    cursor: usize,
    checked: Option<(&HashSet<usize>, &str)>,
    rect: Rect,
) -> usize {
    let inner = overlay::framed(frame, rect, skin, title);
    let entries: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(index, (label, item_style))| {
            let prefix = check_prefix(checked, index);
            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(label.clone(), *item_style),
            ]))
        })
        .collect();
    let viewport =
        render_picker_list(frame, inner, entries, items.len(), cursor, skin);
    render_picker_badge(frame, rect, skin, items.len(), cursor);
    viewport
}

/// Draws the `position/total` badge into the picker frame's bottom border.
/// `rect` is the framed box, not the list inside it.
pub(super) fn render_picker_badge(
    frame: &mut Frame,
    rect: Rect,
    skin: &Skin,
    total: usize,
    cursor: usize,
) {
    let badge = chrome::position_badge(cursor, total);
    chrome::render_badge(frame, rect, skin, &badge);
}

/// Renders a picker's list into `inner` with the cursor highlighted, then a
/// scrollbar on the right whenever the entries overflow the visible rows.
/// Returns the viewport height (the number of visible rows), so the caller can
/// drive page-wise navigation.
pub(super) fn render_picker_list(
    frame: &mut Frame,
    inner: Rect,
    entries: Vec<ListItem<'_>>,
    total: usize,
    cursor: usize,
    skin: &Skin,
) -> usize {
    let mut state = picker_state(cursor);
    frame.render_stateful_widget(picker_list(entries, skin), inner, &mut state);
    let viewport = inner.height as usize;
    scroll::render_scrollbar(
        frame,
        inner,
        skin,
        nav::ScrollView {
            total,
            offset: state.offset(),
            viewport,
        },
    );
    viewport
}

/// The check-mark (or blank) prefix for a multi-select row, or empty for a
/// single-select list.
pub(super) fn check_prefix(
    checked: Option<(&HashSet<usize>, &str)>,
    index: usize,
) -> String {
    match checked {
        Some((set, glyph)) if set.contains(&index) => format!("{glyph} "),
        Some(_) => "  ".to_string(),
        None => String::new(),
    }
}

fn picker_list<'a>(entries: Vec<ListItem<'a>>, skin: &Skin) -> List<'a> {
    List::new(entries).highlight_style(
        style::bg(skin.palette.selection).add_modifier(Modifier::BOLD),
    )
}

pub(super) fn picker_state(cursor: usize) -> ListState {
    let mut state = ListState::default();
    state.select(Some(cursor));
    state
}

/// The popup rect for the list pickers: half the width, one row per item.
pub(super) fn picker_area(area: Rect, item_count: usize) -> Rect {
    let height = fit(item_count as u16 + 2, 5, area.height.saturating_sub(2));
    let width = fit(area.width / 2, 30, area.width.saturating_sub(4));
    centered_rect(width, height, area)
}

/// The blank spacer and the hint line closing a modal body. A modal's key
/// prompt is essential, so it always shows, independent of the global F1 toggle
/// (which governs only the main-app footer).
pub(super) fn hint_block(
    items: &[(&str, &str)],
    palette: &Palette,
    width: usize,
) -> Vec<Line<'static>> {
    shortcut_hints::lines(items, palette.accent, width)
        .into_iter()
        .take(1)
        .flat_map(|hint| [Line::from(""), hint])
        .collect()
}

/// The height of a modal box whose body is one row plus a [`hint_block`]: two
/// border rows, the row itself, and the always-shown hint block.
pub(super) fn hinted_box_height() -> u16 {
    3 + shortcut_hints::footer_height(HINT_BLOCK_ROWS)
}
