//! Forms: the non-blocking [`FormState`] core every app's form is built on,
//! the blocking [`Form`] loop over it, and the [`field`] building blocks
//! every form row is drawn from.
//!
//! A host with its own event loop (one that must keep draining background
//! work) holds a [`FormState`], feeds it keys and acts on the [`FormEvent`]s
//! it returns. A host without one builds a [`Form`] from [`Field`]s and
//! [`run`](Form::run)s it over a [`FormHost`], which draws the view behind
//! and persists the values or returns a [`FieldProblem`] (shown under its field, or in a
//! message box for the whole form); after a save the form stays open, a
//! second save press within a second closes it, `Esc` on a changed form asks
//! "Save changes?". The keys are those of [`FormState`].

use chrono::NaiveDate;

pub mod field;
mod run;

pub use run::FormHost;
mod state;

pub use state::{
    FieldKind, FormEvent, FormField, FormState, MULTILINE_ROWS, NO, YES,
    hint_rows, is_yes, yes_no,
};

/// The format a date field keeps its value in.
const DATE_FORMAT: &str = "%Y-%m-%d";

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

/// What kind of value a [`Field`] holds.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Text,
    Multiline,
    Bool,
    Choice(Vec<String>),
    Date,
}

/// One labelled field of a [`Form`], and the snapshot
/// [`FormHost::save`] reads.
#[derive(Debug, Clone)]
pub struct Field {
    label: String,
    value: String,
    kind: Kind,
}

impl Field {
    /// A single-line text field pre-filled with `initial`.
    pub fn text(label: impl Into<String>, initial: &str) -> Self {
        Field::new(label, initial, Kind::Text)
    }

    /// A multi-line text field pre-filled with `initial`.
    pub fn multiline(label: impl Into<String>, initial: &str) -> Self {
        Field::new(label, initial, Kind::Multiline)
    }

    /// A checkbox field starting at `initial`.
    pub fn checkbox(label: impl Into<String>, initial: bool) -> Self {
        Field::new(label, yes_no(initial), Kind::Bool)
    }

    /// A choice field over `options`, stepped with `←`/`→`, starting at
    /// index `initial` (clamped to the last option).
    pub fn choice(
        label: impl Into<String>,
        options: Vec<String>,
        initial: usize,
    ) -> Self {
        let index = initial.min(options.len().saturating_sub(1));
        let value = options.get(index).cloned().unwrap_or_default();
        Field::new(label, &value, Kind::Choice(options))
    }

    /// A date field opening a calendar picker, pre-selecting `initial`.
    pub fn date(label: impl Into<String>, initial: Option<NaiveDate>) -> Self {
        let value = initial
            .map(|date| date.format(DATE_FORMAT).to_string())
            .unwrap_or_default();
        Field::new(label, &value, Kind::Date)
    }

    fn new(label: impl Into<String>, value: &str, kind: Kind) -> Self {
        Field {
            label: label.into(),
            value: value.to_string(),
            kind,
        }
    }

    /// The field's label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The value of the field.
    pub fn value(&self) -> FieldValue {
        match &self.kind {
            Kind::Text => FieldValue::Text(self.value.clone()),
            Kind::Multiline => FieldValue::Multiline(self.value.clone()),
            Kind::Bool => FieldValue::Bool(is_yes(&self.value)),
            Kind::Choice(_) => FieldValue::Choice(self.value.clone()),
            Kind::Date => FieldValue::Date(
                NaiveDate::parse_from_str(&self.value, DATE_FORMAT).ok(),
            ),
        }
    }

    /// The core field for this one, keyed by its position.
    fn to_form_field(&self, index: usize) -> FormField {
        let key = index.to_string();
        match &self.kind {
            Kind::Text => FormField::text(&key, &self.label, &self.value),
            Kind::Multiline => {
                FormField::multiline(&key, &self.label, &self.value)
            }
            Kind::Bool => {
                FormField::toggle(&key, &self.label, is_yes(&self.value))
            }
            Kind::Choice(options) => FormField::stepper(
                &key,
                &self.label,
                options.clone(),
                &self.value,
            ),
            Kind::Date => {
                FormField::picker(&key, &self.label, &self.value).clearable()
            }
        }
    }
}

/// A problem [`FormHost::save`] reports: the save did not happen.
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

/// A blocking modal form over a set of [`Field`]s, run on a [`FormState`].
pub struct Form {
    state: FormState,
    kinds: Vec<Kind>,
}

impl Form {
    /// A form titled `title` over `fields`, focused on the first field.
    pub fn new(title: impl Into<String>, fields: Vec<Field>) -> Self {
        let form_fields = fields
            .iter()
            .enumerate()
            .map(|(index, field)| field.to_form_field(index))
            .collect();
        let kinds = fields.into_iter().map(|field| field.kind).collect();
        Form {
            state: FormState::new(title, form_fields),
            kinds,
        }
    }

    /// The fields with their current values.
    pub fn fields(&self) -> Vec<Field> {
        self.state
            .fields()
            .iter()
            .zip(&self.kinds)
            .map(|(field, kind)| Field {
                label: field.label.clone(),
                value: field.value.clone(),
                kind: kind.clone(),
            })
            .collect()
    }

    /// Whether any field differs from its baseline.
    pub fn is_dirty(&self) -> bool {
        self.state.is_dirty()
    }

    /// Reports a problem with the field at `index` before the form runs (see
    /// [`FormState::set_error`]). An index past the last field is ignored.
    pub fn set_error(&mut self, index: usize, message: impl Into<String>) {
        self.state.set_error(&index.to_string(), message);
    }

    /// The core state, for a test or a host that draws the form itself.
    pub fn state(&self) -> &FormState {
        &self.state
    }
}
