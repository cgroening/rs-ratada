//! The form core: a non-blocking form state every app's form is built on.
//!
//! A [`FormState`] holds labelled fields, the focus, the normal and editing
//! modes, the baseline the change markers and `Ctrl+O` compare against, a
//! problem shown under its field, a short notice, the "Save changes?"
//! question and the double press that closes the form after a save. It never
//! blocks: [`FormState::handle_key`] turns a key into a [`FormEvent`] the
//! host acts on (save, close, open the picker of a field), and the host draws
//! it with [`FormState::render`] (a modal box) or [`FormState::lines`] (rows
//! for a layout of its own). [`Form`](super::Form) is the blocking loop over
//! it for hosts without an event loop of their own.
//!
//! Keys in normal mode: `Tab`/`BackTab`, `↑`/`↓` and `j`/`k` move the focus
//! (wrapping, section bars skipped); `Enter` edits a text field, toggles a
//! checkbox, steps a stepper or asks for a field's picker (`e` types into a
//! text picker); `Space` toggles, `←`/`→` (`h`/`l`) step, `Del`/`Backspace`
//! clear a clearable picker; `s` saves, `y` copies the focused value, `Esc`
//! closes (asking first when something changed). While a field is typed
//! into, letters type; `Enter` (single line) or `Esc` end the editing, `Tab`
//! ends it and moves on. In both modes `Ctrl+S` saves, `Ctrl+O` puts the
//! focused field back to its baseline and `Ctrl+G` asks for the external
//! editor of a multi-line field.

mod hints;
mod keys;
mod render;
#[cfg(test)]
mod tests;

pub use hints::hint_rows;
pub use render::MULTILINE_ROWS;

use ratatui::style::Style;

use crate::{
    autocomplete::Autocomplete, double_press::DoublePress, input::InputField,
    textarea::TextArea,
};

/// The value a toggle holds when on.
pub const YES: &str = "yes";

/// The value a toggle holds when off.
pub const NO: &str = "no";

/// The toggle value of `flag`.
pub fn yes_no(flag: bool) -> &'static str {
    if flag { YES } else { NO }
}

/// Whether a toggle value reads as on.
pub fn is_yes(value: &str) -> bool {
    value == YES
}

/// How a field is edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// Free text, typed in place after `Enter` (or `e`).
    Text,
    /// Multi-line text, typed in place (`Enter` adds a line, `Esc` ends).
    Multiline,
    /// Yes or no ([`YES`]/[`NO`]), flipped by `Space`/`Enter`.
    Toggle,
    /// One of `options`, stepped with `←`/`→` and shown between the stepper
    /// glyphs.
    Stepper(Vec<String>),
    /// One of `options`, picked in a list the host opens on
    /// [`FormEvent::Activate`] (see [`FormState::choose`]).
    Choice(Vec<String>),
    /// A value chosen in a picker the host opens on [`FormEvent::Activate`].
    Picker,
    /// A picker whose value can also be typed: `Enter` asks for the picker,
    /// `e` types.
    TextPicker,
    /// Edited by the host in an editor of its own.
    Owned,
    /// Shown only; the host computes it.
    ReadOnly,
    /// A section bar between groups of fields; never focused.
    Section,
}

/// One labelled field.
#[derive(Debug, Clone)]
pub struct FormField {
    /// What the host calls the field.
    pub key: String,
    /// The label shown in the label column.
    pub label: String,
    /// The value as shown and edited.
    pub value: String,
    /// How the field is edited.
    pub kind: FieldKind,
    /// Values offered while typing (empty: none).
    pub suggestions: Vec<String>,
    /// Side information right-aligned in the field row (a counter, a
    /// calculated result, a hint).
    pub side_info: String,
    /// Whether the side information is a warning (drawn in the error
    /// colour).
    pub is_warning: bool,
    /// A style for the value.
    pub value_style: Style,
    /// Why the field cannot be changed now (shown dimmed as its side
    /// information); empty while it can.
    pub lock: String,
    /// Whether the value differs across the entries a bulk edit covers; it
    /// shows as mixed until the user changes it.
    pub is_mixed: bool,
    /// Whether `Del`/`Backspace` empty a picker field.
    pub is_clearable: bool,
    /// How the value is compared with its baseline (a calculation by its
    /// result, say); `None` compares the trimmed text.
    pub normalize: Option<fn(&str) -> String>,
    /// The most characters a text field takes; it then shows `used/max` as
    /// its side information while it has none of its own.
    pub max_chars: Option<usize>,
}

