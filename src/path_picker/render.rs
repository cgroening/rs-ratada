//! Rendering the picker body: the current directory, the filter line and the
//! scrollable entry list.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::State;
use crate::{list, shortcut_hints, style, text::truncate, theme::Skin};

/// The prefix of the filter line; its width is taken off the caret line's.
const FILTER_LABEL: &str = "filter ";

pub(super) fn render_body(
    frame: &mut Frame,
    inner: Rect,
    skin: &Skin,
    state: &State,
) {
    let palette = &skin.palette;
    let inner_width = inner.width as usize;

    // Header (current dir), filter line, the scrollable entry list, footer. The
    // footer always reserves its row (popup hints ignore the F1 toggle).
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(shortcut_hints::footer_height(1)),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate(&state.dir.display().to_string(), inner_width),
            style::secondary(palette),
        ))),
        rows[0],
    );
    // The filter carries a real caret: `Home`/`End`/`Ctrl+A` reach the field
    // even though `Left`/`Right` are taken by browsing.
    let mut filter =
        vec![Span::styled(FILTER_LABEL, style::secondary(palette))];
    filter.extend(
        state.filter.caret_spans(
            palette,
            inner_width.saturating_sub(FILTER_LABEL.len()),
        ),
    );
    frame.render_widget(Paragraph::new(Line::from(filter)), rows[1]);

    // The list widget owns the cursor highlight, scroll-to-cursor and the
    // scrollbar on overflow; directories keep their accent color when not
    // under the cursor.
    let entries: Vec<Line<'static>> = state
        .visible
        .iter()
        .map(|&index| {
            let entry = &state.entries[index];
            let marker = if entry.is_dir { "/" } else { " " };
            let line = Line::from(truncate(
                &format!("{marker} {}", entry.name),
                inner_width,
            ));
            if entry.is_dir {
                line.style(style::fg(palette.accent))
            } else {
                line
            }
        })
        .collect();
    let viewport = list::render(
        frame,
        rows[2],
        skin,
        list::ListView {
            rows: entries,
            selected: state.cursor,
            offset: &state.offset,
        },
    );
    state.viewport.set(viewport);

    frame.render_widget(
        Paragraph::new(
            shortcut_hints::lines(
                &[
                    ("\u{2190}\u{2192}", "browse"),
                    ("enter", "pick"),
                    ("ctrl+h", "hidden"),
                ],
                palette.accent_dim,
                inner_width,
            )
            .into_iter()
            .next()
            .unwrap_or_default(),
        ),
        rows[3],
    );
}
