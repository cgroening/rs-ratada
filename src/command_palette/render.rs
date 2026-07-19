//! Rendering the palette: the search line, the row list and the footer hint.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    CommandItem, Row,
    layout::{CATEGORY_WIDTH, layout_rows},
};
use crate::{
    filter_list::FilterList,
    fuzzy, input, list, shortcut_hints, style,
    theme::{Palette, Skin},
};

/// The prefix of the query line; its width is taken off the caret line's.
const SEARCH_LABEL: &str = "search ";

/// The width of the command-label column before the key hint.
const LABEL_WIDTH: usize = 16;

pub(super) fn render_body(
    frame: &mut Frame,
    inner: Rect,
    skin: &Skin,
    items: &[CommandItem<'_>],
    state: &FilterList,
) {
    let palette = &skin.palette;
    let grouped = state.query.trim().is_empty();
    let layout = layout_rows(items, &state.query);

    // The popup footer always reserves its row (popup hints ignore the global
    // F1 toggle, which governs only the main-app footer).
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(shortcut_hints::footer_height(1)),
        ])
        .split(inner);

    let mut search =
        vec![Span::styled(SEARCH_LABEL, style::secondary(palette))];
    search.extend(input::query_spans(
        &state.query,
        palette,
        (rows[0].width as usize).saturating_sub(SEARCH_LABEL.len()),
    ));
    frame.render_widget(Paragraph::new(Line::from(search)), rows[0]);

    let header_style =
        style::fg(palette.accent_dim).add_modifier(Modifier::BOLD);
    let entries: Vec<Line<'static>> = layout
        .rows
        .iter()
        .map(|row| match row {
            Row::Header(title) => {
                Line::from(Span::styled(title.to_uppercase(), header_style))
            }
            Row::Item { item, .. } => {
                item_line(item, &state.query, palette, grouped)
            }
        })
        .collect();

    let selected = layout
        .selectable
        .get(state.cursor.min(layout.selectable.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0);
    let viewport = list::render(
        frame,
        rows[1],
        skin,
        list::ListView {
            rows: entries,
            selected,
            offset: &state.offset,
        },
    );
    state.viewport.set(viewport);

    let hint = footer_hint(skin, rows[2].width as usize, grouped);
    frame.render_widget(Paragraph::new(hint), rows[2]);
}

/// Renders one command row. Enabled commands show the label (with fuzzy match
/// highlights) and an accented key hint; disabled ones are wholly dimmed.
fn item_line(
    item: &CommandItem<'_>,
    query: &str,
    palette: &Palette,
    grouped: bool,
) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    if !grouped {
        spans.push(Span::styled(
            format!("{:<CATEGORY_WIDTH$}", item.category),
            style::secondary(palette),
        ));
    }
    let label = format!("{:<LABEL_WIDTH$}", item.label);
    if item.enabled {
        spans.extend(fuzzy::highlight(
            &label,
            query,
            style::secondary(palette),
            palette,
        ));
        spans.push(Span::styled(
            item.key_hint.to_string(),
            style::fg(palette.accent).add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(label, style::secondary(palette)));
        spans.push(Span::styled(
            item.key_hint.to_string(),
            style::secondary(palette),
        ));
    }
    Line::from(spans)
}

/// The footer hint line: adds `tab section` only while grouped.
fn footer_hint(skin: &Skin, width: usize, grouped: bool) -> Line<'static> {
    let mut hints: Vec<(&str, &str)> =
        vec![("\u{2191}\u{2193}", "move"), ("enter", "run")];
    if grouped {
        hints.push(("tab", "section"));
    }
    hints.push(("esc", "close"));
    shortcut_hints::lines(&hints, skin.palette.accent_dim, width)
        .into_iter()
        .next()
        .unwrap_or_default()
}
