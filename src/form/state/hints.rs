//! The hint rows of a [`FormState`]: a spacer, the focused field's keys (or
//! a notice, or the "Save changes?" question) and the form's keys. Each part
//! wraps on a narrow screen and keeps as many rows as its widest variant
//! needs at that width, so the box keeps its height while the focus moves.
//! A form drawn inline hands the same keys to the app's footer instead
//! ([`FormState::hint_groups`], [`FormState::prompt`]).

use ratatui::text::Line;

use super::{FieldKind, FormState, Mode};
use crate::{
    modal::{MAX_HINT_ROWS, hints_width},
    shortcut_hints, style, text,
    theme::{Color, Skin},
};

/// The question a changed form asks when it is left.
const QUESTION: &str = "Save changes?";

const NORMAL_HINTS: [(&str, &str); 5] = [
    ("Tab", "next"),
    ("s", "save"),
    ("y", "copy"),
    ("Ctrl+O", "reset"),
    ("Esc", "leave (asks to save)"),
];
const EDITING_HINTS: [(&str, &str); 3] =
    [("Tab", "next"), ("Ctrl+S", "save"), ("Ctrl+O", "reset")];
const QUESTION_HINTS: [(&str, &str); 3] =
    [("y", "save"), ("n", "discard"), ("b", "back")];
const TEXT_HINTS: [(&str, &str); 1] = [("Enter", "edit")];
const LINE_EDITING_HINTS: [(&str, &str); 1] = [("Enter/Esc", "done")];
const AREA_HINTS: [(&str, &str); 2] = [("Enter", "edit"), ("Ctrl+G", "editor")];
const AREA_EDITING_HINTS: [(&str, &str); 2] =
    [("Esc", "done"), ("Ctrl+G", "editor")];
const PICKER_HINTS: [(&str, &str); 1] = [("Enter", "choose")];
const CLEARABLE_HINTS: [(&str, &str); 2] =
    [("Enter", "choose"), ("Del", "clear")];
const TEXT_PICKER_HINTS: [(&str, &str); 2] =
    [("Enter", "choose"), ("e", "type")];
const TOGGLE_HINTS: [(&str, &str); 1] = [("Space", "toggle")];
const STEPPER_HINTS: [(&str, &str); 1] = [("\u{2190}\u{2192}", "change")];
const NO_HINTS: [(&str, &str); 0] = [];

/// The keys the form row can show.
const FORM_HINTS: [&[(&str, &str)]; 3] =
    [&NORMAL_HINTS, &EDITING_HINTS, &QUESTION_HINTS];

/// The keys the field row can show.
const FIELD_HINTS: [&[(&str, &str)]; 9] = [
    &TEXT_HINTS,
    &LINE_EDITING_HINTS,
    &AREA_HINTS,
    &AREA_EDITING_HINTS,
    &PICKER_HINTS,
    &CLEARABLE_HINTS,
    &TEXT_PICKER_HINTS,
    &TOGGLE_HINTS,
    &STEPPER_HINTS,
];

/// Every hint row the form can show, for its minimum width.
const ALL_HINTS: [&[(&str, &str)]; 12] = [
    &NORMAL_HINTS,
    &EDITING_HINTS,
    &QUESTION_HINTS,
    &TEXT_HINTS,
    &LINE_EDITING_HINTS,
    &AREA_HINTS,
    &AREA_EDITING_HINTS,
    &PICKER_HINTS,
    &CLEARABLE_HINTS,
    &TEXT_PICKER_HINTS,
    &TOGGLE_HINTS,
    &STEPPER_HINTS,
];

/// The widest hint row the form can show: the box is never narrower.
pub(super) fn min_width() -> usize {
    ALL_HINTS
        .iter()
        .map(|row| hints_width(row))
        .max()
        .unwrap_or(0)
}

/// The rows the hint block of a modal form takes at `width` columns: the
/// spacer, the rows the widest field keys wrap onto and the rows the widest
/// form keys wrap onto (one each at a normal width).
pub fn hint_rows(width: usize) -> u16 {
    let rows =
        1 + most_rows(&FIELD_HINTS, width) + most_rows(&FORM_HINTS, width);
    u16::try_from(rows).unwrap_or(u16::MAX)
}