impl FormField {
    fn new(key: &str, label: &str, value: &str, kind: FieldKind) -> Self {
        FormField {
            key: key.to_string(),
            label: label.to_string(),
            value: value.to_string(),
            kind,
            suggestions: Vec::new(),
            side_info: String::new(),
            is_warning: false,
            value_style: Style::default(),
            lock: String::new(),
            is_mixed: false,
            is_clearable: false,
            normalize: None,
            max_chars: None,
        }
    }

    /// A single-line text field.
    pub fn text(key: &str, label: &str, value: &str) -> Self {
        FormField::new(key, label, value, FieldKind::Text)
    }

    /// A multi-line text field.
    pub fn multiline(key: &str, label: &str, value: &str) -> Self {
        FormField::new(key, label, value, FieldKind::Multiline)
    }

    /// A toggle, on when `on`.
    pub fn toggle(key: &str, label: &str, on: bool) -> Self {
        FormField::new(key, label, yes_no(on), FieldKind::Toggle)
    }

    /// A stepper over `options`, on `value`.
    pub fn stepper(
        key: &str,
        label: &str,
        options: Vec<String>,
        value: &str,
    ) -> Self {
        FormField::new(key, label, value, FieldKind::Stepper(options))
    }

    /// A choice from `options` in a picker list, on `value`.
    pub fn choice(
        key: &str,
        label: &str,
        options: Vec<String>,
        value: &str,
    ) -> Self {
        FormField::new(key, label, value, FieldKind::Choice(options))
    }

    /// A field whose value the host picks (a date, a category, a path).
    pub fn picker(key: &str, label: &str, value: &str) -> Self {
        FormField::new(key, label, value, FieldKind::Picker)
    }

    /// A picker whose value can also be typed.
    pub fn text_picker(key: &str, label: &str, value: &str) -> Self {
        FormField::new(key, label, value, FieldKind::TextPicker)
    }

    /// A value only the host changes, with an editor of its own.
    pub fn owned(key: &str, label: &str, value: &str) -> Self {
        FormField::new(key, label, value, FieldKind::Owned)
    }

    /// A value the host computes, shown only.
    pub fn read_only(key: &str, label: &str, value: &str) -> Self {
        FormField::new(key, label, value, FieldKind::ReadOnly)
    }

    /// A section bar titled `label`.
    pub fn section(label: &str) -> Self {
        FormField::new("", label, "", FieldKind::Section)
    }

    /// The field with `suggestions` offered while typing.
    #[must_use]
    pub fn with_suggestions(mut self, suggestions: Vec<String>) -> Self {
        self.suggestions = suggestions;
        self
    }

    /// The field marked as mixed (a bulk edit over differing values).
    #[must_use]
    pub fn mixed(mut self) -> Self {
        self.is_mixed = true;
        self
    }

    /// The field emptied by `Del`/`Backspace` (a picker).
    #[must_use]
    pub fn clearable(mut self) -> Self {
        self.is_clearable = true;
        self
    }

    /// The field compared with its baseline through `normalize`.
    #[must_use]
    pub fn normalized(mut self, normalize: fn(&str) -> String) -> Self {
        self.normalize = Some(normalize);
        self
    }

    /// The field limited to `max` characters (typing and pasting stop
    /// there), with a `used/max` counter.
    #[must_use]
    pub fn max_chars(mut self, max: usize) -> Self {
        self.max_chars = Some(max);
        self
    }

    /// Whether the user changes the value.
    fn tracks_changes(&self) -> bool {
        !matches!(self.kind, FieldKind::ReadOnly | FieldKind::Section)
    }

    /// Whether the field opens a picker on `Enter`.
    fn is_picker(&self) -> bool {
        matches!(
            self.kind,
            FieldKind::Choice(_) | FieldKind::Picker | FieldKind::TextPicker
        )
    }

    /// Whether the focus can rest on the field.
    fn is_focusable(&self) -> bool {
        self.kind != FieldKind::Section
    }

    /// `value` as the field compares it with its baseline.
    fn comparable(&self, value: &str) -> String {
        match self.normalize {
            Some(normalize) => normalize(value),
            None => value.trim().to_string(),
        }
    }
}

/// What a key asks the host to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormEvent {
    /// Nothing beyond drawing again.
    Pending,
    /// The value of the field `key` changed (the host may recompute side
    /// information).
    Changed(String),
    /// Open the picker (or the host's editor) of the field `key`; the host
    /// sets the value with [`FormState::set_value`].
    Activate(String),
    /// Hand the multi-line field `key` to the external editor.
    ExternalEditor(String),
    /// Save the values; the host answers with [`FormState::mark_saved`] or
    /// [`FormState::set_error`]. The form stays open (see
    /// [`FormState::closes_after_save`]).
    Save,
    /// Close the form: it was saved and nothing changed since.
    Close,
    /// Close the form without saving.
    Discard,
}

