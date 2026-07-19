//! Driving a [`Form`]: the modal loop, key dispatch and the sub-pickers a
//! field opens (the date calendar and the external editor).

use std::io;

use crossterm::event::KeyCode;
use ratatui::Frame;

use super::{FieldState, Form, FormOutcome};
use crate::{
    date_picker, editor, input,
    modal::ModalSignal,
    nav, overlay,
    terminal::{Tui, TuiEvent},
    theme::Skin,
};

impl Form {
    /// Runs the form until the user saves, cancels or quits.
    ///
    /// # Errors
    ///
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
                TuiEvent::Paste(text) => self.paste_into_focus(&text),
                TuiEvent::Key(key) => {
                    // Control *without* Alt: AltGr is reported as Ctrl+Alt and
                    // types real characters, which must reach the fields.
                    let ctrl = input::is_command(key);
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

    fn handle_field_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        tui: &mut Tui,
        skin: &Skin,
    ) -> io::Result<Option<FormOutcome>> {
        // Control *without* Alt: AltGr is reported as Ctrl+Alt and types real
        // characters, which must reach the fields rather than open `$EDITOR`.
        let ctrl = input::is_command(key);

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
                if toggles_checkbox(key) {
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
}

/// Whether `key` flips a checkbox field.
///
/// Only a *bare* `Space` toggles: crossterm reports `Ctrl+Space` as
/// `Char(' ') + CONTROL`, so an unguarded match would let a host's `Ctrl+Space`
/// chord silently flip the focused checkbox as well.
pub(super) fn toggles_checkbox(key: crossterm::event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Char(' ') => input::is_bare_character(key),
        KeyCode::Enter => true,
        _ => false,
    }
}
