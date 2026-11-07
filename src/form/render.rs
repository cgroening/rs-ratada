//! Rendering a [`Form`]: the shared modal frame and one row per field.
//!
//! The form looks like every other modal (frame, fill and padding from
//! [`overlay::framed`], the modal hint block). Its rows come from the
//! [`field`](super::field) building blocks: the label column as wide as the
//! longest label, the focused row tinted from border to border with its label
//! in the accent, a picker field with the pointer glyph and a problem in red
//! under its field. The hint block has fixed rows - the keys of the focused
//! field above the keys of the form - so the box keeps its height while the
//! focus moves.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::{
    Field, FieldState, Form, Mode,
    field::{self, FieldStyles, LabelMarks},
};
use crate::{
    chrome::{self, MODAL_CHROME_HEIGHT, MODAL_CHROME_WIDTH, MODAL_PADDING_X},
    layout::centered_rect,
    modal::{hint_block, hints_width},
    overlay, shortcut_hints, style,
    theme::Skin,
};

/// The fewest columns a value gets, however short the opening values are.
const MIN_VALUE_WIDTH: usize = 20;

/// Rows of the hint block: a blank spacer, the field's keys, the form's keys.
const HINT_ROWS: u16 = 3;

/// The form's own keys in normal mode.
const NORMAL_HINTS: [(&str, &str); 4] = [
    ("tab", "next"),
    ("s", "save"),
    ("y", "copy"),
    ("esc", "leave (asks to save)"),
];

/// The form's own keys while a field is edited.
const EDITING_HINTS: [(&str, &str); 2] = [("tab", "next"), ("ctrl+s", "save")];

/// The keys of the focused field, by kind and mode.
const TEXT_HINTS: [(&str, &str); 2] = [("enter", "edit"), ("ctrl+o", "reset")];
const TEXT_EDITING_HINTS: [(&str, &str); 2] =
    [("enter", "done"), ("ctrl+o", "reset")];
const MULTILINE_HINTS: [(&str, &str); 3] =
    [("enter", "edit"), ("ctrl+g", "editor"), ("ctrl+o", "reset")];
const MULTILINE_EDITING_HINTS: [(&str, &str); 3] =
    [("esc", "done"), ("ctrl+g", "editor"), ("ctrl+o", "reset")];
const BOOL_HINTS: [(&str, &str); 2] =
    [("space", "toggle"), ("ctrl+o", "reset")];
const CHOICE_HINTS: [(&str, &str); 2] =
    [("\u{2190}\u{2192}", "change"), ("ctrl+o", "reset")];
const DATE_HINTS: [(&str, &str); 3] =
    [("enter", "choose"), ("del", "clear"), ("ctrl+o", "reset")];

/// Every hint row the form can show, for its minimum width.
const ALL_HINTS: [&[(&str, &str)]; 9] = [
    &NORMAL_HINTS,
    &EDITING_HINTS,
    &TEXT_HINTS,
    &TEXT_EDITING_HINTS,
    &MULTILINE_HINTS,
    &MULTILINE_EDITING_HINTS,
    &BOOL_HINTS,
    &CHOICE_HINTS,
    &DATE_HINTS,
];

impl Form {
    pub(super) fn render(&self, frame: &mut Frame, skin: &Skin) {
        let outer = frame.area();
        let body: u16 =
            (0..self.fields.len()).map(|i| self.row_height(i)).sum();
        let footer = shortcut_hints::footer_height(HINT_ROWS);
        let width = self.box_width().min(outer.width);
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

        let row = RowLayout {
            label_width: self.label_width(),
            styles: FieldStyles::from_palette(&skin.palette),
        };
        for (index, field) in self.fields.iter().enumerate() {
            self.render_field(frame, rects[index], skin, &row, index, field);
        }
        if let Some(footer_rect) = rects.last() {
            self.render_hints(frame, *footer_rect, skin);
        }
    }

    /// The width of the label column: the longest label.
    fn label_width(&self) -> usize {
        field::label_column_width(self.fields.iter().map(Field::label))
    }

