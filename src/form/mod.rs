//! A schema-driven form modal.
//!
//! Build it from a list of [`Field`]s, run it, then read the values back. All
//! fields are visible; `Tab`/`BackTab` step between them (wrapping); the
//! focused row is tinted, changed fields get a `*` marker, `Ctrl+S` saves and
//! `Esc` cancels. Field behaviour: text edits inline; multiline edits inline
//! and opens `$EDITOR` with `Ctrl+G`; bool toggles with `Space`; choice cycles
//! with `←`/`→`; date opens the calendar with `Enter` (marked by the pointer
//! glyph).
//!
//! The form does not validate: after [`FormOutcome::Saved`] the host checks
//! the values and, on a problem, calls [`Form::set_error`] and runs the form
//! again. The message shows in red under its field until the next key.

use chrono::NaiveDate;

use super::{input::InputField, textarea::TextArea};

mod render;
mod run;

const MULTILINE_ROWS: u16 = 4;
const LABEL_WIDTH: u16 = 14;

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
    /// A single-line text field pre-filled with `initial`.
    pub fn text(label: impl Into<String>, initial: &str) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Text {
                input: InputField::new(initial),
                initial: initial.to_string(),
            },
        }
    }

    /// A multi-line text field pre-filled with `initial`.
    pub fn multiline(label: impl Into<String>, initial: &str) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Multiline {
                area: TextArea::new(initial),
                initial: initial.to_string(),
            },
        }
    }

    /// A checkbox field starting at `initial`.
    pub fn checkbox(label: impl Into<String>, initial: bool) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Bool {
                value: initial,
                initial,
            },
        }
    }

    /// A cycling choice field over `options`, starting at index `initial`.
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

    /// A date field opening a calendar picker, pre-selecting `initial`.
    pub fn date(label: impl Into<String>, initial: Option<NaiveDate>) -> Self {
        Self {
            label: label.into(),
            state: FieldState::Date {
                value: initial,
                initial,
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
    /// The user saved (`Ctrl+S`).
    Saved,
    /// The user cancelled (`Esc`).
    Cancelled,
    /// The global quit chord was pressed.
    Quit,
}

/// A modal form over a set of [`Field`]s.
pub struct Form {
    title: String,
    fields: Vec<Field>,
    focus: usize,
    /// A host-reported problem: the field index and its message.
    error: Option<(usize, String)>,
}

impl Form {
    /// A form titled `title` over `fields`, focused on the first field.
    pub fn new(title: impl Into<String>, fields: Vec<Field>) -> Self {
        Self {
            title: title.into(),
            fields,
            focus: 0,
            error: None,
        }
    }

    /// The fields, for reading values after [`FormOutcome::Saved`].
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Reports a problem with the field at `index`: the next [`Form::run`]
    /// shows `message` in red under that field, its label red, and focuses it.
    /// The message clears on the next key. An index past the last field is
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
}
