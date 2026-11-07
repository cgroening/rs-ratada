//! A schema-driven form modal, and the [`field`] building blocks every form
//! row is drawn from.
//!
//! Build a [`Form`] from a list of [`Field`]s and [`run`](Form::run) it with a
//! save callback. All fields are visible; `Tab`/`BackTab` (and `↑`/`↓`) step
//! between them, wrapping. The form opens in normal mode, where letters are
//! commands: `Enter` edits the focused text field (or toggles a checkbox,
//! opens the calendar of a date field), `←`/`→` step a choice, `s` saves and
//! `y` copies the focused value. While a field is edited, letters type into
//! it; `Enter` (single line) or `Esc` end the editing. In both modes `Ctrl+S`
//! saves, `Ctrl+O` puts the focused field back to its baseline and `Ctrl+G`
//! hands a multi-line field to `$EDITOR`.
//!
//! Saving calls the host's callback with the fields. It persists them or
//! returns a [`FieldProblem`]: one tied to a field shows in red under it and
//! takes the focus, any other in a message box. After a save the form stays
//! open with the saved values as the new baseline; a second save press within
//! a second closes it. `Esc` closes an unchanged form at once and asks "Save
//! changes?" otherwise.

use chrono::NaiveDate;

use super::{double_press::DoublePress, input::InputField, textarea::TextArea};

pub mod field;
mod render;
mod run;

const MULTILINE_ROWS: u16 = 4;

/// A read-only snapshot of a field's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    /// A single-line text value.
    Text(String),
    /// A multi-line text value.
    Multiline(String),
    /// A checkbox value.
    Bool(bool),
    /// The selected choice label.
    Choice(String),
    /// A date, or `None` when cleared.
    Date(Option<NaiveDate>),
}

/// A field's value and its baseline - the value the form opened with or last
/// saved, which the change marker compares against and `Ctrl+O` restores.
enum FieldState {
    Text {
        input: InputField,
        baseline: String,
    },
    Multiline {
        area: TextArea,
        baseline: String,
    },
    Bool {
        value: bool,
        baseline: bool,
    },
    Choice {
        index: usize,
        baseline: usize,
        options: Vec<String>,
    },
    Date {
        value: Option<NaiveDate>,
        baseline: Option<NaiveDate>,
    },
}

/// One labelled form field.
pub struct Field {
    label: String,
    state: FieldState,
}

impl Field {
    /// A single-line text field pre-filled with `initial`.
    pub fn text(label: impl Into<String>, initial: &str) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Text {
                input: InputField::new(initial),
                baseline: initial.to_string(),
            },
        }
    }

    /// A multi-line text field pre-filled with `initial`.
    pub fn multiline(label: impl Into<String>, initial: &str) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Multiline {
                area: TextArea::new(initial),
                baseline: initial.to_string(),
            },
        }
    }

    /// A checkbox field starting at `initial`.
    pub fn checkbox(label: impl Into<String>, initial: bool) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Bool {
                value: initial,
                baseline: initial,
            },
        }
    }

    /// A choice field over `options`, stepped with `←`/`→`, starting at
    /// index `initial` (clamped to the last option).
    pub fn choice(
        label: impl Into<String>,
        options: Vec<String>,
        initial: usize,
    ) -> Self {
        let index = initial.min(options.len().saturating_sub(1));
        Self {
            label: label.into(),
            state: FieldState::Choice {
                index,
                baseline: index,
                options,
            },
        }
    }

    /// A date field opening a calendar picker, pre-selecting `initial`.
    pub fn date(label: impl Into<String>, initial: Option<NaiveDate>) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Date {
                value: initial,
                baseline: initial,
            },
        }
    }

    /// The field's label.
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
            FieldState::Choice { index, options, .. } => FieldValue::Choice(
                options.get(*index).cloned().unwrap_or_default(),
            ),
            FieldState::Date { value, .. } => FieldValue::Date(*value),
        }
    }

    /// Whether the value differs from its baseline.
    pub fn is_dirty(&self) -> bool {
        match &self.state {
            FieldState::Text { input, baseline } => input.value() != baseline,
            FieldState::Multiline { area, baseline } => area.text() != baseline,
            FieldState::Bool { value, baseline } => value != baseline,
            FieldState::Choice {
                index, baseline, ..
            } => index != baseline,
            FieldState::Date { value, baseline } => value != baseline,
        }
    }

    /// Puts the value back to its baseline.
    fn reset(&mut self) {
        match &mut self.state {
            FieldState::Text { input, baseline } => {
                *input = InputField::new(baseline);
            }
            FieldState::Multiline { area, baseline } => {
                area.set_text(baseline.clone());
            }
            FieldState::Bool { value, baseline } => *value = *baseline,
            FieldState::Choice {
                index, baseline, ..
            } => *index = *baseline,
            FieldState::Date { value, baseline } => *value = *baseline,
        }
    }

    /// Makes the current value the baseline (after a save).
    fn mark_clean(&mut self) {
        match &mut self.state {
            FieldState::Text { input, baseline } => {
                *baseline = input.value().to_string();
            }
            FieldState::Multiline { area, baseline } => {
                *baseline = area.text().to_string();
            }
            FieldState::Bool { value, baseline } => *baseline = *value,
            FieldState::Choice {
                index, baseline, ..
            } => *baseline = *index,
            FieldState::Date { value, baseline } => *baseline = *value,
        }
    }

    /// The value as the text `y` copies; `None` when there is none.
    fn copy_text(&self) -> Option<String> {
        let text = match self.value() {
            FieldValue::Text(text)
            | FieldValue::Multiline(text)
            | FieldValue::Choice(text) => text,
            FieldValue::Bool(value) => value.to_string(),
            FieldValue::Date(date) => date?.to_string(),
        };
        (!text.is_empty()).then_some(text)
    }

    /// Whether `Enter` starts typing into this field.
    fn is_typed(&self) -> bool {
        matches!(
            self.state,
            FieldState::Text { .. } | FieldState::Multiline { .. }
        )
    }

    fn height(&self) -> u16 {
        match self.state {
            FieldState::Multiline { .. } => MULTILINE_ROWS + 1,
            _ => 1,
        }
    }
}

