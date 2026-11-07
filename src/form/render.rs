//! Rendering a [`Form`]: the shared modal frame and one row per field.
//!
//! The form looks like every other modal (frame, fill and padding from
//! [`overlay::framed`], the modal hint block); the focused row is tinted from
//! border to border with its label in the accent, a date field carries the
//! pointer glyph, and a reported problem shows in red under its field.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::{Field, FieldState, Form, LABEL_WIDTH};
use crate::{
    chrome::{self, MODAL_CHROME_HEIGHT, MODAL_CHROME_WIDTH, MODAL_PADDING_X},
    layout::centered_rect,
    modal::render::{HINT_BLOCK_ROWS, hint_block},
    overlay, shortcut_hints, style,
    text::truncate,
    theme::Skin,
};

/// The narrowest form box: wide enough for its hint line inside the frame.
const MIN_WIDTH: u16 = 38 + MODAL_CHROME_WIDTH;

/// The form's keys, shown in the modal hint block.
const HINTS: [(&str, &str); 3] =
    [("tab", "next"), ("ctrl+s", "save"), ("esc", "cancel")];

impl Form {
    pub(super) fn render(&self, frame: &mut Frame, skin: &Skin) {
        let outer = frame.area();
        let body: u16 =
            (0..self.fields.len()).map(|i| self.row_height(i)).sum();
        let footer = shortcut_hints::footer_height(HINT_BLOCK_ROWS);
        let width = (outer.width * 2 / 3).clamp(MIN_WIDTH, outer.width);
        let height = (body + MODAL_CHROME_HEIGHT + footer).min(outer.height);
        let area = centered_rect(width, height, outer);
        frame.render_widget(ratatui::widgets::Clear, area);
        let inner = overlay::framed(frame, area, skin, &self.title);
        let badge = chrome::position_badge(self.focus, self.fields.len());
        chrome::render_badge(frame, area, skin, &badge);

        let mut constraints: Vec<Constraint> = (0..self.fields.len())
            .map(|i| Constraint::Length(self.row_height(i)))
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
            let hints =
                hint_block(&HINTS, &skin.palette, footer_rect.width as usize);
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
        let error = self.error_for(index);
        let (rect, error_rect) = split_error_row(rect, error.is_some());
        if focused {
            // The tint reaches into the side padding, border to border.
            frame.render_widget(
                Block::default().style(style::bg(palette.selection)),
                widen_into_padding(rect, frame.area()),
            );
        }
        if let (Some(message), Some(error_rect)) = (error, error_rect) {
            render_error(frame, error_rect, skin, message);
        }

        let marker = if field.is_dirty() { "*" } else { " " };
        let label = format!(
            "{marker} {:<width$}",
            field.label,
            width = LABEL_WIDTH as usize - 2
        );
        let label_style = if error.is_some() {
            style::error(palette).add_modifier(Modifier::BOLD)
        } else if focused {
            style::accent(palette).add_modifier(Modifier::BOLD)
        } else {
            style::secondary(palette)
        };

        if let FieldState::Multiline { area, .. } = &field.state {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(rect);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(label, label_style))),
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
            Paragraph::new(Line::from(Span::styled(label, label_style))),
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
            FieldState::Date { value, .. } => {
                date_line(skin, *value, value_width)
            }
            FieldState::Multiline { .. } => Line::from(""),
        };
        frame.render_widget(Paragraph::new(value), columns[1]);
    }
}

/// A date value (or `—` when cleared) with the pointer glyph flush right,
/// marking that `Enter` opens the calendar.
fn date_line(
    skin: &Skin,
    value: Option<chrono::NaiveDate>,
    width: usize,
) -> Line<'static> {
    let pointer = skin.glyphs.pointer;
    let text =
        value.map_or_else(|| "\u{2014}".to_string(), |date| date.to_string());
    let room = width.saturating_sub(pointer.width() + 1);
    let text = truncate(&text, room);
    let fill = width.saturating_sub(text.width() + pointer.width());
    Line::from(vec![
        Span::raw(text),
        Span::raw(" ".repeat(fill)),
        Span::styled(pointer, style::secondary(&skin.palette)),
    ])
}

/// Splits a field's rect into its own rows and, when it has an error, the
/// error row below them.
fn split_error_row(rect: Rect, has_error: bool) -> (Rect, Option<Rect>) {
    if !has_error || rect.height < 2 {
        return (rect, None);
    }
    let field = Rect {
        height: rect.height - 1,
        ..rect
    };
    let error = Rect {
        y: rect.y + rect.height - 1,
        height: 1,
        ..rect
    };
    (field, Some(error))
}

/// Draws a field's problem in red, aligned under the value column.
fn render_error(frame: &mut Frame, rect: Rect, skin: &Skin, message: &str) {
    let indent = LABEL_WIDTH.min(rect.width);
    let rect = Rect {
        x: rect.x + indent,
        width: rect.width - indent,
        ..rect
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            truncate(message, rect.width as usize),
            style::error(&skin.palette),
        )),
        rect,
    );
}

/// `rect` widened by the modal side padding, so a row tint reaches the border;
/// clamped to `bounds`.
fn widen_into_padding(rect: Rect, bounds: Rect) -> Rect {
    let x = rect.x.saturating_sub(MODAL_PADDING_X).max(bounds.x);
    let right = (rect.x + rect.width + MODAL_PADDING_X).min(bounds.right());
    Rect {
        x,
        width: right.saturating_sub(x),
        ..rect
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
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

    fn form() -> Form {
        Form::new(
            "Task",
            vec![
                Field::text("Title", "Write docs"),
                Field::date("Due", NaiveDate::from_ymd_opt(2026, 11, 7)),
            ],
        )
    }

    /// Renders `form` into a 60x14 screen and returns its rows as text.
    fn rows(form: &Form) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(60, 14)).unwrap();
        terminal.draw(|frame| form.render(frame, &skin())).unwrap();
        let buffer = terminal.backend().buffer();
        (0..14)
            .map(|y| (0..60).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    #[test]
    fn a_date_field_carries_the_pointer_glyph() {
        let rows = rows(&form());
        let due = rows
            .iter()
            .find(|row| row.contains("2026-11-07"))
            .expect("the date row is drawn");
        assert!(
            due.trim_end()
                .trim_end_matches('│')
                .trim_end()
                .ends_with('›')
        );
    }

    #[test]
    fn a_reported_error_shows_under_its_field_and_takes_the_focus() {
        let mut form = form();
        form.set_error(1, "Too late.");
        assert_eq!(form.focus, 1);
        let rows = rows(&form);
        let due = rows.iter().position(|row| row.contains("2026-11-07"));
        let error = rows.iter().position(|row| row.contains("Too late."));
        assert_eq!(error, due.map(|row| row + 1));
    }

    #[test]
    fn the_hint_line_keeps_every_key_at_the_narrowest_width() {
        let rows = rows(&form());
        let hints = rows
            .iter()
            .find(|row| row.contains("tab next"))
            .expect("the hints are drawn");
        assert!(hints.contains("esc cancel"), "{hints:?}");
    }

    #[test]
    fn an_error_past_the_last_field_is_ignored() {
        let mut form = form();
        form.set_error(9, "nope");
        assert!(form.error.is_none());
        assert_eq!(form.focus, 0);
    }

    #[test]
    fn the_fields_start_inside_the_modal_padding() {
        let rows = rows(&form());
        let title = rows
            .iter()
            .find(|row| row.contains("Title"))
            .expect("the first field is drawn");
        // Border, two padding columns, the dirty-marker column and its gap.
        assert!(title.contains("│    Title"), "{title:?}");
    }
}
