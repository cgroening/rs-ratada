//! Rendering the sidebar: its frame, the filter line and the scrollable list
//! of section headers and items.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{Overflow, Row, Sidebar, split_for_hbar};
use crate::{chrome, input, nav, scroll, style, text, theme::Skin};

impl Sidebar {
    /// Renders the panel/box, the filter line (when open) and the list.
    pub fn render(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let inner = self.render_frame(frame, area, skin);
        let list_area = self.render_filter_line(frame, inner, skin);
        self.render_list(frame, list_area, skin);
    }

    /// Draws the surrounding chrome (filled panel by default, or a box) and
    /// returns the inner content area.
    fn render_frame(&self, frame: &mut Frame, area: Rect, skin: &Skin) -> Rect {
        if let Some(decor) = &self.decor {
            let badge = self.filtered_items().count().to_string();
            return chrome::framed_decor(frame, area, skin, decor, &badge);
        }
        let block = chrome::menu_panel(skin);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    }

    /// When filtering, draws the `/query` line on the top row and returns the
    /// remaining list area; otherwise returns `inner` unchanged.
    fn render_filter_line(
        &self,
        frame: &mut Frame,
        inner: Rect,
        skin: &Skin,
    ) -> Rect {
        if !self.filtering {
            return inner;
        }
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(inner);
        let palette = &skin.palette;
        let mut query = vec![Span::styled("/", style::fg(palette.accent))];
        query.extend(input::query_spans(
            &self.filter,
            palette,
            (split[0].width as usize).saturating_sub(1),
        ));
        frame.render_widget(Paragraph::new(Line::from(query)), split[0]);
        split[1]
    }

    /// Draws the rows, the vertical scrollbar and (in scroll mode, on overflow)
    /// the horizontal scrollbar, keeping the cursor row visible.
    fn render_list(&self, frame: &mut Frame, area: Rect, skin: &Skin) {
        let rows = self.rows();
        let max_width = rows.iter().map(Self::row_width).max().unwrap_or(0);
        // A vertical scrollbar claims the rightmost column when the rows
        // overflow the height, so reserve it: labels then clip before the bar
        // instead of underneath it. An hbar only shrinks the height further, so
        // measuring against the full area height keeps this decision simple.
        let has_scrollbar = rows.len() > area.height as usize;
        let content_width =
            (area.width as usize).saturating_sub(usize::from(has_scrollbar));

        let overflowing =
            self.overflow == Overflow::Scroll && max_width > content_width;
        let (body, hbar) = split_for_hbar(area, overflowing);

        let height = body.height as usize;
        self.viewport.set(height.max(1));
        let selected_row = self.selected_row(&rows);
        let offset = self.scroll_offset(&rows, selected_row, height);
        self.offset.set(offset);
        let h_offset = self.clamp_h_offset(max_width, content_width);

        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(height)
            .map(|(index, row)| {
                self.render_row(
                    row,
                    index == selected_row,
                    content_width,
                    h_offset,
                    skin,
                )
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), body);

        scroll::render_scrollbar(
            frame,
            body,
            skin,
            nav::ScrollView {
                total: rows.len(),
                offset,
                viewport: height,
            },
        );
        if let Some(hbar) = hbar {
            scroll::render_hscrollbar(
                frame,
                hbar,
                skin,
                nav::ScrollView {
                    total: max_width,
                    offset: h_offset,
                    viewport: content_width,
                },
            );
        }
    }

    /// Builds one styled line: a dim header, or an item with the selection
    /// pointer and (when selected) an accent, full-width tinted bar.
    fn render_row(
        &self,
        row: &Row,
        selected: bool,
        width: usize,
        h_offset: usize,
        skin: &Skin,
    ) -> Line<'static> {
        let palette = &skin.palette;
        match row {
            Row::Header(title) => Line::from(Span::styled(
                self.clip(title, h_offset, width),
                style::dim().add_modifier(Modifier::BOLD),
            )),
            Row::Item(item) => {
                let pointer = if selected { skin.glyphs.pointer } else { " " };
                let full = format!("{pointer} {}", item.label);
                let clipped = self.clip(&full, h_offset, width);
                if selected {
                    let bar = text::pad_end(&clipped, width);
                    let style = style::fg(palette.accent)
                        .add_modifier(Modifier::BOLD)
                        .bg(style::to_ratatui(palette.selection));
                    Line::from(Span::styled(bar, style))
                } else {
                    Line::from(Span::styled(clipped, Style::default()))
                }
            }
        }
    }

    /// Clips `text` to `width` columns per the overflow mode.
    fn clip(&self, text: &str, h_offset: usize, width: usize) -> String {
        match self.overflow {
            Overflow::Truncate => text::truncate(text, width),
            Overflow::Scroll => text::window(text, h_offset, width),
        }
    }
}