/// A problem the save callback reports: the save did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProblem {
    /// The index of the field to blame, or `None` for a problem of the whole
    /// form (a write conflict, an I/O error).
    pub field: Option<usize>,
    /// What is wrong, as the user should read it.
    pub message: String,
}

impl FieldProblem {
    /// A problem with the field at `index`: it shows in red under the field,
    /// which takes the focus.
    pub fn at(index: usize, message: impl Into<String>) -> Self {
        Self {
            field: Some(index),
            message: message.into(),
        }
    }

    /// A problem of the whole form: it shows in a message box.
    pub fn form(message: impl Into<String>) -> Self {
        Self {
            field: None,
            message: message.into(),
        }
    }
}

/// How the form was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormOutcome {
    /// The form was closed after at least one successful save.
    Saved,
    /// The form was closed without saving anything.
    Cancelled,
    /// The global quit chord was pressed (saves made before still stand).
    Quit,
}

/// Whether letters are commands or type into the focused field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Editing,
}

/// A modal form over a set of [`Field`]s.
pub struct Form {
    title: String,
    fields: Vec<Field>,
    focus: usize,
    mode: Mode,
    /// A field problem: the field index and its message.
    error: Option<(usize, String)>,
    /// A short confirmation shown in the hint block until the next key.
    notice: Option<String>,
    save_press: DoublePress,
    has_saved: bool,
}

impl Form {
    /// A form titled `title` over `fields`, focused on the first field.
    pub fn new(title: impl Into<String>, fields: Vec<Field>) -> Self {
        Self {
            title: title.into(),
            fields,
            focus: 0,
            mode: Mode::Normal,
            error: None,
            notice: None,
            save_press: DoublePress::new(),
            has_saved: false,
        }
    }

    /// The fields, for reading the values.
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Whether any field differs from its baseline.
    pub fn is_dirty(&self) -> bool {
        self.fields.iter().any(Field::is_dirty)
    }

    /// Reports a problem with the field at `index` before the form runs: it
    /// shows in red under that field, its label red, and focuses it. The
    /// message clears on the next key. An index past the last field is
    /// ignored.
    pub fn set_error(&mut self, index: usize, message: impl Into<String>) {
        if index < self.fields.len() {
            self.focus = index;
            self.error = Some((index, message.into()));
        }
    }

    /// The message reported for the field at `index`, if any.
    fn error_for(&self, index: usize) -> Option<&str> {
        self.error
            .as_ref()
            .filter(|(field, _)| *field == index)
            .map(|(_, message)| message.as_str())
    }

    /// The rows the field at `index` takes, with its error line.
    fn row_height(&self, index: usize) -> u16 {
        self.fields[index].height() + u16::from(self.error_for(index).is_some())
    }

    /// Puts the focused field back to its baseline.
    fn reset_focused(&mut self) {
        self.fields[self.focus].reset();
    }

    /// Makes every current value the baseline (after a save).
    fn mark_clean(&mut self) {
        for field in &mut self.fields {
            field.mark_clean();
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{run::toggles_checkbox, *};

    /// `Ctrl+Space` is a chord, not a toggle: crossterm reports it as
    /// `Char(' ') + CONTROL`, so without the guard a host's `Ctrl+Space` would
    /// also flip the focused checkbox. `AltGr+Space` counts too - a character,
    /// but not a bare one.
    #[test]
    fn only_a_bare_space_toggles_a_checkbox() {
        for modified in [
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL),
            KeyEvent::new(
                KeyCode::Char(' '),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::ALT),
        ] {
            assert!(
                !toggles_checkbox(modified),
                "{modified:?} must not toggle the checkbox"
            );
        }
        // The bare keys still toggle.
        assert!(toggles_checkbox(KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE
        )));
        assert!(toggles_checkbox(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE
        )));
    }

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

    #[test]
    fn reset_restores_the_baseline_and_a_save_moves_it() {
        let mut field = Field::checkbox("done", false);
        if let FieldState::Bool { value, .. } = &mut field.state {
            *value = true;
        }
        field.reset();
        assert_eq!(field.value(), FieldValue::Bool(false));

        if let FieldState::Bool { value, .. } = &mut field.state {
            *value = true;
        }
        field.mark_clean();
        assert!(!field.is_dirty());
        field.reset();
        assert_eq!(field.value(), FieldValue::Bool(true));
    }
}
