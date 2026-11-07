//! Driving a [`Form`]: the blocking loop over its [`FormState`], the host
//! that draws behind it and saves it, and the sub-modals a field opens (the
//! date calendar, the external editor).
//!
//! [`FormState`]: super::FormState

use std::io;

use chrono::NaiveDate;
use ratatui::Frame;

use super::{
    DATE_FORMAT, Field, FieldProblem, Form, FormEvent, FormOutcome, Kind,
};
use crate::{
    date_picker, editor,
    modal::{self, ModalSignal},
    overlay,
    terminal::{Tui, TuiEvent},
    theme::Skin,
};

/// What a [`Form`] runs over: the view drawn (dimmed) behind it and the
/// place its values are saved to.
///
/// One trait rather than two closures, because both usually reach the same
/// app: the background reads it, the save writes to it.
pub trait FormHost {
    /// Draws the view behind the form.
    fn render_bg(&self, frame: &mut Frame);

    /// Persists the fields of a save press, or returns the
    /// [`FieldProblem`] that stops it (nothing is saved then).
    ///
    /// # Errors
    ///
    /// Returns the problem the form shows.
    fn save(&mut self, fields: &[Field]) -> Result<(), FieldProblem>;
}

impl Form {
    /// Runs the form over `host` until the user closes it or quits. Each
    /// save press hands the fields to [`FormHost::save`].
    ///
    /// # Errors
    ///
    /// Propagates terminal I/O errors.
    pub fn run(
        &mut self,
        tui: &mut Tui,
        skin: &Skin,
        host: &mut impl FormHost,
    ) -> io::Result<FormOutcome> {
        if self.state.fields().is_empty() {
            return Ok(FormOutcome::Cancelled);
        }
        loop {
            tui.draw(|frame| self.draw(frame, skin, &*host))?;
            let event = match tui.read_event()? {
                TuiEvent::Quit => return Ok(FormOutcome::Quit),
                TuiEvent::Resize => continue,
                TuiEvent::Paste(text) => {
                    self.state.paste(&text);
                    continue;
                }
                TuiEvent::Key(key) => self.state.handle_key(key),
            };
            let outcome = match event {
                FormEvent::Pending | FormEvent::Changed(_) => None,
                FormEvent::Activate(key) => {
                    self.open_picker(tui, skin, &*host, &key)?
                }
                FormEvent::ExternalEditor(key) => {
                    self.open_editor(tui, &key)?;
                    None
                }
                FormEvent::Save => self.save(tui, skin, host)?,
                FormEvent::Close => Some(FormOutcome::Saved),
                FormEvent::Discard => Some(self.closed()),
            };
            if let Some(outcome) = outcome {
                return Ok(outcome);
            }
        }
    }

    /// The outcome of closing the form now.
    fn closed(&self) -> FormOutcome {
        if self.state.has_saved() {
            FormOutcome::Saved
        } else {
            FormOutcome::Cancelled
        }
    }

    /// The view behind, dimmed, and the form over it.
    fn draw(&self, frame: &mut Frame, skin: &Skin, host: &impl FormHost) {
        host.render_bg(frame);
        overlay::dim(frame, overlay::SCRIM_FACTOR);
        self.state.render(frame, frame.area(), skin);
    }

    /// Hands the fields to the host: a success becomes the new baseline
    /// (and closes the form after its "Save changes?" question), a problem
    /// shows at its field or in a message box.
    fn save(
        &mut self,
        tui: &mut Tui,
        skin: &Skin,
        host: &mut impl FormHost,
    ) -> io::Result<Option<FormOutcome>> {
        match host.save(&self.fields()) {
            Ok(()) => {
                self.state.mark_saved();
                Ok(self.state.closes_after_save().then_some(FormOutcome::Saved))
            }
            Err(FieldProblem {
                field: Some(index),
                message,
            }) => {
                self.state.set_error(&index.to_string(), message);
                Ok(None)
            }
            Err(FieldProblem {
                field: None,
                message,
            }) => {
                let (form, host): (&Form, &_) = (self, host);
                let signal = modal::message(
                    tui,
                    skin,
                    "Not saved",
                    &message,
                    |frame| {
                        form.draw(frame, skin, host);
                    },
                )?;
                Ok(matches!(signal, ModalSignal::Quit)
                    .then_some(FormOutcome::Quit))
            }
        }
    }

    /// Opens the calendar of the date field `key`.
    fn open_picker(
        &mut self,
        tui: &mut Tui,
        skin: &Skin,
        host: &impl FormHost,
        key: &str,
    ) -> io::Result<Option<FormOutcome>> {
        let is_date = key
            .parse::<usize>()
            .ok()
            .and_then(|index| self.kinds.get(index))
            .is_some_and(|kind| *kind == Kind::Date);
        if !is_date {
            return Ok(None);
        }
        let current =
            NaiveDate::parse_from_str(self.state.value(key), DATE_FORMAT).ok();
        let signal = {
            let form: &Form = self;
            date_picker::date_picker(tui, skin, " Date ", current, true, |f| {
                form.draw(f, skin, host);
            })?
        };
        match signal {
            ModalSignal::Quit => return Ok(Some(FormOutcome::Quit)),
            ModalSignal::Cancelled => {}
            ModalSignal::Value(date) => {
                let value = date
                    .map(|date| date.format(DATE_FORMAT).to_string())
                    .unwrap_or_default();
                self.state.set_value(key, &value);
            }
        }
        Ok(None)
    }

    /// Hands the multi-line field `key` to `$EDITOR`.
    fn open_editor(&mut self, tui: &mut Tui, key: &str) -> io::Result<()> {
        let initial = self.state.value(key).to_string();
        let command = editor::resolve_editor();
        if let Some(text) = editor::edit_in_editor(tui, &command, &initial)? {
            self.state.set_value(key, &text);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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

    /// A host over nothing that records every saved title, or refuses
    /// each save with `problem`.
    #[derive(Default)]
    struct Host {
        saves: Vec<String>,
        problem: Option<FieldProblem>,
    }

    impl FormHost for Host {
        fn render_bg(&self, _frame: &mut Frame) {}

        fn save(&mut self, fields: &[Field]) -> Result<(), FieldProblem> {
            if let Some(problem) = &self.problem {
                return Err(problem.clone());
            }
            if let FieldValue::Text(title) = fields[0].value() {
                self.saves.push(title);
            }
            Ok(())
        }
    }

    /// Runs `form` on `keys`; returns the outcome and every saved title.
    fn run(form: &mut Form, keys: Vec<KeyEvent>) -> (FormOutcome, Vec<String>) {
        script_keys(keys);
        let mut host = Host::default();
        let mut tui = Tui::for_test(70, 20).expect("a test terminal");
        let outcome = form
            .run(&mut tui, &skin(), &mut host)
            .expect("the form runs");
        assert_eq!(scripted_remaining(), Some(0), "every scripted key is read");
        clear_scripted_input();
        (outcome, host.saves)
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
    fn esc_on_a_changed_form_asks_and_n_discards() {
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
        let mut host = Host {
            problem: Some(FieldProblem::at(1, "Not yet.")),
            ..Host::default()
        };
        let outcome = form
            .run(&mut tui, &skin(), &mut host)
            .expect("the form runs");
        clear_scripted_input();
        assert_eq!(outcome, FormOutcome::Quit);
        assert_eq!(form.state().focus_key(), "1");
    }
}
