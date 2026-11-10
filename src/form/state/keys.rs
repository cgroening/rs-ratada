//! The keys of a [`FormState`]: both modes, saving, the "Save changes?"
//! question, and the blocking helper that picks a choice.

use std::{io, time::Instant};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;

use super::{
    Editing, Editor, FieldKind, FormEvent, FormState, Mode, is_yes, yes_no,
};
use crate::{
    autocomplete::{AcOutcome, Autocomplete, Placement},
    clipboard,
    form::field,
    input::{self, InputField},
    modal::{self, ModalSignal, SelectItem, SelectOptions},
    nav,
    terminal::Tui,
    textarea::TextArea,
    theme::Skin,
};

/// Most suggestion rows a form field shows at once. The menu may run past
/// the modal down to the screen's edge, so it can be longer than the
/// toolkit's default.
const SUGGESTION_ROWS: usize = 15;

impl FormState {
    /// Handles one key and says what the host has to do.
    pub fn handle_key(&mut self, key: KeyEvent) -> FormEvent {
        // A problem or a notice reflects the last key; any key clears it.
        self.error = None;
        self.notice = None;
        if self.asking_save {
            return self.answer_save_question(key);
        }
        // Control *without* Alt: AltGr is reported as Ctrl+Alt and types
        // real characters, which must reach the fields.
        let ctrl = input::is_command(key);
        if ctrl && key.code == KeyCode::Char('s') {
            return self.press_save();
        }
        if field::is_reset_key(key) {
            let index = self.focus;
            return if self.reset(index) {
                self.changed_event(index)
            } else {
                FormEvent::Pending
            };
        }
        if ctrl && key.code == KeyCode::Char('g') {
            return self.external_editor();
        }
        match self.mode {
            Mode::Editing => self.edit_key(key),
            Mode::Normal => self.normal_key(key),
        }
    }

    /// Puts pasted text into the field being typed into. Outside an edit a
    /// paste on a field that is typed into (text, multi-line, text picker,
    /// not locked) starts editing it first; anywhere else it is dropped.
    pub fn paste(&mut self, text: &str) {
        if self.editing.is_none() && !self.asking_save {
            let is_typed = self.fields.get(self.focus).is_some_and(|field| {
                field.lock.is_empty()
                    && matches!(
                        field.kind,
                        FieldKind::Text
                            | FieldKind::Multiline
                            | FieldKind::TextPicker
                    )
            });
            if is_typed {
                let _ = self.start_edit();
            }
        }
        let Some(editing) = self.editing.as_mut() else {
            return;
        };
        match &mut editing.editor {
            Editor::Line(input) => input.paste(text),
            Editor::Area(area) => area.paste(text),
        }
        let typed = editing.editor.text().to_string();
        editing.complete.refresh(&typed);
    }

    /// Opens the list of the choice field `key` (the host calls it on
    /// [`FormEvent::Activate`]): the entry that applies is checked, the
    /// picked one becomes the value. Returns whether the value changed;
    /// `Err` only for terminal errors (the quit chord comes back as `None`
    /// in the outer `Option`).
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the list cannot be drawn.
    pub fn choose(
        &mut self,
        tui: &mut Tui,
        skin: &Skin,
        key: &str,
        render_bg: impl Fn(&mut Frame),
    ) -> io::Result<Option<bool>> {
        let Some(field) = self.field(key) else {
            return Ok(Some(false));
        };
        let FieldKind::Choice(options) = &field.kind else {
            return Ok(Some(false));
        };
        let options = options.clone();
        let current = options.iter().position(|o| *o == field.value);
        let items: Vec<SelectItem> =
            options.iter().map(|o| SelectItem::new(o.clone())).collect();
        let title = field.label.clone();
        let select = SelectOptions {
            current,
            ..SelectOptions::default()
        };
        let signal = modal::select_with(
            tui,
            skin,
            &title,
            &items,
            current.unwrap_or(0),
            &select,
            render_bg,
        )?;
        Ok(match signal {
            ModalSignal::Quit => None,
            ModalSignal::Cancelled => Some(false),
            ModalSignal::Value(index) => {
                let value = options.get(index).cloned().unwrap_or_default();
                let changed = self.value(key) != value;
                self.set_value(key, &value);
                Some(changed)
            }
        })
    }

