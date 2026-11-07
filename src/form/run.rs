//! Driving a [`Form`]: the modal loop, the keys of both modes, saving through
//! the host's callback and the sub-modals a field opens (the date calendar,
//! the external editor, "Save changes?").

use std::{io, time::Instant};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;

use super::{Field, FieldProblem, FieldState, Form, FormOutcome, Mode, field};
use crate::{
    clipboard, date_picker, editor, input,
    modal::{self, ModalSignal, SaveChoice},
    nav, overlay,
    terminal::{Tui, TuiEvent},
    theme::Skin,
};

/// What a key asks the loop to do next.
enum Step {
    Stay,
    Close(FormOutcome),
}

impl Form {
    /// Runs the form until the user closes it or quits. Each save press
    /// calls `on_save` with the fields; it persists them, or returns a
    /// [`FieldProblem`] and nothing is saved.
    ///
    /// # Errors
    ///
    /// Propagates terminal I/O errors.
    pub fn run(
        &mut self,
        tui: &mut Tui,
        skin: &Skin,
        render_bg: impl Fn(&mut Frame),
        mut on_save: impl FnMut(&[Field]) -> Result<(), FieldProblem>,
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
            let step = match tui.read_event()? {
                TuiEvent::Quit => Step::Close(FormOutcome::Quit),
                TuiEvent::Resize => Step::Stay,
                TuiEvent::Paste(text) => {
                    self.paste_into_focus(&text);
                    Step::Stay
                }
                TuiEvent::Key(key) => {
                    // A problem or a notice reflects the last key; any key
                    // dismisses it.
                    self.error = None;
                    self.notice = None;
                    self.handle_key(key, tui, skin, &render_bg, &mut on_save)?
                }
            };
            if let Step::Close(outcome) = step {
                return Ok(outcome);
            }
        }
    }

    /// The outcome of closing the form now.
    fn closed(&self) -> FormOutcome {
        if self.has_saved {
            FormOutcome::Saved
        } else {
            FormOutcome::Cancelled
        }
    }

    fn handle_key(
        &mut self,
        key: KeyEvent,
        tui: &mut Tui,
        skin: &Skin,
        render_bg: &impl Fn(&mut Frame),
        on_save: &mut impl FnMut(&[Field]) -> Result<(), FieldProblem>,
    ) -> io::Result<Step> {
        // Control *without* Alt: AltGr is reported as Ctrl+Alt and types real
        // characters, which must reach the fields.
        let ctrl = input::is_command(key);
        if ctrl && key.code == KeyCode::Char('s') {
            return self.press_save(tui, skin, render_bg, on_save);
        }
        if field::is_reset_key(key) {
            self.reset_focused();
            return Ok(Step::Stay);
        }
        if ctrl && key.code == KeyCode::Char('g') {
            self.open_editor_for_focus(tui)?;
            return Ok(Step::Stay);
        }
        match key.code {
            KeyCode::Tab => self.step_focus(1),
            KeyCode::BackTab => self.step_focus(-1),
            _ if self.mode == Mode::Editing => self.edit_key(key),
            _ => return self.normal_key(key, tui, skin, render_bg, on_save),
        }
        Ok(Step::Stay)
    }

    /// Moves the focus by `delta` fields (wrapping) and ends any editing.
    fn step_focus(&mut self, delta: isize) {
        self.mode = Mode::Normal;
        self.focus = nav::cycle(self.focus, self.fields.len(), delta);
    }

    /// A key while the focused field is being typed.
    fn edit_key(&mut self, key: KeyEvent) {
        let single_line =
            matches!(self.fields[self.focus].state, FieldState::Text { .. });
        let ends_editing = key.code == KeyCode::Esc
            || (single_line && key.code == KeyCode::Enter);
        if ends_editing {
            self.mode = Mode::Normal;
            return;
        }
        match &mut self.fields[self.focus].state {
            FieldState::Text { input, .. } => {
                input.handle_key(key);
            }
            FieldState::Multiline { area, .. } => {
                area.handle_key(key);
            }
            FieldState::Bool { .. }
            | FieldState::Choice { .. }
            | FieldState::Date { .. } => self.mode = Mode::Normal,
        }
    }

    /// A key in normal mode, where letters are commands.
    fn normal_key(
        &mut self,
        key: KeyEvent,
        tui: &mut Tui,
        skin: &Skin,
        render_bg: &impl Fn(&mut Frame),
        on_save: &mut impl FnMut(&[Field]) -> Result<(), FieldProblem>,
    ) -> io::Result<Step> {
        let bare = input::is_bare_character(key);
        match key.code {
            KeyCode::Esc => return self.leave(tui, skin, render_bg, on_save),
            KeyCode::Down => self.step_focus(1),
            KeyCode::Up => self.step_focus(-1),
            KeyCode::Char('j') if bare => self.step_focus(1),
            KeyCode::Char('k') if bare => self.step_focus(-1),
            KeyCode::Char('s') if bare => {
                return self.press_save(tui, skin, render_bg, on_save);
            }
            KeyCode::Char('y') if bare => self.copy_focused(),
            KeyCode::Enter if self.fields[self.focus].is_typed() => {
                self.mode = Mode::Editing;
            }
            KeyCode::Enter
                if matches!(
                    self.fields[self.focus].state,
                    FieldState::Date { .. }
                ) =>
            {
                return self.open_date_picker(tui, skin);
            }
            _ => self.change_focused(key),
        }
        Ok(Step::Stay)
    }

    /// The keys that change a checkbox, a choice or a date in place.
    fn change_focused(&mut self, key: KeyEvent) {
        match &mut self.fields[self.focus].state {
            FieldState::Bool { value, .. } if toggles_checkbox(key) => {
                *value = !*value;
            }
            FieldState::Choice { index, options, .. } => match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    *index = nav::cycle(*index, options.len(), -1);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    *index = nav::cycle(*index, options.len(), 1);
                }
                _ => {}
            },
            FieldState::Date { value, .. }
                if matches!(key.code, KeyCode::Delete | KeyCode::Backspace) =>
            {
                *value = None;
            }
            _ => {}
        }
    }

    /// `s` or `Ctrl+S`: saves, keeping the form open; a second press within a
    /// second of a successful save closes it.
    fn press_save(
        &mut self,
        tui: &mut Tui,
        skin: &Skin,
        render_bg: &impl Fn(&mut Frame),
        on_save: &mut impl FnMut(&[Field]) -> Result<(), FieldProblem>,
    ) -> io::Result<Step> {
        let doubled = self.save_press.register(Instant::now());
        if doubled && self.has_saved && !self.is_dirty() {
            return Ok(Step::Close(self.closed()));
        }
        self.mode = Mode::Normal;
        if self.save(tui, skin, render_bg, on_save)? {
            self.notice = Some("Saved".to_string());
        }
        Ok(Step::Stay)
    }

    /// Hands the fields to `on_save`; `true` when they were saved. A problem
    /// is shown at its field or, for the whole form, in a message box.
    fn save(
        &mut self,
        tui: &mut Tui,
        skin: &Skin,
        render_bg: &impl Fn(&mut Frame),
        on_save: &mut impl FnMut(&[Field]) -> Result<(), FieldProblem>,
    ) -> io::Result<bool> {
        match on_save(&self.fields) {
            Ok(()) => {
                self.mark_clean();
                self.has_saved = true;
                Ok(true)
            }
            Err(FieldProblem {
                field: Some(index),
                message,
            }) => {
                self.set_error(index, message);
                Ok(false)
            }
            Err(FieldProblem {
                field: None,
                message,
            }) => {
                let form: &Form = self;
                modal::message(tui, skin, "Not saved", &message, |frame| {
                    render_bg(frame);
                    form.render(frame, skin);
                })?;
                Ok(false)
            }
        }
    }

    /// `Esc` in normal mode: closes an unchanged form, asks "Save changes?"
    /// for a changed one.
    fn leave(
        &mut self,
        tui: &mut Tui,
        skin: &Skin,
        render_bg: &impl Fn(&mut Frame),
        on_save: &mut impl FnMut(&[Field]) -> Result<(), FieldProblem>,
    ) -> io::Result<Step> {
        if !self.is_dirty() {
            return Ok(Step::Close(self.closed()));
        }
        let answer = {
            let form: &Form = self;
            modal::ask_save(tui, skin, |frame| {
                render_bg(frame);
                form.render(frame, skin);
            })?
        };
        Ok(match answer {
            ModalSignal::Quit => Step::Close(FormOutcome::Quit),
            ModalSignal::Value(SaveChoice::Save) => {
                if self.save(tui, skin, render_bg, on_save)? {
                    Step::Close(FormOutcome::Saved)
                } else {
                    Step::Stay
                }
            }
            ModalSignal::Value(SaveChoice::Discard) => {
                Step::Close(self.closed())
            }
            ModalSignal::Value(SaveChoice::Back) | ModalSignal::Cancelled => {
                Step::Stay
            }
        })
    }

    /// `y`: copies the focused value and says how it went.
    fn copy_focused(&mut self) {
        let notice = match self.fields[self.focus].copy_text() {
            None => "Nothing to copy here",
            Some(text) if clipboard::copy(&text) => "Copied",
            Some(_) => "Clipboard not available",
        };
        self.notice = Some(notice.to_string());
    }

    /// Routes a pasted payload to the focused text field; other field kinds
    /// have no text to paste into and ignore it.
    fn paste_into_focus(&mut self, text: &str) {
        match &mut self.fields[self.focus].state {
            FieldState::Text { input, .. } => input.paste(text),
            FieldState::Multiline { area, .. } => area.paste(text),
            FieldState::Bool { .. }
            | FieldState::Choice { .. }
            | FieldState::Date { .. } => {}
        }
    }

    fn open_date_picker(
        &mut self,
        tui: &mut Tui,
        skin: &Skin,
    ) -> io::Result<Step> {
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
            ModalSignal::Quit => return Ok(Step::Close(FormOutcome::Quit)),
            ModalSignal::Cancelled => {}
            ModalSignal::Value(date) => {
                if let FieldState::Date { value, .. } =
                    &mut self.fields[self.focus].state
                {
                    *value = date;
                }
            }
        }
        Ok(Step::Stay)
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
}

