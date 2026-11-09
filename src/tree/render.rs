//! Drawing a [`TreeView`]: one row per visible node - indentation, fold
//! glyph and label, then an optional dimmed suffix and the node's cells in
//! right-aligned columns, lined up across the visible rows under an optional
//! underlined header row.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::{Flat, TreeItem, TreeView};
use crate::{chrome, list, style, text, theme::Skin};

/// The columns between the label column and the first cell, between two
/// cells, and between a label and its suffix.
const COLUMN_GAP: usize = 2;

/// The indentation per tree level.
const INDENT: &str = "  ";

impl TreeView {
    /// Renders the visible nodes with indentation and expand markers, the
    /// cursor row highlighted and a scrollbar on overflow. With column
    /// headers set, their row takes the top line of `area`.
    pub fn render(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let (header_area, rows_area) = self.split_header(area);
        let flat = self.flatten();
        let columns =
            Columns::measure(self, &flat, skin, self.content_width(rows_area));
        if let Some(header_area) = header_area {
            let header = columns.header(self, skin);
            frame.render_widget(Paragraph::new(header), header_area);
        }
        let lines: Vec<Line<'static>> =
            flat.iter().map(|node| columns.row(node, skin)).collect();
        let view = list::ListView {
            rows: lines,
            selected: self.cursor,
            offset: &self.offset,
        };
        let viewport = match &self.decor {
            Some(decor) => {
                list::render_boxed(frame, rows_area, skin, view, decor, true)
            }
            // Without a box there is no border to hang the badge on, so the
            // bottom row carries it.
            None => list::render_counted(frame, rows_area, skin, view),
        };
        self.viewport.set(viewport);
    }

    /// `area` split into the header row (when there are headers and room
    /// for a row below them) and the area for the rows. The header row is
    /// moved in to where the rows' text starts inside a box.
    fn split_header(&self, area: Rect) -> (Option<Rect>, Rect) {
        if self.headers.is_empty() || area.height < 2 {
            return (None, area);
        }
        let inset = if self.decor.is_some() {
            1 + chrome::BOX_PADDING_X
        } else {
            0
        };
        let header = Rect {
            x: area.x + inset,
            width: area.width.saturating_sub(2 * inset),
            height: 1,
            ..area
        };
        let rows = Rect {
            y: area.y + 1,
            height: area.height - 1,
            ..area
        };
        (Some(header), rows)
    }

    /// The columns a row's text may take inside `area`. With columns, the
    /// right-most one stays free for the scrollbar, whether or not one is
    /// drawn, so the cells never move.
    fn content_width(&self, area: Rect) -> usize {
        let inner = if self.decor.is_some() {
            area.width.saturating_sub(chrome::BOX_CHROME_WIDTH)
        } else {
            area.width
        };
        let scrollbar = u16::from(self.has_columns());
        usize::from(inner.saturating_sub(scrollbar))
    }

    /// Whether any row carries cells or the view has headers.
    fn has_columns(&self) -> bool {
        !self.headers.is_empty() || self.roots.iter().any(has_cells)
    }
}

/// Whether `item` or any node below it carries cells.
fn has_cells(item: &TreeItem) -> bool {
    !item.columns.is_empty() || item.children.iter().any(has_cells)
}

/// The column geometry of a tree, measured over its visible rows, so the
/// table is no wider than what is on screen needs.
struct Columns {
    /// The width of each cell column.
    widths: Vec<usize>,
    /// The width of the label column on a row with cells: the widest visible
    /// label with its indentation and suffix among those rows, or less when
    /// the row is too narrow.
    label_width: usize,
    /// The width a row's text may take in all.
    content_width: usize,
}

impl Columns {
    /// Measures the columns of `view`'s visible rows `flat` for rows
    /// `content_width` columns wide. The headings count like a top-level
    /// row, so the header always fits.
    fn measure(
        view: &TreeView,
        flat: &[Flat],
        skin: &Skin,
        content_width: usize,
    ) -> Self {
        let mut widths: Vec<usize> =
            view.headers.iter().map(|header| header.width()).collect();
        let mut widest_label =
            view.label_heading.as_ref().map_or(0, |heading| {
                lead(0, false, false, skin).width() + heading.width()
            });
        for node in flat {
            // A row without cells has the whole width to itself, so it does
            // not push the columns out.
            if node.columns.is_empty() {
                continue;
            }
            let lead = lead(node.depth, node.has_children, node.expanded, skin);
            let suffix = node
                .suffix
                .as_ref()
                .map_or(0, |suffix| COLUMN_GAP + suffix.width());
            widest_label =
                widest_label.max(lead.width() + node.label.width() + suffix);
            for (index, cell) in node.columns.iter().enumerate() {
                if index == widths.len() {
                    widths.push(0);
                }
                widths[index] = widths[index].max(cell.text.width());
            }
        }
        let cells_width = cells_width(&widths);
        let room = content_width.saturating_sub(cells_width + COLUMN_GAP);
        Self {
            label_width: widest_label.min(room),
            widths,
            content_width,
        }
    }