    /// The box width: label column and the widest opening value, but never
    /// narrower than a hint row. Taken from the baselines, so typing does not
    /// resize the box.
    fn box_width(&self) -> u16 {
        let values = self
            .fields
            .iter()
            .map(baseline_width)
            .max()
            .unwrap_or(0)
            .max(MIN_VALUE_WIDTH);
        let content = field::value_column(self.label_width()) + values;
        let hints = ALL_HINTS.iter().map(|row| hints_width(row)).max();
        let content = content.max(hints.unwrap_or(0));
        u16::try_from(content)
            .unwrap_or(u16::MAX)
            .saturating_add(MODAL_CHROME_WIDTH)
    }

    fn render_field(
        &self,
        frame: &mut Frame,
        rect: Rect,
        skin: &Skin,
        row: &RowLayout,
        index: usize,
        field: &Field,
    ) {
        let focused = index == self.focus;
        let error = self.error_for(index);
        let (rect, error_rect) = split_error_row(rect, error.is_some());
        if focused {
            // The tint reaches into the side padding, border to border.
            frame.render_widget(
                Block::default().style(row.styles.focus_tint),
                widen_into_padding(rect, frame.area()),
            );
        }
        if let (Some(message), Some(error_rect)) = (error, error_rect) {
            let indent = field::value_column(row.label_width);
            let line = field::problem_line(
                message,
                indent,
                error_rect.width as usize,
                &row.styles,
            );
            frame.render_widget(Paragraph::new(line), error_rect);
        }

        let marks = LabelMarks {
            changed: field.is_dirty(),
            focused,
            problem: error.is_some(),
        };
        let editing = focused && self.mode == Mode::Editing;
        if let FieldState::Multiline { area, .. } = &field.state {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(rect);
            let label = field::label_spans(&field.label, 0, marks, &row.styles);
            frame.render_widget(Paragraph::new(Line::from(label)), rows[0]);
            area.render(frame, rows[1], skin, editing);
            return;
        }

        let mut spans = field::label_cell(
            &field.label,
            row.label_width,
            marks,
            &row.styles,
        );
        let value_width = (rect.width as usize)
            .saturating_sub(field::value_column(row.label_width));
        spans.extend(value_spans(
            field,
            skin,
            &row.styles,
            value_width,
            editing,
        ));
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
    }

    /// The hint block: a spacer, the focused field's keys (or a notice), the
    /// form's keys.
    fn render_hints(&self, frame: &mut Frame, rect: Rect, skin: &Skin) {
        let width = rect.width as usize;
        let palette = &skin.palette;
        let form_hints: &[(&str, &str)] = match self.mode {
            Mode::Normal => &NORMAL_HINTS,
            Mode::Editing => &EDITING_HINTS,
        };
        let field_row = match &self.notice {
            Some(notice) => {
                Line::from(Span::styled(notice.clone(), style::accent(palette)))
            }
            None => hint_line(self.field_hints(), skin, width),
        };
        let mut lines = vec![Line::from(""), field_row];
        lines
            .extend(hint_block(form_hints, palette, width).into_iter().skip(1));
        frame.render_widget(Paragraph::new(lines), rect);
    }

    /// The keys of the focused field in the current mode.
    fn field_hints(&self) -> &'static [(&'static str, &'static str)] {
        let editing = self.mode == Mode::Editing;
        match self.fields[self.focus].state {
            FieldState::Text { .. } if editing => &TEXT_EDITING_HINTS,
            FieldState::Text { .. } => &TEXT_HINTS,
            FieldState::Multiline { .. } if editing => &MULTILINE_EDITING_HINTS,
            FieldState::Multiline { .. } => &MULTILINE_HINTS,
            FieldState::Bool { .. } => &BOOL_HINTS,
            FieldState::Choice { .. } => &CHOICE_HINTS,
            FieldState::Date { .. } => &DATE_HINTS,
        }
    }
}

/// What every row of one form shares.
struct RowLayout {
    label_width: usize,
    styles: FieldStyles,
}

