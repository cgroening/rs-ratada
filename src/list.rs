//! A selectable, vertically scrollable list with a scrollbar.
//!
//! The `position/total` indicator never overlays a row. Where there is a frame
//! it lives in its bottom border: [`render_boxed`] hands it to its own box, and
//! a list inside a popup gets it from the popup's frame (see
//! [`crate::chrome::render_badge`]). Where there is none, [`render_counted`]
//! keeps the bottom row free for it. Plain [`render`] draws no indicator at all.
//!
//! The cursor row is always tinted. [`CursorMark::Pointer`] adds the pointer
//! glyph in a leading column (blank on every other row), the look of a list
//! box; [`render_marked`] and [`render_boxed_marked`] take the mark.

use std::cell::Cell;

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

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

/// How a list marks its cursor row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorMark {
    /// The selection tint alone (what [`render`] draws).
    #[default]
    Tint,
    /// The selection tint plus the pointer glyph in a leading column. The
    /// column is as wide as the glyph and one space, blank on every other
    /// row, so the labels stay aligned.
    Pointer,
}

impl CursorMark {
    /// The columns the mark takes in front of every row's content.
    pub fn width(self, skin: &Skin) -> usize {
        match self {
            Self::Tint => 0,
            Self::Pointer => skin.glyphs.pointer.width() + 1,
        }
    }
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
    render_core(frame, area, skin, view, CursorMark::Tint)
}

/// Like [`render`], but marks the cursor row with `mark`. A list inside a
/// frame it does not own (a picker popup) takes [`CursorMark::Pointer`].
pub fn render_marked(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    view: ListView,
    mark: CursorMark,
) -> usize {
    render_core(frame, area, skin, view, mark)
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
        return render_core(frame, area, skin, view, CursorMark::Tint);
    }
    let badge = chrome::position_badge(view.selected, view.rows.len());
    let rows = Rect {
        height: area.height - 1,
        ..area
    };
    let viewport = render_core(frame, rows, skin, view, CursorMark::Tint);
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
        render_core(frame, inner, skin, view, CursorMark::Tint)
    } else {
        render(frame, area, skin, view)
    }
}

/// The list box: `view` in a rounded box (see [`chrome::BoxDecor`]) with the
/// `position/total` badge in its bottom border and the cursor row marked by
/// `mark` - [`CursorMark::Pointer`] for the full list-box look. Returns the
/// inner viewport height (the visible row count).
pub fn render_boxed_marked(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    view: ListView,
    decor: &chrome::BoxDecor,
    mark: CursorMark,
) -> usize {
    let badge = chrome::position_badge(view.selected, view.rows.len());
    let inner = chrome::framed_decor(frame, area, skin, decor, &badge);
    render_core(frame, inner, skin, view, mark)
}