    /// A key while a field is typed into: the suggestions see it first.
    fn edit_key(&mut self, key: KeyEvent) -> FormEvent {
        let Some(editing) = self.editing.as_mut() else {
            self.mode = super::Mode::Normal;
            return FormEvent::Pending;
        };
        match editing.complete.on_key(key) {
            AcOutcome::Accepted(value) => {
                editing.editor.set_text(&value);
                editing.complete.refresh(&value);
                return FormEvent::Pending;
            }
            AcOutcome::Navigated | AcOutcome::Closed => {
                return FormEvent::Pending;
            }
            AcOutcome::Ignored => {}
        }
        let is_line = matches!(editing.editor, Editor::Line(_));
        match key.code {
            KeyCode::Esc => self.end_edit(),
            KeyCode::Enter if is_line => self.end_edit(),
            KeyCode::Tab => {
                let event = self.end_edit();
                self.step_focus(1);
                event
            }
            KeyCode::BackTab => {
                let event = self.end_edit();
                self.step_focus(-1);
                event
            }
            _ => {
                let handled = match &mut editing.editor {
                    Editor::Line(input) => input.handle_key(key),
                    Editor::Area(area) => area.handle_key(key),
                };
                if handled {
                    let text = editing.editor.text().to_string();
                    editing.complete.refresh(&text);
                }
                FormEvent::Pending
            }
        }
    }

    /// Ends the editing; a changed value is reported.
    fn end_edit(&mut self) -> FormEvent {
        let before = self.fields.get(self.focus).map(|f| f.value.clone());
        match self.commit_edit() {
            Some(key) if before.as_deref() != Some(self.value(&key)) => {
                FormEvent::Changed(key)
            }
            _ => FormEvent::Pending,
        }
    }

    /// A key in normal mode, where letters are commands.
    fn normal_key(&mut self, key: KeyEvent) -> FormEvent {
        let bare = input::is_bare_character(key);
        match key.code {
            KeyCode::Esc => self.leave(),
            KeyCode::Tab | KeyCode::Down => self.step_focus(1),
            KeyCode::BackTab | KeyCode::Up => self.step_focus(-1),
            KeyCode::Char('j') if bare => self.step_focus(1),
            KeyCode::Char('k') if bare => self.step_focus(-1),
            KeyCode::Char('s') if bare => self.press_save(),
            KeyCode::Char('y') if bare => self.copy_focused(),
            KeyCode::Char('e') if bare => self.type_into_focused(),
            KeyCode::Enter => self.activate_focused(),
            _ => self.change_focused(key),
        }
    }

    /// Moves the focus by `delta` focusable fields, wrapping.
    fn step_focus(&mut self, delta: isize) -> FormEvent {
        let len = self.fields.len();
        let mut index = self.focus;
        for _ in 0..len {
            index = nav::cycle(index, len, delta);
            if self.fields[index].is_focusable() {
                break;
            }
        }
        self.focus = index;
        FormEvent::Pending
    }

    /// `Enter` in normal mode: edit, toggle, step or ask for the picker.
    fn activate_focused(&mut self) -> FormEvent {
        let Some(field) = self.fields.get(self.focus) else {
            return FormEvent::Pending;
        };
        if !field.lock.is_empty() {
            return FormEvent::Pending;
        }
        match &field.kind {
            FieldKind::Text | FieldKind::Multiline => self.start_edit(),
            FieldKind::Toggle => self.flip_focused(),
            FieldKind::Stepper(_) => self.step_value(1),
            FieldKind::Choice(_)
            | FieldKind::Picker
            | FieldKind::TextPicker
            | FieldKind::Owned => FormEvent::Activate(field.key.clone()),
            FieldKind::ReadOnly | FieldKind::Section => FormEvent::Pending,
        }
    }

    /// `e`: types into a text field or a text picker.
    fn type_into_focused(&mut self) -> FormEvent {
        let typed = self.fields.get(self.focus).is_some_and(|field| {
            field.lock.is_empty()
                && matches!(
                    field.kind,
                    FieldKind::Text
                        | FieldKind::Multiline
                        | FieldKind::TextPicker
                )
        });
        if typed {
            self.start_edit()
        } else {
            FormEvent::Pending
        }
    }

    /// Starts typing into the focused field, seeded with its value.
    fn start_edit(&mut self) -> FormEvent {
        let Some(field) = self.fields.get(self.focus) else {
            return FormEvent::Pending;
        };
        let editor = match (field.kind == FieldKind::Multiline, field.max_chars)
        {
            (true, Some(max)) => {
                Editor::Area(TextArea::new(&field.value).max_len(max))
            }
            (true, None) => Editor::Area(TextArea::new(&field.value)),
            (false, Some(max)) => {
                Editor::Line(InputField::new(&field.value).max_len(max))
            }
            (false, None) => Editor::Line(InputField::new(&field.value)),
        };
        let mut complete = Autocomplete::new(field.suggestions.clone())
            .open_on_empty()
            .max_visible(SUGGESTION_ROWS)
            .placement(Placement::Below);
        complete.refresh(&field.value);
        self.editing = Some(Editing {
            index: self.focus,
            editor,
            complete,
        });
        self.mode = Mode::Editing;
        FormEvent::Pending
    }

