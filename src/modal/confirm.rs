//! Yes/no confirmation modals, including the declining-by-default variant
//! that destructive actions use and the variant whose body the caller styles.

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
use super::render::{hint_block, hinted_box_height, text_extent};
use crate::{
    input::{self},
    layout::{centered_rect, fit},
    overlay::{self, PopupFlow, popup},
    terminal::Tui,
    theme::Skin,
};

/// Columns a confirm box adds around its body: the two border columns, the
/// frame's inner padding and a little air to the right of the longest line.
const BOX_CHROME: u16 = 6;

/// The width a confirm box never goes below, so a short question still reads
/// as a dialog and the hint line fits.
const MIN_BOX_WIDTH: u16 = 28;

/// Columns a frame title needs beyond its own text: the two corners and one
/// border cell on either side.
const TITLE_CHROME: u16 = 4;

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
        answer_hints(self.default_yes)
    }
}

/// A yes/no question whose frame title and body the caller supplies.
///
/// Where [`Question`] shows one plain prompt under a fixed title, this one
/// lets the frame carry the actual question and the body carry styled lines -
/// label/value rows, a coloured warning, anything a [`Line`] can express.
pub struct StyledQuestion<'a> {
    /// The frame title, usually the question itself (`" Delete? "`).
    pub title: &'a str,
    /// Builds the body for the given body width, in columns. It is called on
    /// every draw, so a body that wraps to the width follows a terminal
    /// resize; a line wider than the width is clipped.
    pub lines: &'a dyn Fn(u16) -> Vec<Line<'static>>,
    /// What `Enter` answers. `y`/`n` always answer explicitly, `Esc` declines.
    pub default_yes: bool,
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
        |area, (): &()| confirm_area(prompt, area),
        |frame, (): &()| render_bg(frame),
        |frame, rect, (): &()| render_confirm(frame, skin, question, rect),
        |(): &mut (), key| confirm_key(key, default_yes),
    )
}

/// Asks a yes/no question with a caller-styled body under its own title.
///
/// The keys are those of [`confirm_default`]: `y` confirms, `n` and `Esc`
/// decline, and `Enter` gives the answer `question.default_yes` names. The box
/// is as wide as the longest body line (or the title) and as tall as the body.
pub fn confirm_styled(
    tui: &mut Tui,
    skin: &Skin,
    question: &StyledQuestion<'_>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<bool>> {
    let default_yes = question.default_yes;
    let mut state = ();
    popup(
        tui,
        &mut state,
        |area, (): &()| styled_area(question, area),
        |frame, (): &()| render_bg(frame),
        |frame, rect, (): &()| render_styled(frame, skin, question, rect),
        |(): &mut (), key| confirm_key(key, default_yes),
    )
}

/// The footer hints of a yes/no question, binding `enter` to whichever answer
/// it gives.
fn answer_hints(default_yes: bool) -> [(&'static str, &'static str); 2] {
    if default_yes {
        [("enter/y", "yes"), ("n", "no")]
    } else {
        [("y", "yes"), ("enter/n", "no")]
    }
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

/// The box of a confirm dialog asking `prompt`: as wide as its longest line
/// and one row taller per further line, so a multi-line prompt keeps its line
/// breaks and the hint block stays in view.
pub(super) fn confirm_area(prompt: &str, area: Rect) -> Rect {
    let (widest, rows) = text_extent(prompt);
    let width =
        fit(widest.saturating_add(BOX_CHROME), MIN_BOX_WIDTH, area.width);
    let height = hinted_box_height().saturating_add(rows - 1);
    centered_rect(width, height, area)
}

/// The box of a styled confirm dialog: wide enough for its longest body line
/// and its title, one row taller per further body line.
pub(super) fn styled_area(question: &StyledQuestion<'_>, area: Rect) -> Rect {
    let body = (question.lines)(body_width(area.width));
    let widest = body.iter().map(Line::width).max().unwrap_or(0);
    let widest = u16::try_from(widest).unwrap_or(u16::MAX);
    let title = u16::try_from(question.title.width()).unwrap_or(u16::MAX);
    let wanted = widest
        .saturating_add(BOX_CHROME)
        .max(title.saturating_add(TITLE_CHROME));
    let width = fit(wanted, MIN_BOX_WIDTH, area.width);
    let further_rows = u16::try_from(body.len().saturating_sub(1));
    let height =
        hinted_box_height().saturating_add(further_rows.unwrap_or(u16::MAX));
    centered_rect(width, height, area)
}

/// The columns the body of a `box_width` wide confirm box may fill. Sizing
/// and drawing both ask the caller for exactly this width, so the body that
/// was measured is the body that is drawn.
fn body_width(box_width: u16) -> u16 {
    box_width.saturating_sub(BOX_CHROME)
}

pub(super) fn render_confirm(
    frame: &mut Frame,
    skin: &Skin,
    question: &Question<'_>,
    rect: Rect,
) {
    let inner = overlay::framed(frame, rect, skin, " Confirm ");
    let width = inner.width as usize;
    let mut lines: Vec<Line> =
        question.prompt.split('\n').map(Line::from).collect();
    lines.extend(hint_block(&question.hints(), &skin.palette, width));
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

pub(super) fn render_styled(
    frame: &mut Frame,
    skin: &Skin,
    question: &StyledQuestion<'_>,
    rect: Rect,
) {
    let inner = overlay::framed(frame, rect, skin, question.title);
    let mut lines = (question.lines)(body_width(rect.width));
    let hints = answer_hints(question.default_yes);
    lines.extend(hint_block(&hints, &skin.palette, inner.width as usize));
    frame.render_widget(Paragraph::new(lines), inner);
}