/// Whether letters are commands or type into the focused field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Editing,
}

/// The editor of the field being typed into.
enum Editor {
    Line(InputField),
    Area(TextArea),
}

/// The field being typed into and its suggestions.
struct Editing {
    index: usize,
    editor: Editor,
    complete: Autocomplete,
}

/// A form over labelled fields, driven by the host's event loop.
pub struct FormState {
    title: String,
    fields: Vec<FormField>,
    focus: usize,
    mode: Mode,
    editing: Option<Editing>,
    /// The values when the form opened or was last saved.
    baseline: Vec<String>,
    /// Which fields were mixed then: a mixed field that got a value counts
    /// as changed, even when the value is the one it held underneath.
    baseline_mixed: Vec<bool>,
    /// A problem with a field, shown in red under it until the next key.
    error: Option<(usize, String)>,
    /// A short confirmation shown in the hint rows until the next key.
    notice: Option<String>,
    asking_save: bool,
    close_after_save: bool,
    save_press: DoublePress,
    has_saved: bool,
}

impl FormState {
    /// A form titled `title` over `fields`, focused on the first focusable
    /// field; the values it opens with are its baseline.
    pub fn new(title: impl Into<String>, fields: Vec<FormField>) -> Self {
        let baseline = fields.iter().map(|field| field.value.clone()).collect();
        let baseline_mixed =
            fields.iter().map(|field| field.is_mixed).collect();
        let focus = fields.iter().position(FormField::is_focusable);
        FormState {
            title: title.into(),
            fields,
            focus: focus.unwrap_or(0),
            mode: Mode::Normal,
            editing: None,
            baseline,
            baseline_mixed,
            error: None,
            notice: None,
            asking_save: false,
            close_after_save: false,
            save_press: DoublePress::new(),
            has_saved: false,
        }
    }

    /// The form's title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The fields, top to bottom.
    pub fn fields(&self) -> &[FormField] {
        &self.fields
    }

    /// The field `key`.
    pub fn field(&self, key: &str) -> Option<&FormField> {
        self.fields.iter().find(|field| field.key == key)
    }

    fn field_mut(&mut self, key: &str) -> Option<&mut FormField> {
        self.fields.iter_mut().find(|field| field.key == key)
    }

    /// The index of the field `key`.
    pub fn index_of(&self, key: &str) -> Option<usize> {
        self.fields.iter().position(|field| field.key == key)
    }

    /// The value of the field `key` (empty for an unknown key).
    pub fn value(&self, key: &str) -> &str {
        self.field(key).map_or("", |field| field.value.as_str())
    }

    /// The value of the field `key` as it stands: the editor's text while
    /// that field is being typed, else its value.
    pub fn live_value(&self, key: &str) -> String {
        if let Some(editing) = &self.editing
            && self.fields.get(editing.index).is_some_and(|f| f.key == key)
        {
            return editing.editor.text().to_string();
        }
        self.value(key).to_string()
    }

    /// Every field's value, in order.
    pub fn values(&self) -> Vec<String> {
        self.fields
            .iter()
            .map(|field| field.value.clone())
            .collect()
    }

    /// Sets the value of the field `key` (after its picker, say); a mixed
    /// field is no longer mixed.
    pub fn set_value(&mut self, key: &str, value: &str) {
        if let Some(field) = self.field_mut(key) {
            field.value = value.to_string();
            field.is_mixed = false;
        }
    }

    /// Sets the side information of the field `key`; `is_warning` draws it
    /// in the error colour.
    pub fn set_side_info(&mut self, key: &str, info: &str, is_warning: bool) {
        if let Some(field) = self.field_mut(key) {
            field.side_info = info.to_string();
            field.is_warning = is_warning;
        }
    }

    /// Sets the value style of the field `key`.
    pub fn set_value_style(&mut self, key: &str, style: Style) {
        if let Some(field) = self.field_mut(key) {
            field.value_style = style;
        }
    }

    /// Sets the suggestions of the field `key`.
    pub fn set_suggestions(&mut self, key: &str, suggestions: Vec<String>) {
        if let Some(field) = self.field_mut(key) {
            field.suggestions = suggestions;
        }
    }

    /// Locks the field `key` with `reason` (empty unlocks it).
    pub fn set_lock(&mut self, key: &str, reason: &str) {
        if let Some(field) = self.field_mut(key) {
            field.lock = reason.to_string();
        }
    }

    /// The key of the focused field.
    pub fn focus_key(&self) -> &str {
        self.fields
            .get(self.focus)
            .map_or("", |field| field.key.as_str())
    }