/// Whether `key` flips a checkbox field.
///
/// Only a *bare* `Space` toggles: crossterm reports `Ctrl+Space` as
/// `Char(' ') + CONTROL`, so an unguarded match would let a host's `Ctrl+Space`
/// chord silently flip the focused checkbox as well.
pub(super) fn toggles_checkbox(key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char(' ') => input::is_bare_character(key),
        KeyCode::Enter => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::form::FieldValue;
    use crate::terminal::{
        clear_scripted_input, script_keys, scripted_remaining,
    };
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

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn form() -> Form {
        Form::new(
            "Task",
            vec![Field::text("Title", "a"), Field::checkbox("Done", false)],
        )
    }

    /// Runs `form` on `keys`; returns the outcome and every saved title.
    fn run(form: &mut Form, keys: Vec<KeyEvent>) -> (FormOutcome, Vec<String>) {
        script_keys(keys);
        let saves = RefCell::new(Vec::new());
        let mut tui = Tui::for_test(70, 20).expect("a test terminal");
        let outcome = form
            .run(
                &mut tui,
                &skin(),
                |_| {},
                |fields| {
                    if let FieldValue::Text(title) = fields[0].value() {
                        saves.borrow_mut().push(title);
                    }
                    Ok(())
                },
            )
            .expect("the form runs");
        assert_eq!(scripted_remaining(), Some(0), "every scripted key is read");
        clear_scripted_input();
        (outcome, saves.into_inner())
    }

    #[test]
    fn letters_are_commands_until_enter_starts_editing() {
        let mut form = form();
        let keys = vec![
            key(KeyCode::Char('x')),
            key(KeyCode::Enter),
            key(KeyCode::Char('b')),
            key(KeyCode::Enter),
            ctrl('s'),
            ctrl('s'),
        ];
        let (outcome, saves) = run(&mut form, keys);
        assert_eq!(outcome, FormOutcome::Saved);
        assert_eq!(saves, vec!["ab".to_string()]);
    }

    #[test]
    fn a_save_keeps_the_form_open_and_esc_then_leaves() {
        let mut form = form();
        let keys = vec![
            key(KeyCode::Char(' ')),
            key(KeyCode::Tab),
            key(KeyCode::Char(' ')),
            key(KeyCode::Char('s')),
            key(KeyCode::Esc),
        ];
        let (outcome, saves) = run(&mut form, keys);
        assert_eq!(outcome, FormOutcome::Saved);
        assert_eq!(saves.len(), 1);
        assert_eq!(form.fields()[1].value(), FieldValue::Bool(true));
    }

    #[test]
    fn esc_on_a_changed_form_asks_and_discard_leaves_unsaved() {
        let mut form = form();
        let keys = vec![
            key(KeyCode::Tab),
            key(KeyCode::Char(' ')),
            key(KeyCode::Esc),
            key(KeyCode::Char('n')),
        ];
        let (outcome, saves) = run(&mut form, keys);
        assert_eq!(outcome, FormOutcome::Cancelled);
        assert!(saves.is_empty());
    }

    #[test]
    fn esc_then_y_saves_and_leaves() {
        let mut form = form();
        let keys = vec![
            key(KeyCode::Tab),
            key(KeyCode::Char(' ')),
            key(KeyCode::Esc),
            key(KeyCode::Char('y')),
        ];
        let (outcome, saves) = run(&mut form, keys);
        assert_eq!(outcome, FormOutcome::Saved);
        assert_eq!(saves.len(), 1);
    }

    #[test]
    fn ctrl_o_resets_the_focused_field_while_editing() {
        let mut form = form();
        let keys = vec![
            key(KeyCode::Enter),
            key(KeyCode::Char('z')),
            ctrl('o'),
            key(KeyCode::Esc),
            key(KeyCode::Esc),
        ];
        let (outcome, _) = run(&mut form, keys);
        assert_eq!(outcome, FormOutcome::Cancelled);
        assert_eq!(form.fields()[0].value(), FieldValue::Text("a".into()));
    }

    #[test]
    fn a_field_problem_keeps_the_form_open_at_that_field() {
        let mut form = form();
        script_keys(vec![key(KeyCode::Char('s')), ctrl('q')]);
        let mut tui = Tui::for_test(70, 20).expect("a test terminal");
        let outcome = form
            .run(
                &mut tui,
                &skin(),
                |_| {},
                |_| Err(FieldProblem::at(1, "Not yet.")),
            )
            .expect("the form runs");
        clear_scripted_input();
        assert_eq!(outcome, FormOutcome::Quit);
        assert_eq!(form.focus, 1);
        assert_eq!(form.error_for(1), Some("Not yet."));
    }
}
