//! Rendering the picker body: the current directory, the filter line and the
//! scrollable entry list.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::State;
use crate::{
    list, modal::MAX_HINT_ROWS, shortcut_hints, style, text::truncate,
    theme::Skin,
};

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

    // Header (current dir), filter line, the scrollable entry list, footer.
    // The footer always shows (popup hints ignore the F1 toggle) and wraps
    // on a narrow screen instead of cutting keys.
    let hints = hint_lines(skin, inner_width);
    let hint_rows = u16::try_from(hints.len().max(1)).unwrap_or(1);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(hint_rows),
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

    frame.render_widget(Paragraph::new(hints), rows[3]);
}

/// The picker's keys wrapped at `width`, at most [`MAX_HINT_ROWS`] rows.
fn hint_lines(skin: &Skin, width: usize) -> Vec<Line<'static>> {
    shortcut_hints::lines(&HINTS, skin.palette.accent_dim, width)
        .into_iter()
        .take(MAX_HINT_ROWS)
        .collect()
}

/// The keys of the path picker.
const HINTS: [(&str, &str); 3] = [
    ("\u{2190}\u{2192}", "browse"),
    ("Enter", "pick"),
    ("Ctrl+H", "hidden"),
];

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
        assert_eq!(hint_lines(&skin(), 80).len(), 1);
        let narrow = hint_lines(&skin(), 20);
        assert!(narrow.len() > 1);
        let shown = text(&narrow);
        for (key, _) in HINTS {
            assert!(shown.contains(key), "{key} missing in\n{shown}");
        }
    }
}
