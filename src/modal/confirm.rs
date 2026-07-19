//! Yes/no confirmation modals, including the declining-by-default variant
//! that destructive actions use.

use std::io;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use super::ModalSignal;
use super::render::{hint_block, hinted_box_height};
use crate::{
    input::{self},
    layout::{centered_rect, fit},
    overlay::{self, PopupFlow, popup},
    terminal::Tui,
    theme::Skin,
};

/// Asks a yes/no question. `Enter`/`y` confirm, `Esc`/`n` decline.
///
/// For a destructive prompt reach for [`confirm_default`] with
/// [`Question::declining`], which makes `Enter` decline instead.
pub fn confirm(
    tui: &mut Tui,
    skin: &Skin,
    prompt: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<bool>> {
    confirm_default(tui, skin, &Question::new(prompt), render_bg)
}

/// A yes/no question and which way a bare `Enter` answers it.
#[derive(Debug, Clone, Copy)]
pub struct Question<'a> {
    /// The question shown in the dialog.
    pub prompt: &'a str,
    /// What `Enter` answers. `y`/`n` always answer explicitly, `Esc` declines.
    pub default_yes: bool,
}

impl<'a> Question<'a> {
    /// A question a bare `Enter` confirms.
    #[must_use]
    pub fn new(prompt: &'a str) -> Self {
        Self {
            prompt,
            default_yes: true,
        }
    }

    /// A question a bare `Enter` declines - the safe default for a destructive
    /// action, where an absent-minded `Enter` must not delete anything.
    #[must_use]
    pub fn declining(prompt: &'a str) -> Self {
        Self {
            prompt,
            default_yes: false,
        }
    }

    /// The footer hints, binding `enter` to whichever answer it gives.
    pub(super) fn hints(&self) -> [(&'static str, &'static str); 2] {
        if self.default_yes {
            [("enter/y", "yes"), ("n", "no")]
        } else {
            [("y", "yes"), ("enter/n", "no")]
        }
    }
}

/// Asks a yes/no question whose `Enter` answer the caller chooses.
///
/// `y` confirms and `n` declines regardless; `Esc` always declines. Use
/// [`Question::declining`] for a destructive prompt so `Enter` cannot confirm
/// it by accident, and [`confirm`] when `Enter` should mean yes.
pub fn confirm_default(
    tui: &mut Tui,
    skin: &Skin,
    question: &Question<'_>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<bool>> {
    let prompt = question.prompt;
    let default_yes = question.default_yes;
    let mut state = ();
    popup(
        tui,
        &mut state,
        |area, (): &()| {
            let width = fit(prompt.width() as u16 + 6, 28, area.width);
            centered_rect(width, hinted_box_height(), area)
        },
        |frame, (): &()| render_bg(frame),
        |frame, rect, (): &()| render_confirm(frame, skin, question, rect),
        |(): &mut (), key| confirm_key(key, default_yes),
    )
}

/// The answer `key` gives a yes/no question, or `Continue` for anything else.
///
/// Only a *bare* `y`/`n` answers: a modified one is a chord, not an answer, and
/// letting `Ctrl+Y` or `AltGr+Y` through would silently confirm - defeating the
/// whole point of [`Question::declining`] on a destructive prompt.
pub(super) fn confirm_key(key: KeyEvent, default_yes: bool) -> PopupFlow<bool> {
    match key.code {
        KeyCode::Char('y' | 'Y') if input::is_bare_character(key) => {
            PopupFlow::Done(true)
        }
        KeyCode::Char('n' | 'N') if input::is_bare_character(key) => {
            PopupFlow::Done(false)
        }
        KeyCode::Esc => PopupFlow::Done(false),
        KeyCode::Enter => PopupFlow::Done(default_yes),
        _ => PopupFlow::Continue,
    }
}

fn render_confirm(
    frame: &mut Frame,
    skin: &Skin,
    question: &Question<'_>,
    rect: Rect,
) {
    let inner = overlay::framed(frame, rect, skin, " Confirm ");
    let width = inner.width as usize;
    let mut lines = vec![Line::from(question.prompt.to_string())];
    lines.extend(hint_block(&question.hints(), &skin.palette, width));
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}
