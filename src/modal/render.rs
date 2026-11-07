//! Geometry and drawing shared by the modal widgets: the picker list, its
//! badge, the hint block and the box sizing.

use std::collections::HashSet;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{HighlightSpacing, List, ListItem, ListState, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    chrome::{self, MODAL_CHROME_HEIGHT, MODAL_CHROME_WIDTH},
    layout::{centered_rect, fit},
    nav,
    overlay::{self},
    scroll, shortcut_hints, style,
    theme::{Color, Palette, Skin},
};

/// The most rows a modal's hints wrap onto when the box is narrower than
/// their line (a narrow screen); below that the box grows no further.
pub const MAX_HINT_ROWS: usize = 3;

/// The rows of a hint block on one line: the blank spacer and the hints.
const SINGLE_HINT_BLOCK: u16 = 2;

/// The narrowest list picker, however short its labels and hints.
const MIN_PICKER_WIDTH: u16 = 28 + MODAL_CHROME_WIDTH;

/// The shortest list picker: the frame, three rows and the hint block.
const MIN_PICKER_HEIGHT: u16 = MODAL_CHROME_HEIGHT + 3 + SINGLE_HINT_BLOCK;

/// The keys of a single-choice list picker.
pub(super) const SELECT_HINTS: [(&str, &str); 2] =
    [("enter", "pick"), ("esc", "cancel")];

/// What a list picker shows around its rows: the frame title, the cursor, the
/// checks of a multi-select (with their glyph) and the key hints.
pub(super) struct PickerChrome<'a> {
    pub(super) title: &'a str,
    pub(super) cursor: usize,
    pub(super) checked: Option<(&'a HashSet<usize>, &'a str)>,
    pub(super) hints: &'a [(&'a str, &'a str)],
}

/// Renders a single-column picker and returns its viewport height (visible
/// rows), for page-wise navigation.
pub(super) fn render_picker(
    frame: &mut Frame,
    skin: &Skin,
    items: &[String],
    chrome: &PickerChrome<'_>,
    rect: Rect,
) -> usize {
    let entries: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let prefix = check_prefix(chrome.checked, index);
            ListItem::new(Line::from(format!("{prefix}{label}")))
        })
        .collect();
    render_framed_picker(frame, skin, entries, chrome, rect)
}

/// Renders a styled-label picker and returns its viewport height (visible
/// rows), for page-wise navigation.
pub(super) fn render_styled_picker(
    frame: &mut Frame,
    skin: &Skin,
    items: &[(String, Style)],
    chrome: &PickerChrome<'_>,
    rect: Rect,
) -> usize {
    let entries: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(index, (label, item_style))| {
            let prefix = check_prefix(chrome.checked, index);
            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(label.clone(), *item_style),
            ]))
        })
        .collect();
    render_framed_picker(frame, skin, entries, chrome, rect)
}

/// Draws the frame, the hints, the rows and the position badge; returns the
/// viewport height.
fn render_framed_picker(
    frame: &mut Frame,
    skin: &Skin,
    entries: Vec<ListItem<'_>>,
    chrome: &PickerChrome<'_>,
    rect: Rect,
) -> usize {
    let total = entries.len();
    let inner = overlay::framed(frame, rect, skin, chrome.title);
    let list = render_picker_hints(frame, inner, skin, chrome.hints);
    let viewport =
        render_picker_list(frame, list, entries, total, chrome.cursor, skin);
    render_picker_badge(frame, rect, skin, total, chrome.cursor);
    viewport
}

/// Draws the [`hint_block`] at the bottom of a picker's `inner` area and
/// returns the rows above it for the list. A box too low for both keeps every
/// row for the list and drops the hints.
pub(super) fn render_picker_hints(
    frame: &mut Frame,
    inner: Rect,
    skin: &Skin,
    hints: &[(&str, &str)],
) -> Rect {
    let hint_rows = hint_block_height(hints, usize::from(inner.width));
    if inner.height <= hint_rows {
        return inner;
    }
    let list_height = inner.height - hint_rows;
    let hint_area = Rect {
        y: inner.y + list_height,
        height: hint_rows,
        ..inner
    };
    let lines = hint_block(hints, &skin.palette, usize::from(inner.width));
    frame.render_widget(Paragraph::new(lines), hint_area);
    Rect {
        height: list_height,
        ..inner
    }
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

/// The picker rows with the cursor row marked twice: the pointer glyph in a
/// leading column (blank on every other row, so the labels stay aligned) and
/// the selection tint. The cursor opens on the entry that currently applies,
/// so that entry is what both marks point at.
fn picker_list<'a>(entries: Vec<ListItem<'a>>, skin: &Skin) -> List<'a> {
    List::new(entries)
        .highlight_style(
            style::bg(skin.palette.selection).add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(Line::from(format!("{} ", skin.glyphs.pointer)))
        .highlight_spacing(HighlightSpacing::Always)
}