    /// The keys that change a toggle, a stepper or a picker in place.
    fn change_focused(&mut self, key: KeyEvent) -> FormEvent {
        let Some(field) = self.fields.get(self.focus) else {
            return FormEvent::Pending;
        };
        if !field.lock.is_empty() {
            return FormEvent::Pending;
        }
        let bare = input::is_bare_character(key);
        match (&field.kind, key.code) {
            (FieldKind::Toggle, KeyCode::Char(' ')) if bare => {
                self.flip_focused()
            }
            (FieldKind::Stepper(_), KeyCode::Left) => self.step_value(-1),
            (FieldKind::Stepper(_), KeyCode::Right) => self.step_value(1),
            (FieldKind::Stepper(_), KeyCode::Char('h')) if bare => {
                self.step_value(-1)
            }
            (FieldKind::Stepper(_), KeyCode::Char('l')) if bare => {
                self.step_value(1)
            }
            (_, KeyCode::Delete | KeyCode::Backspace)
                if field.is_clearable && field.is_picker() =>
            {
                let key = field.key.clone();
                self.set_value(&key, "");
                FormEvent::Changed(key)
            }
            _ => FormEvent::Pending,
        }
    }

    /// Flips the focused toggle; a mixed toggle turns on first.
    fn flip_focused(&mut self) -> FormEvent {
        let index = self.focus;
        let field = &mut self.fields[index];
        let on = field.is_mixed || !is_yes(&field.value);
        field.value = yes_no(on).to_string();
        field.is_mixed = false;
        self.changed_event(index)
    }

    /// Steps the focused stepper by `delta` options, wrapping. A value that is
    /// none of the options (one the host set, say "Custom") steps forward to
    /// the first option and back to the last.
    fn step_value(&mut self, delta: isize) -> FormEvent {
        let index = self.focus;
        let field = &mut self.fields[index];
        let FieldKind::Stepper(options) = &field.kind else {
            return FormEvent::Pending;
        };
        let current = options.iter().position(|o| *o == field.value);
        let next = match current {
            Some(current) => nav::cycle(current, options.len(), delta),
            None if delta < 0 => options.len().saturating_sub(1),
            None => 0,
        };
        if let Some(option) = options.get(next) {
            field.value.clone_from(option);
            field.is_mixed = false;
        }
        self.changed_event(index)
    }

    /// `y`: copies the focused value and says how it went.
    fn copy_focused(&mut self) -> FormEvent {
        let value = self
            .fields
            .get(self.focus)
            .filter(|field| field.kind != FieldKind::Section)
            .map(|field| field.value.clone())
            .filter(|value| !value.is_empty());
        let notice = match value {
            None => "Nothing to copy here",
            Some(value) if clipboard::copy(&value) => "Copied",
            Some(_) => "Clipboard not available",
        };
        self.notice = Some(notice.to_string());
        FormEvent::Pending
    }

    /// `Ctrl+G` on a multi-line field: the host opens the external editor.
    fn external_editor(&mut self) -> FormEvent {
        let Some(field) = self.fields.get(self.focus) else {
            return FormEvent::Pending;
        };
        if field.kind != FieldKind::Multiline || !field.lock.is_empty() {
            return FormEvent::Pending;
        }
        let key = field.key.clone();
        self.stop_editing();
        FormEvent::ExternalEditor(key)
    }

    /// `s`/`Ctrl+S`: asks the host to save, keeping the form open; a second
    /// press within a second of a save that left nothing changed closes it.
    fn press_save(&mut self) -> FormEvent {
        let doubled = self.save_press.register(Instant::now());
        if doubled && self.has_saved && !self.is_dirty() {
            return FormEvent::Close;
        }
        self.stop_editing();
        FormEvent::Save
    }

    /// `Esc` in normal mode: closes an unchanged form, asks for a changed
    /// one.
    fn leave(&mut self) -> FormEvent {
        if self.is_dirty() {
            self.asking_save = true;
            return FormEvent::Pending;
        }
        if self.has_saved {
            FormEvent::Close
        } else {
            FormEvent::Discard
        }
    }

    /// The answer to "Save changes?": `y` saves and closes once the save
    /// succeeds, `n` discards, `b`/`Esc` go back to the form.
    fn answer_save_question(&mut self, key: KeyEvent) -> FormEvent {
        match key.code {
            KeyCode::Char('y' | 'Y') => {
                self.asking_save = false;
                self.close_after_save = true;
                FormEvent::Save
            }
            KeyCode::Char('n' | 'N') => FormEvent::Discard,
            KeyCode::Char('b' | 'B') | KeyCode::Esc => {
                self.asking_save = false;
                FormEvent::Pending
            }
            _ => FormEvent::Pending,
        }
    }

    /// The event for a changed value of the field at `index`.
    fn changed_event(&self, index: usize) -> FormEvent {
        self.fields
            .get(index)
            .map_or(FormEvent::Pending, |f| FormEvent::Changed(f.key.clone()))
    }
}
