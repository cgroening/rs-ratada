//! Rendering a [`Form`]: the framed body and one row per field.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use super::{Field, FieldState, Form, LABEL_WIDTH};
use crate::{
    chrome, layout::centered_rect, shortcut_hints, style, theme::Skin,
};

impl Form {
    pub(super) fn render(&self, frame: &mut Frame, skin: &Skin) {
        let palette = &skin.palette;
        let outer = frame.area();
        let body: u16 = self.fields.iter().map(Field::height).sum();
        let footer = shortcut_hints::footer_height(1);
        let width = (outer.width * 2 / 3).clamp(40, outer.width);
        // Two border rows plus the always-shown popup footer.
        let height = (body + 2 + footer).min(outer.height);
        let area = centered_rect(width, height, outer);
        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style::border(palette))
            .style(style::bg(palette.background))
            .title(chrome::border_title(
                skin,
                &self.title,
                style::fg(palette.accent)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let badge = chrome::position_badge(self.focus, self.fields.len());
        chrome::render_badge(frame, area, skin, &badge);

        let mut constraints: Vec<Constraint> = self
            .fields
            .iter()
            .map(|f| Constraint::Length(f.height()))
            .collect();
        constraints.push(Constraint::Length(footer));
        let rects = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        for (index, field) in self.fields.iter().enumerate() {
            self.render_field(frame, rects[index], skin, index, field);
        }
        if let Some(footer_rect) = rects.last() {
            let hints = shortcut_hints::lines(
                &[("tab", "next"), ("ctrl+s", "save"), ("esc", "cancel")],
                palette.accent_dim,
                footer_rect.width as usize,
            );
            frame.render_widget(Paragraph::new(hints), *footer_rect);
        }
    }

    fn render_field(
        &self,
        frame: &mut Frame,
        rect: Rect,
        skin: &Skin,
        index: usize,
        field: &Field,
    ) {
        let palette = &skin.palette;
        let focused = index == self.focus;
        if focused {
            frame.render_widget(
                Block::default().style(style::bg(palette.selection)),
                rect,
            );
        }
        let marker = if field.is_dirty() { "*" } else { " " };
        let label = format!(
            "{marker} {:<width$}",
            field.label,
            width = LABEL_WIDTH as usize - 2
        );

        if let FieldState::Multiline { area, .. } = &field.state {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(rect);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    label,
                    style::secondary(palette),
                ))),
                rows[0],
            );
            area.render(frame, rows[1], skin, focused);
            return;
        }

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(LABEL_WIDTH), Constraint::Min(1)])
            .split(rect);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                label,
                style::secondary(palette),
            ))),
            columns[0],
        );
        let value_width = columns[1].width as usize;
        let value: Line = match &field.state {
            FieldState::Text { input, .. } => {
                input.render_line(palette, value_width, focused)
            }
            FieldState::Bool { value, .. } => {
                Line::from(if *value { "[x]" } else { "[ ]" })
            }
            FieldState::Choice { index, options, .. } => {
                Line::from(format!("\u{2039} {} \u{203a}", options[*index]))
            }
            FieldState::Date { value, .. } => Line::from(
                value.map_or_else(|| "\u{2014}".to_string(), |d| d.to_string()),
            ),
            FieldState::Multiline { .. } => Line::from(""),
        };
        frame.render_widget(Paragraph::new(value), columns[1]);
    }
}