    /// Moves the focus to the field `key` (and ends any editing).
    pub fn focus_on(&mut self, key: &str) {
        if let Some(index) = self.index_of(key) {
            self.stop_editing();
            self.focus = index;
        }
    }

    /// Whether a field is being typed into (keys belong to it).
    pub fn is_editing(&self) -> bool {
        self.mode == Mode::Editing
    }

    /// Whether the field at `index` differs from its baseline: another
    /// value, or a value given to a field that was mixed.
    pub fn is_changed(&self, index: usize) -> bool {
        let (Some(field), Some(base)) =
            (self.fields.get(index), self.baseline.get(index))
        else {
            return false;
        };
        let was_mixed = self.baseline_mixed.get(index) == Some(&true);
        field.tracks_changes()
            && (field.is_mixed != was_mixed
                || field.comparable(&field.value) != field.comparable(base))
    }

    /// Whether any field differs from its baseline.
    pub fn is_dirty(&self) -> bool {
        (0..self.fields.len()).any(|index| self.is_changed(index))
    }

    /// Takes the current values as the baseline (once the opening values
    /// are filled in).
    pub fn mark_clean(&mut self) {
        self.baseline = self.values();
        self.baseline_mixed =
            self.fields.iter().map(|field| field.is_mixed).collect();
    }

    /// Records a successful save: the values become the baseline and the
    /// hint rows say so.
    pub fn mark_saved(&mut self) {
        self.mark_clean();
        self.has_saved = true;
        self.notice = Some("Saved".to_string());
    }

    /// Whether the form was saved at least once.
    pub fn has_saved(&self) -> bool {
        self.has_saved
    }

    /// Whether the host closes the form once the pending save succeeded (it
    /// came from the "Save changes?" question).
    pub fn closes_after_save(&self) -> bool {
        self.close_after_save
    }

    /// Reports a problem with the field `key`: it shows in red under the
    /// field, its label turns red, the focus moves there; the next key
    /// clears it. A pending close is called off.
    pub fn set_error(&mut self, key: &str, message: impl Into<String>) {
        if let Some(index) = self.index_of(key) {
            self.close_after_save = false;
            self.stop_editing();
            self.focus = index;
            self.error = Some((index, message.into()));
        }
    }

    /// Shows a short notice in the hint rows until the next key.
    pub fn set_notice(&mut self, notice: impl Into<String>) {
        self.notice = Some(notice.into());
    }

    /// Puts the field at `index` back to its baseline (the editor too).
    fn reset(&mut self, index: usize) -> bool {
        let Some(base) = self.baseline.get(index).cloned() else {
            return false;
        };
        let Some(field) = self.fields.get_mut(index) else {
            return false;
        };
        if !field.tracks_changes() {
            return false;
        }
        let was_mixed = self.baseline_mixed.get(index) == Some(&true);
        let is_changed = field.value != base || field.is_mixed != was_mixed;
        field.value.clone_from(&base);
        field.is_mixed = was_mixed;
        if let Some(editing) = self
            .editing
            .as_mut()
            .filter(|editing| editing.index == index)
        {
            editing.editor.set_text(&base);
            editing.complete.refresh(&base);
        }
        is_changed
    }

    /// Writes the editor's text into its field and stops editing; returns
    /// the edited field's key.
    fn commit_edit(&mut self) -> Option<String> {
        let editing = self.editing.take()?;
        self.mode = Mode::Normal;
        let field = self.fields.get_mut(editing.index)?;
        let value = match &editing.editor {
            Editor::Line(input) => input.value().trim().to_string(),
            Editor::Area(area) => area.text().to_string(),
        };
        // A mixed field stays mixed until something is typed into it.
        if value != field.value {
            field.is_mixed = false;
        }
        field.value = value;
        Some(field.key.clone())
    }

    /// Ends any editing, keeping the typed text.
    fn stop_editing(&mut self) {
        let _ = self.commit_edit();
        self.mode = Mode::Normal;
    }

    /// The position of the focused field among the focusable ones, and
    /// their count (for the badge).
    fn focus_position(&self) -> (usize, usize) {
        let focusable: Vec<usize> = (0..self.fields.len())
            .filter(|&index| self.fields[index].is_focusable())
            .collect();
        let position = focusable.iter().position(|&i| i == self.focus);
        (position.unwrap_or(0), focusable.len())
    }
}

impl Editor {
    fn text(&self) -> &str {
        match self {
            Editor::Line(input) => input.value(),
            Editor::Area(area) => area.text(),
        }
    }

    fn set_text(&mut self, text: &str) {
        match self {
            Editor::Line(input) => *input = InputField::new(text),
            Editor::Area(area) => area.set_text(text.to_string()),
        }
    }
}
