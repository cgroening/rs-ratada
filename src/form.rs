//! A schema-driven form modal.
//!
//! Build it from a list of [`Field`]s, run it, then read the values back. All
//! fields are visible; `Tab`/`BackTab` step between them (wrapping); the
//! focused row is tinted, changed fields get a `*` marker, `Ctrl+S` saves and
//! `Esc` cancels. Field behaviour: text edits inline; multiline edits inline
//! and opens `$EDITOR` with `Ctrl+G`; bool toggles with `Space`; choice cycles
//! with `←`/`→`; date opens the calendar with `Enter`.

use std::io;

use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use super::{
    date_picker, editor, footer,
    input::InputField,
    layout::centered_rect,
    modal::ModalSignal,
    nav, overlay, style,
    terminal::{Tui, TuiEvent},
    textarea::TextArea,
};
use crate::theme::Skin;

const MULTILINE_ROWS: u16 = 4;
const LABEL_WIDTH: u16 = 14;

/// A read-only snapshot of a field's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Text(String),
    Multiline(String),
    Bool(bool),
    Choice(String),
    Date(Option<NaiveDate>),
}

enum FieldState {
    Text {
        input: InputField,
        initial: String,
    },
    Multiline {
        area: TextArea,
        initial: String,
    },
    Bool {
        value: bool,
        initial: bool,
    },
    Choice {
        index: usize,
        initial: usize,
        options: Vec<String>,
    },
    Date {
        value: Option<NaiveDate>,
        initial: Option<NaiveDate>,
    },
}

/// One labelled form field.
pub struct Field {
    label: String,
    state: FieldState,
}

impl Field {
    pub fn text(label: impl Into<String>, initial: &str) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Text {
                input: InputField::new(initial),
                initial: initial.to_string(),
            },
        }
    }

    pub fn multiline(label: impl Into<String>, initial: &str) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Multiline {
                area: TextArea::new(initial),
                initial: initial.to_string(),
            },
        }
    }

    pub fn checkbox(label: impl Into<String>, initial: bool) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Bool {
                value: initial,
                initial,
            },
        }
    }

    pub fn choice(
        label: impl Into<String>,
        options: Vec<String>,
        initial: usize,
    ) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Choice {
                index: initial.min(options.len().saturating_sub(1)),
                initial,
                options,
            },
        }
    }

    pub fn date(label: impl Into<String>, initial: Option<NaiveDate>) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Date {
                value: initial,
                initial,
            },
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// The current value of the field.
    pub fn value(&self) -> FieldValue {
        match &self.state {
            FieldState::Text { input, .. } => {
                FieldValue::Text(input.value().to_string())
            }
            FieldState::Multiline { area, .. } => {
                FieldValue::Multiline(area.text().to_string())
            }
            FieldState::Bool { value, .. } => FieldValue::Bool(*value),
            FieldState::Choice { index, options, .. } => {
                FieldValue::Choice(options[*index].clone())
            }
            FieldState::Date { value, .. } => FieldValue::Date(*value),
        }
    }

    /// Whether the value changed from its initial.
    pub fn is_dirty(&self) -> bool {
        match &self.state {
            FieldState::Text { input, initial } => input.value() != initial,
            FieldState::Multiline { area, initial } => area.text() != initial,
            FieldState::Bool { value, initial } => value != initial,
            FieldState::Choice { index, initial, .. } => index != initial,
            FieldState::Date { value, initial } => value != initial,
        }
    }

    fn height(&self) -> u16 {
        match self.state {
            FieldState::Multiline { .. } => MULTILINE_ROWS + 1,
            _ => 1,
        }
    }
}

/// How the form was closed.
pub enum FormOutcome {
    Saved,
    Cancelled,
    Quit,
}

/// A modal form over a set of [`Field`]s.
pub struct Form {
    title: String,
    fields: Vec<Field>,
    focus: usize,
}

impl Form {
    pub fn new(title: impl Into<String>, fields: Vec<Field>) -> Self {
        Self {
            title: title.into(),
            fields,
            focus: 0,
        }
    }

    /// The fields, for reading values after [`FormOutcome::Saved`].
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Runs the form until the user saves, cancels or quits.
    ///
    /// # Errors
    /// Propagates terminal I/O errors.
    pub fn run(
        &mut self,
        tui: &mut Tui,
        skin: &Skin,
        render_bg: impl Fn(&mut Frame),
    ) -> io::Result<FormOutcome> {
        if self.fields.is_empty() {
            return Ok(FormOutcome::Cancelled);
        }
        loop {
            tui.draw(|frame| {
                render_bg(frame);
                overlay::dim(frame, overlay::SCRIM_FACTOR);
                self.render(frame, skin);
            })?;
            match tui.read_event()? {
                TuiEvent::Quit => return Ok(FormOutcome::Quit),
                TuiEvent::Resize => {}
                TuiEvent::Key(key) => {
                    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                    match key.code {
                        KeyCode::Esc => return Ok(FormOutcome::Cancelled),
                        KeyCode::Char('s') if ctrl => {
                            return Ok(FormOutcome::Saved);
                        }
                        KeyCode::Tab => {
                            self.focus =
                                nav::cycle(self.focus, self.fields.len(), 1);
                        }
                        KeyCode::BackTab => {
                            self.focus =
                                nav::cycle(self.focus, self.fields.len(), -1);
                        }
                        _ => {
                            if let Some(outcome) =
                                self.handle_field_key(key, tui, skin)?
                            {
                                return Ok(outcome);
                            }
                        }
                    }
                }
            }
        }
    }