/// One hint row drawn like the form's own keys.
fn hint_line(
    hints: &[(&str, &str)],
    skin: &Skin,
    width: usize,
) -> Line<'static> {
    hint_block(hints, &skin.palette, width)
        .into_iter()
        .nth(1)
        .unwrap_or_default()
}

/// The width of a field's opening value, for sizing the box.
fn baseline_width(field: &Field) -> usize {
    match &field.state {
        FieldState::Text { baseline, .. } => baseline.width(),
        FieldState::Multiline { .. } | FieldState::Bool { .. } => 0,
        FieldState::Choice { options, .. } => {
            options
                .iter()
                .map(|option| option.width())
                .max()
                .unwrap_or(0)
                + 4
        }
        FieldState::Date { .. } => field::PICKER_PLACEHOLDER.width() + 2,
    }
}

/// The value part of a single-row field, `width` columns wide.
fn value_spans(
    field: &Field,
    skin: &Skin,
    styles: &FieldStyles,
    width: usize,
    editing: bool,
) -> Vec<Span<'static>> {
    match &field.state {
        FieldState::Text { input, .. } => {
            input.render_line(&skin.palette, width, editing).spans
        }
        FieldState::Bool { value, .. } => {
            let mark = if *value { "[x]" } else { "[ ]" };
            vec![Span::raw(mark)]
        }
        FieldState::Choice { index, options, .. } => {
            let value = options.get(*index).map_or("", String::as_str);
            vec![Span::raw(skin.glyphs.stepper(value))]
        }
        FieldState::Date { value, .. } => {
            let text = value.map(|date| date.to_string()).unwrap_or_default();
            field::picker_value_spans(&text, width, skin.glyphs.pointer, styles)
        }
        FieldState::Multiline { .. } => Vec::new(),
    }
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
                Field::choice("Kind", vec!["bug".into(), "idea".into()], 0),
            ],
        )
    }

    /// Renders `form` into a `width`x16 screen and returns its rows as text.
    fn rows_at(form: &Form, width: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, 16)).unwrap();
        terminal.draw(|frame| form.render(frame, &skin())).unwrap();
        let buffer = terminal.backend().buffer();
        (0..16)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    fn rows(form: &Form) -> Vec<String> {
        rows_at(form, 70)
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
    fn an_empty_date_shows_the_placeholder() {
        let form = Form::new("Task", vec![Field::date("Due", None)]);
        let rows = rows(&form);
        assert!(
            rows.iter()
                .any(|row| row.contains(field::PICKER_PLACEHOLDER)),
            "{rows:#?}"
        );
        assert!(!rows.iter().any(|row| row.contains('\u{2014}')));
    }

    #[test]
    fn a_choice_shows_the_stepper_not_the_pointer() {
        let rows = rows(&form());
        let kind = rows
            .iter()
            .find(|row| row.contains("Kind"))
            .expect("the choice row is drawn");
        assert!(kind.contains("\u{2190} bug \u{2192}"), "{kind:?}");
    }

    #[test]
    fn values_start_two_columns_after_the_longest_label() {
        let rows = rows(&form());
        let title = rows.iter().find(|row| row.contains("Title")).unwrap();
        let due = rows.iter().find(|row| row.contains("Due")).unwrap();
        let title_value = title.find("Write").unwrap();
        let due_value = due.find("2026").unwrap();
        assert_eq!(title_value, due_value);
        assert_eq!(&title[title_value - 2..title_value], "  ");
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
    fn the_hint_rows_keep_every_key_at_the_narrowest_width() {
        let form = form();
        let width = form.box_width();
        let rows = rows_at(&form, width);
        let joined = rows.join("\n");
        assert!(joined.contains("esc leave (asks to save)"), "{joined}");
        assert!(joined.contains("ctrl+o reset"), "{joined}");
    }

    #[test]
    fn the_box_keeps_its_height_while_the_focus_moves() {
        let mut form = form();
        let height = |form: &Form| {
            rows(form).iter().filter(|row| row.contains('│')).count()
        };
        let first = height(&form);
        form.focus = 2;
        assert_eq!(height(&form), first);
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
        // Border, two padding columns, the change-marker column.
        assert!(title.contains("│   Title"), "{title:?}");
    }
}
