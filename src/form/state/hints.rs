//! The hint rows of a [`FormState`]: a spacer, the focused field's keys (or
//! a notice, or the "Save changes?" question) and the form's keys, so the box
//! keeps its height while the focus moves.

use ratatui::text::Line;

use super::{FieldKind, FormState, Mode};
use crate::{
    modal::{hint_block, hints_width},
    style,
    theme::Skin,
};

const NORMAL_HINTS: [(&str, &str); 5] = [
    ("tab", "next"),
    ("s", "save"),
    ("y", "copy"),
    ("ctrl+o", "reset"),
    ("esc", "leave (asks to save)"),
];
const EDITING_HINTS: [(&str, &str); 3] =
    [("tab", "next"), ("ctrl+s", "save"), ("ctrl+o", "reset")];
const QUESTION_HINTS: [(&str, &str); 3] =
    [("y", "save"), ("n", "discard"), ("b", "back")];
const TEXT_HINTS: [(&str, &str); 1] = [("enter", "edit")];
const LINE_EDITING_HINTS: [(&str, &str); 1] = [("enter/esc", "done")];
const AREA_HINTS: [(&str, &str); 2] = [("enter", "edit"), ("ctrl+g", "editor")];
const AREA_EDITING_HINTS: [(&str, &str); 2] =
    [("esc", "done"), ("ctrl+g", "editor")];
const PICKER_HINTS: [(&str, &str); 1] = [("enter", "choose")];
const CLEARABLE_HINTS: [(&str, &str); 2] =
    [("enter", "choose"), ("del", "clear")];
const TEXT_PICKER_HINTS: [(&str, &str); 2] =
    [("enter", "choose"), ("e", "type")];
const TOGGLE_HINTS: [(&str, &str); 1] = [("space", "toggle")];
const STEPPER_HINTS: [(&str, &str); 1] = [("\u{2190}\u{2192}", "change")];
const NO_HINTS: [(&str, &str); 0] = [];

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

impl FormState {
    /// The hint block: a spacer, the focused field's keys (or a notice, or
    /// the "Save changes?" question), the form's keys.
    pub fn hint_lines(&self, width: usize, skin: &Skin) -> Vec<Line<'static>> {
        let accent = style::accent(&skin.palette);
        let (field_row, form): (Line, &[(&str, &str)]) = if self.asking_save {
            (Line::styled("Save changes?", accent), &QUESTION_HINTS)
        } else {
            let row = match &self.notice {
                Some(notice) => Line::styled(notice.clone(), accent),
                None => hint_row(self.field_hints(), skin, width),
            };
            let form: &[(&str, &str)] = match self.mode {
                Mode::Normal => &NORMAL_HINTS,
                Mode::Editing => &EDITING_HINTS,
            };
            (row, form)
        };
        vec![Line::raw(""), field_row, hint_row(form, skin, width)]
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

/// One hint row drawn like the toolkit's modal keys.
fn hint_row(
    hints: &[(&str, &str)],
    skin: &Skin,
    width: usize,
) -> Line<'static> {
    hint_block(hints, &skin.palette, width)
        .into_iter()
        .nth(1)
        .unwrap_or_default()
}