    /// The header row: the label heading where the top-level labels start,
    /// each column heading right-aligned over its column; dimmed and
    /// underlined across the table.
    fn header(&self, view: &TreeView, skin: &Skin) -> Line<'static> {
        let lead = lead(0, false, false, skin).width();
        let heading = view.label_heading.as_deref().unwrap_or("");
        let heading =
            text::truncate(heading, self.label_width.saturating_sub(lead));
        let pad = self.label_width.saturating_sub(lead + heading.width());
        let mut spans = vec![
            Span::raw(" ".repeat(lead)),
            Span::raw(heading),
            Span::raw(" ".repeat(pad)),
        ];
        for (index, width) in self.widths.iter().enumerate() {
            let heading = view.headers.get(index).map_or("", String::as_str);
            spans.push(Span::raw(" ".repeat(COLUMN_GAP)));
            push_right_aligned(&mut spans, heading, *width, Style::default());
        }
        patch_all(&mut spans, style::dim().add_modifier(Modifier::UNDERLINED));
        Line::from(spans)
    }

    /// The line of one visible `node`.
    fn row(&self, node: &Flat, skin: &Skin) -> Line<'static> {
        let has_cells = !node.columns.is_empty();
        let room = if has_cells {
            self.label_width
        } else {
            self.content_width
        };
        let lead = lead(node.depth, node.has_children, node.expanded, skin);
        let label =
            text::truncate(&node.label, room.saturating_sub(lead.width()));
        let mut used = lead.width() + label.width();
        let mut spans =
            vec![Span::raw(lead), Span::styled(label, node.label_style)];
        if let Some(suffix) = &node.suffix {
            let free = room.saturating_sub(used + COLUMN_GAP);
            let suffix = text::truncate(suffix, free);
            if !suffix.is_empty() {
                used += COLUMN_GAP + suffix.width();
                spans.push(Span::raw(" ".repeat(COLUMN_GAP)));
                spans.push(Span::styled(suffix, style::dim()));
            }
        }
        if has_cells {
            spans.push(Span::raw(" ".repeat(room.saturating_sub(used))));
            for (index, width) in self.widths.iter().enumerate() {
                spans.push(Span::raw(" ".repeat(COLUMN_GAP)));
                let (cell, cell_style) = node
                    .columns
                    .get(index)
                    .map_or(("", Style::default()), |cell| {
                        (cell.text.as_str(), cell.style)
                    });
                push_right_aligned(&mut spans, cell, *width, cell_style);
            }
        }
        // Row styles ride on the spans, so the cursor tint still lays over
        // the row as a whole.
        patch_all(&mut spans, node.row_style);
        if node.is_dimmed {
            patch_all(&mut spans, style::dim());
        }
        Line::from(spans)
    }
}

/// Patches `style` over every span of a row.
fn patch_all(spans: &mut [Span<'static>], style: Style) {
    for span in spans {
        span.style = span.style.patch(style);
    }
}

/// The text in front of a label: the indentation of `depth`, the fold glyph
/// of a node with children (blank for a leaf) and a space.
fn lead(
    depth: usize,
    has_children: bool,
    expanded: bool,
    skin: &Skin,
) -> String {
    let marker = if has_children {
        skin.glyphs.fold(expanded)
    } else {
        " "
    };
    format!("{}{marker} ", INDENT.repeat(depth))
}

/// The width the cell columns take with the gaps between them.
fn cells_width(widths: &[usize]) -> usize {
    if widths.is_empty() {
        return 0;
    }
    widths.iter().sum::<usize>() + COLUMN_GAP * (widths.len() - 1)
}

/// Pushes `text` in `style`, right-aligned in `width` columns: the padding
/// in front stays unstyled, so a styled cell only colors its own text.
fn push_right_aligned(
    spans: &mut Vec<Span<'static>>,
    text: &str,
    width: usize,
    style: Style,
) {
    spans.push(Span::raw(" ".repeat(width.saturating_sub(text.width()))));
    spans.push(Span::styled(text.to_string(), style));
}