/// The most rows any of `sets` wraps onto at `width`, at least one.
fn most_rows(sets: &[&[(&str, &str)]], width: usize) -> usize {
    sets.iter()
        .map(|hints| shortcut_hints::lines(hints, Color::Default, width).len())
        .max()
        .unwrap_or(0)
        .clamp(1, MAX_HINT_ROWS)
}

/// `lines` cut or padded to `rows`.
fn fit_rows(mut lines: Vec<Line<'static>>, rows: usize) -> Vec<Line<'static>> {
    lines.resize(rows, Line::default());
    lines
}

impl FormState {
    /// The hint block of the modal form: a spacer, the focused field's keys
    /// (or a notice, or the "Save changes?" question), the form's keys, each
    /// wrapped at `width` into the rows [`hint_rows`] reserves.
    pub fn hint_lines(&self, width: usize, skin: &Skin) -> Vec<Line<'static>> {
        let accent = style::accent(&skin.palette);
        let field = match self.prompt() {
            Some(prompt) => text::wrap(prompt, width)
                .into_iter()
                .map(|row| Line::styled(row, accent))
                .collect(),
            None => key_rows(self.field_hints(), skin, width),
        };
        let mut lines = vec![Line::raw("")];
        lines.extend(fit_rows(field, most_rows(&FIELD_HINTS, width)));
        lines.extend(fit_rows(
            key_rows(self.form_hints(), skin, width),
            most_rows(&FORM_HINTS, width),
        ));
        lines
    }

    /// The keys of a form drawn inline, for the app's footer: the focused
    /// field's keys (when it has any) and the form's, labelled `Field` and
    /// `Form`; while "Save changes?" is asked, only its answers.
    pub fn hint_groups(
        &self,
    ) -> Vec<(&'static str, &'static [(&'static str, &'static str)])> {
        if self.asking_save {
            return vec![("Form", &QUESTION_HINTS)];
        }
        let field = self.field_hints();
        let mut groups = Vec::with_capacity(2);
        if !field.is_empty() {
            groups.push(("Field", field));
        }
        groups.push(("Form", self.form_hints()));
        groups
    }

    /// What a form drawn inline asks the app's status band to show: the
    /// "Save changes?" question, or a notice such as "Saved".
    pub fn prompt(&self) -> Option<&str> {
        if self.asking_save {
            return Some(QUESTION);
        }
        self.notice.as_deref()
    }

    /// The form's keys in the current mode.
    fn form_hints(&self) -> &'static [(&'static str, &'static str)] {
        if self.asking_save {
            return &QUESTION_HINTS;
        }
        match self.mode {
            Mode::Normal => &NORMAL_HINTS,
            Mode::Editing => &EDITING_HINTS,
        }
    }

    /// The keys of the focused field in the current mode.
    fn field_hints(&self) -> &'static [(&'static str, &'static str)] {
        let Some(field) = self.fields.get(self.focus) else {
            return &NO_HINTS;
        };
        if !field.lock.is_empty() {
            return &NO_HINTS;
        }
        let editing = self.mode == Mode::Editing;
        match &field.kind {
            FieldKind::Multiline if editing => &AREA_EDITING_HINTS,
            FieldKind::Multiline => &AREA_HINTS,
            FieldKind::Text | FieldKind::TextPicker if editing => {
                &LINE_EDITING_HINTS
            }
            FieldKind::Text | FieldKind::Owned => &TEXT_HINTS,
            FieldKind::TextPicker => &TEXT_PICKER_HINTS,
            FieldKind::Choice(_) | FieldKind::Picker if field.is_clearable => {
                &CLEARABLE_HINTS
            }
            FieldKind::Choice(_) | FieldKind::Picker => &PICKER_HINTS,
            FieldKind::Toggle => &TOGGLE_HINTS,
            FieldKind::Stepper(_) => &STEPPER_HINTS,
            FieldKind::ReadOnly | FieldKind::Section => &NO_HINTS,
        }
    }
}

/// `hints` wrapped at `width` like the toolkit's modal keys, at most
/// [`MAX_HINT_ROWS`] rows.
fn key_rows(
    hints: &[(&str, &str)],
    skin: &Skin,
    width: usize,
) -> Vec<Line<'static>> {
    shortcut_hints::lines(hints, skin.palette.accent, width)
        .into_iter()
        .take(MAX_HINT_ROWS)
        .collect()
}
