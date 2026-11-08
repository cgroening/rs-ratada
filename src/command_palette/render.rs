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
    fuzzy, input, list,
    modal::MAX_HINT_ROWS,
    shortcut_hints, style,
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

    // The popup footer always shows (popup hints ignore the global F1
    // toggle, which governs only the main-app footer) and wraps on a narrow
    // screen instead of cutting keys.
    let hints = footer_hints(skin, usize::from(inner.width), grouped);
    let hint_rows = u16::try_from(hints.len().max(1)).unwrap_or(1);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(hint_rows),
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

    frame.render_widget(Paragraph::new(hints), rows[2]);
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

/// The footer hint lines, wrapped at `width` onto at most
/// [`MAX_HINT_ROWS`] rows: adds `tab section` only while grouped.
fn footer_hints(
    skin: &Skin,
    width: usize,
    grouped: bool,
) -> Vec<Line<'static>> {
    let mut hints: Vec<(&str, &str)> =
        vec![("\u{2191}\u{2193}", "move"), ("Enter", "run")];
    if grouped {
        hints.push(("Tab", "section"));
    }
    hints.push(("Esc", "close"));
    shortcut_hints::lines(&hints, skin.palette.accent_dim, width)
        .into_iter()
        .take(MAX_HINT_ROWS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{
        ColorOverrides, GlyphVariant, Glyphs, Palette, ThemeRegistry,
    };

    fn skin() -> Skin {
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
        )
    }

    fn text(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_keys_wrap_on_a_narrow_screen_and_stay_complete() {
        assert_eq!(footer_hints(&skin(), 80, true).len(), 1);
        let narrow = footer_hints(&skin(), 24, true);
        assert!(narrow.len() > 1);
        let shown = text(&narrow);
        for key in ["move", "Enter", "Tab", "Esc"] {
            assert!(shown.contains(key), "{key} missing in\n{shown}");
        }
    }
}