pub(super) fn picker_state(cursor: usize) -> ListState {
    let mut state = ListState::default();
    state.select(Some(cursor));
    state
}

/// Columns the check column of a multi-select takes (glyph and gap).
pub(super) const CHECK_PREFIX_WIDTH: usize = 2;

/// Columns a picker row spends beside its label: the pointer column (glyph
/// and gap) and the scrollbar column.
const PICKER_ROW_CHROME: usize = 3;

/// The popup rect for the list pickers: as wide as the widest row
/// (`content_width` columns of label, plus the pointer and scrollbar columns)
/// but at least as wide as `hints`, one row per item plus the hint block.
pub(super) fn picker_area(
    area: Rect,
    item_count: usize,
    content_width: usize,
    hints: &[(&str, &str)],
) -> Rect {
    let content = content_width.saturating_add(PICKER_ROW_CHROME);
    let wanted = content.max(hints_width(hints));
    let wanted_width = u16::try_from(wanted)
        .unwrap_or(u16::MAX)
        .saturating_add(MODAL_CHROME_WIDTH);
    let width =
        fit(wanted_width, MIN_PICKER_WIDTH, area.width.saturating_sub(4));
    // The width first: on a narrow screen the hints wrap and need rows.
    let inner = usize::from(width.saturating_sub(MODAL_CHROME_WIDTH));
    let rows = u16::try_from(item_count).unwrap_or(u16::MAX);
    let wanted_height = rows
        .saturating_add(MODAL_CHROME_HEIGHT)
        .saturating_add(hint_block_height(hints, inner));
    let height = fit(
        wanted_height,
        MIN_PICKER_HEIGHT,
        area.height.saturating_sub(2),
    );
    centered_rect(width, height, area)
}

/// The widest of `labels` in display columns, for sizing a picker.
pub(super) fn labels_width<'a>(labels: impl Iterator<Item = &'a str>) -> usize {
    labels.map(UnicodeWidthStr::width).max().unwrap_or(0)
}

/// The columns `hints` take on one row, so a box never cuts them off. A host
/// box adds it to its minimum width (plus `chrome::MODAL_CHROME_WIDTH`).
pub fn hints_width(hints: &[(&str, &str)]) -> usize {
    shortcut_hints::lines(hints, Color::Default, usize::MAX)
        .first()
        .map_or(0, Line::width)
}

/// The blank spacer and the hint lines closing a modal body, wrapped at
/// `width` onto at most [`MAX_HINT_ROWS`] rows, so a narrow screen shows
/// every key instead of cutting the line. A modal's key prompt is essential,
/// so it always shows, independent of the global F1 toggle (which governs
/// only the main-app footer). A host modal ends its body with it, so its keys
/// look like the toolkit's, and sizes it with [`hint_block_height`].
pub fn hint_block(
    items: &[(&str, &str)],
    palette: &Palette,
    width: usize,
) -> Vec<Line<'static>> {
    let rows = shortcut_hints::lines(items, palette.accent, width);
    if rows.is_empty() {
        return Vec::new();
    }
    std::iter::once(Line::from(""))
        .chain(rows.into_iter().take(MAX_HINT_ROWS))
        .collect()
}

/// The rows a [`hint_block`] of `items` takes at `width` columns: the
/// spacer plus one row, or more while the hints wrap on a narrow screen.
pub fn hint_block_height(items: &[(&str, &str)], width: usize) -> u16 {
    let rows = shortcut_hints::lines(items, Color::Default, width)
        .len()
        .clamp(1, MAX_HINT_ROWS);
    1 + u16::try_from(rows).unwrap_or(1)
}

/// The height of a modal box whose body is one row plus the [`hint_block`]
/// of `hints` at `inner_width`: the frame (borders and top padding), the row
/// itself and the hint block.
pub(super) fn hinted_box_height(
    hints: &[(&str, &str)],
    inner_width: u16,
) -> u16 {
    MODAL_CHROME_HEIGHT + 1 + hint_block_height(hints, usize::from(inner_width))
}

/// The columns and rows `text` needs when every `\n` starts a new row: the
/// width of its longest line and the number of lines (at least one, so an
/// empty text keeps its row). A modal sizes its box from this instead of from
/// the text's total width, which would lay a multi-line body out as one line.
pub(super) fn text_extent(text: &str) -> (u16, u16) {
    let widest = text
        .split('\n')
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let rows = text.split('\n').count();
    (
        u16::try_from(widest).unwrap_or(u16::MAX),
        u16::try_from(rows).unwrap_or(u16::MAX),
    )
}