/// Draws the list rows (with the selection highlight) and the scrollbar,
/// returning the viewport height (the number of visible rows).
fn render_core(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    view: ListView,
    mark: CursorMark,
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
            let is_cursor = index == selected;
            let line = with_mark(line, mark, is_cursor, skin);
            if is_cursor {
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

/// Puts the `mark` column in front of `line`: the pointer glyph on the cursor
/// row, blanks of the same width elsewhere. [`CursorMark::Tint`] leaves the
/// line as it is.
fn with_mark(
    line: Line<'static>,
    mark: CursorMark,
    is_cursor: bool,
    skin: &Skin,
) -> Line<'static> {
    if mark == CursorMark::Tint {
        return line;
    }
    let lead = if is_cursor {
        Span::styled(
            format!("{} ", skin.glyphs.pointer),
            style::fg(skin.palette.accent),
        )
    } else {
        Span::raw(" ".repeat(mark.width(skin)))
    };
    let mut spans = Vec::with_capacity(line.spans.len() + 1);
    spans.push(lead);
    spans.extend(line.spans);
    Line {
        spans,
        style: line.style,
        alignment: line.alignment,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

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

    fn rows(count: usize) -> Vec<Line<'static>> {
        (0..count).map(|i| Line::from(format!("row {i}"))).collect()
    }

    /// Runs `body` against a real frame of the given size and returns whatever
    /// it reports, so the viewport arithmetic is checked on the running widget
    /// rather than re-derived in the test.
    fn viewport_of(
        width: u16,
        height: u16,
        body: impl FnOnce(&mut Frame, Rect, &Skin) -> usize,
    ) -> usize {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("a test terminal");
        let mut reported = 0;
        terminal
            .draw(|frame| {
                reported = body(frame, frame.area(), &skin());
            })
            .expect("the frame draws");
        reported
    }

    #[test]
    fn a_plain_list_uses_every_row_of_its_area() {
        let offset = Cell::new(0);
        let viewport = viewport_of(20, 6, |frame, area, skin| {
            render(
                frame,
                area,
                skin,
                ListView {
                    rows: rows(10),
                    selected: 0,
                    offset: &offset,
                },
            )
        });
        assert_eq!(viewport, 6);
    }

    /// The counted variant reserves the bottom row for the badge, so its
    /// viewport is one shorter than the area.
    #[test]
    fn a_counted_list_gives_up_one_row_to_the_badge() {
        let offset = Cell::new(0);
        let viewport = viewport_of(20, 6, |frame, area, skin| {
            render_counted(
                frame,
                area,
                skin,
                ListView {
                    rows: rows(10),
                    selected: 0,
                    offset: &offset,
                },
            )
        });
        assert_eq!(viewport, 5);
    }

    /// Content wins over the badge: with a single row there is nothing to
    /// spare, so the badge is dropped rather than the only visible entry.
    #[test]
    fn a_one_row_area_keeps_its_content_and_drops_the_badge() {
        let offset = Cell::new(0);
        let viewport = viewport_of(20, 1, |frame, area, skin| {
            render_counted(
                frame,
                area,
                skin,
                ListView {
                    rows: rows(10),
                    selected: 0,
                    offset: &offset,
                },
            )
        });
        assert_eq!(viewport, 1);
    }

    /// The offset is kept across frames, and must follow a selection that
    /// sits below the visible window.
    #[test]
    fn the_offset_follows_a_selection_past_the_viewport() {
        let offset = Cell::new(0);
        viewport_of(20, 4, |frame, area, skin| {
            render(
                frame,
                area,
                skin,
                ListView {
                    rows: rows(20),
                    selected: 12,
                    offset: &offset,
                },
            )
        });
        assert!(offset.get() > 0, "the list did not scroll to the selection");
        assert!(offset.get() <= 12, "scrolled past the selection");
    }

    #[test]
    fn an_empty_list_renders_without_panicking() {
        let offset = Cell::new(0);
        let viewport = viewport_of(20, 4, |frame, area, skin| {
            render_counted(
                frame,
                area,
                skin,
                ListView {
                    rows: Vec::new(),
                    selected: 0,
                    offset: &offset,
                },
            )
        });
        assert_eq!(viewport, 3);
    }

    /// Draws `rows` with `mark` and returns the text of every buffer row.
    fn drawn_rows(selected: usize, mark: CursorMark) -> Vec<String> {
        let offset = Cell::new(0);
        let backend = TestBackend::new(12, 3);
        let mut terminal = Terminal::new(backend).expect("a test terminal");
        terminal
            .draw(|frame| {
                render_marked(
                    frame,
                    frame.area(),
                    &skin(),
                    ListView {
                        rows: rows(3),
                        selected,
                        offset: &offset,
                    },
                    mark,
                );
            })
            .expect("the frame draws");
        let buffer = terminal.backend().buffer();
        (0..3)
            .map(|y| {
                (0..12)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    /// The pointer marks only the cursor row; the others get blanks of the
    /// same width, so every label starts in the same column.
    #[test]
    fn the_pointer_marks_only_the_cursor_row() {
        let drawn = drawn_rows(1, CursorMark::Pointer);
        assert!(drawn[0].starts_with("  row 0"), "{drawn:?}");
        assert!(drawn[1].starts_with("\u{203a} row 1"), "{drawn:?}");
        assert!(drawn[2].starts_with("  row 2"), "{drawn:?}");
    }

    #[test]
    fn the_tint_mark_draws_no_lead_column() {
        let drawn = drawn_rows(1, CursorMark::Tint);
        assert!(drawn[1].starts_with("row 1"), "{drawn:?}");
    }

    #[test]
    fn the_mark_width_counts_the_glyph_and_one_space() {
        assert_eq!(CursorMark::Pointer.width(&skin()), 2);
        assert_eq!(CursorMark::Tint.width(&skin()), 0);
    }
}