    fn handle_field_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        tui: &mut Tui,
        skin: &Skin,
    ) -> io::Result<Option<FormOutcome>> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // Date opens a sub-modal that renders the form as its background, so it
        // is handled with whole-self borrows before the per-field borrow.
        if key.code == KeyCode::Enter
            && matches!(self.fields[self.focus].state, FieldState::Date { .. })
        {
            return self.open_date_picker(tui, skin);
        }
        if ctrl && key.code == KeyCode::Char('g') {
            self.open_editor_for_focus(tui)?;
            return Ok(None);
        }

        match &mut self.fields[self.focus].state {
            FieldState::Text { input, .. } => {
                input.handle_key(key);
            }
            FieldState::Multiline { area, .. } => {
                area.handle_key(key);
            }
            FieldState::Bool { value, .. } => {
                if matches!(key.code, KeyCode::Char(' ') | KeyCode::Enter) {
                    *value = !*value;
                }
            }
            FieldState::Choice { index, options, .. } => match key.code {
                KeyCode::Left => *index = nav::cycle(*index, options.len(), -1),
                KeyCode::Right => *index = nav::cycle(*index, options.len(), 1),
                _ => {}
            },
            FieldState::Date { .. } => {}
        }
        Ok(None)
    }

    fn open_date_picker(
        &mut self,
        tui: &mut Tui,
        skin: &Skin,
    ) -> io::Result<Option<FormOutcome>> {
        let current = match &self.fields[self.focus].state {
            FieldState::Date { value, .. } => *value,
            _ => None,
        };
        let signal = {
            let form: &Form = self;
            date_picker::date_picker(
                tui,
                skin,
                " Date ",
                current,
                true,
                |frame| form.render(frame, skin),
            )?
        };
        match signal {
            ModalSignal::Quit => return Ok(Some(FormOutcome::Quit)),
            ModalSignal::Cancelled => {}
            ModalSignal::Value(date) => {
                if let FieldState::Date { value, .. } =
                    &mut self.fields[self.focus].state
                {
                    *value = date;
                }
            }
        }
        Ok(None)
    }

    fn open_editor_for_focus(&mut self, tui: &mut Tui) -> io::Result<()> {
        let FieldState::Multiline { area, .. } = &self.fields[self.focus].state
        else {
            return Ok(());
        };
        let initial = area.text().to_string();
        let command = editor::resolve_editor();
        if let Some(text) = editor::edit_in_editor(tui, &command, &initial)?
            && let FieldState::Multiline { area, .. } =
                &mut self.fields[self.focus].state
        {
            area.set_text(text);
        }
        Ok(())
    }

    fn render(&self, frame: &mut Frame, skin: &Skin) {
        let palette = &skin.palette;
        let outer = frame.area();
        let body: u16 = self.fields.iter().map(Field::height).sum();
        let width = (outer.width * 2 / 3).clamp(40, outer.width);
        let height = (body + 3).min(outer.height);
        let area = centered_rect(width, height, outer);
        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style::fg(palette.accent))
            .style(style::bg(palette.background))
            .title(Span::styled(
                format!("\u{2500} {} ", self.title),
                style::fg(palette.accent)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut constraints: Vec<Constraint> = self
            .fields
            .iter()
            .map(|f| Constraint::Length(f.height()))
            .collect();
        constraints.push(Constraint::Min(1));
        let rects = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        for (index, field) in self.fields.iter().enumerate() {
            self.render_field(frame, rects[index], skin, index, field);
        }
        if let Some(footer_rect) = rects.last() {
            let hints = footer::lines(
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
                Block::default().style(style::bg(palette.selection_bg)),
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
                Paragraph::new(Line::from(Span::styled(label, style::dim()))),
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
            Paragraph::new(Line::from(Span::styled(label, style::dim()))),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_tracks_changes() {
        let field = Field::checkbox("done", false);
        assert!(!field.is_dirty());
        let mut field = Field::choice("p", vec!["a".into(), "b".into()], 0);
        assert!(!field.is_dirty());
        if let FieldState::Choice { index, .. } = &mut field.state {
            *index = 1;
        }
        assert!(field.is_dirty());
        assert_eq!(field.value(), FieldValue::Choice("b".to_string()));
    }
}
