//! A choice between a few answers shown side by side as buttons, like
//! "Changed only" / "All": `←`/`→` move the mark, `Enter` takes the marked
//! answer and `Esc` cancels.

use std::io;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::{
    ModalSignal,
    confirm::{BOX_CHROME, MIN_BOX_WIDTH, TITLE_CHROME},
    render::{hint_block, hinted_box_height, hints_width},
};
use crate::{
    layout::{centered_rect, fit},
    nav::step_clamped,
    overlay::{self, PopupFlow, popup},
    style,
    terminal::Tui,
    theme::Skin,
};

/// The footer hints.
const HINTS: [(&str, &str); 3] = [
    ("\u{2190}\u{2192}", "choose"),
    ("Enter", "ok"),
    ("Esc", "cancel"),
];

/// The space between two buttons.
const GAP: &str = "   ";

/// A question answered by picking one of a few buttons.
pub struct Choice<'a> {
    /// The frame title, usually the question itself.
    pub title: &'a str,
    /// The button labels, left to right.
    pub options: &'a [&'a str],
    /// The index of the button marked when the dialog opens.
    pub default: usize,
}

/// Asks `choice` and returns the index of the taken button, or
/// `Cancelled` for `Esc`. `←`/`→`, `h`/`l` and `Tab`/`⇧Tab` move the mark
/// and stop at either end; `Enter` takes the marked button.
pub fn choose(
    tui: &mut Tui,
    skin: &Skin,
    choice: &Choice<'_>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<usize>> {
    let len = choice.options.len();
    let mut marked = choice.default.min(len.saturating_sub(1));
    popup(
        tui,
        &mut marked,
        |area, _: &usize| choice_area(choice, area),
        |frame, _: &usize| render_bg(frame),
        |frame, rect, marked: &usize| {
            let inner = overlay::framed(frame, rect, skin, choice.title);
            let mut lines = vec![
                Line::default(),
                buttons_line(choice.options, *marked, skin),
                Line::default(),
            ];
            let width = usize::from(inner.width);
            lines.extend(hint_block(&HINTS, &skin.palette, width));
            frame.render_widget(Paragraph::new(lines), inner);
        },
        |marked, key| choice_key(marked, key, len),
    )
}

/// The box: wide enough for the buttons, the hints and the title.
fn choice_area(choice: &Choice<'_>, area: Rect) -> Rect {
    let buttons = buttons_width(choice.options);
    let widest = buttons.max(hints_width(&HINTS));
    let widest = u16::try_from(widest).unwrap_or(u16::MAX);
    let title = u16::try_from(choice.title.width()).unwrap_or(u16::MAX);
    let wanted = widest
        .saturating_add(BOX_CHROME)
        .max(title.saturating_add(TITLE_CHROME));
    let width = fit(wanted, MIN_BOX_WIDTH, area.width);
    // One empty row above and below the buttons.
    let inner = width.saturating_sub(BOX_CHROME);
    let height = hinted_box_height(&HINTS, inner).saturating_add(2);
    centered_rect(width, height, area)
}

/// The columns the button row takes.
fn buttons_width(options: &[&str]) -> usize {
    let labels: usize = options.iter().map(|label| button(label).width()).sum();
    labels + GAP.width() * options.len().saturating_sub(1)
}

/// A label padded into a button.
fn button(label: &str) -> String {
    format!(" {label} ")
}

/// The button row, the `marked` button highlighted.
fn buttons_line(options: &[&str], marked: usize, skin: &Skin) -> Line<'static> {
    let palette = &skin.palette;
    let mut spans = Vec::new();
    for (index, label) in options.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(GAP));
        }
        let style = if index == marked {
            style::selected(palette)
                .patch(style::accent(palette))
                .add_modifier(Modifier::BOLD)
        } else {
            style::secondary(palette)
        };
        spans.push(Span::styled(button(label), style));
    }
    Line::from(spans)
}

/// Moves the mark or answers: the index for `Enter`, `Cancelled` for `Esc`.
fn choice_key(
    marked: &mut usize,
    key: KeyEvent,
    len: usize,
) -> PopupFlow<usize> {
    let has_extra = !key.modifiers.difference(KeyModifiers::SHIFT).is_empty();
    let delta = match key.code {
        _ if has_extra => return PopupFlow::Continue,
        KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => -1,
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => 1,
        KeyCode::Enter => return PopupFlow::Done(*marked),
        KeyCode::Esc => return PopupFlow::Cancelled,
        _ => return PopupFlow::Continue,
    };
    *marked = step_clamped(*marked, len, delta);
    PopupFlow::Continue
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn arrows_move_the_mark_and_stop_at_the_ends() {
        let mut marked = 0;
        choice_key(&mut marked, key(KeyCode::Left), 2);
        assert_eq!(marked, 0, "stops at the left end");
        choice_key(&mut marked, key(KeyCode::Right), 2);
        assert_eq!(marked, 1);
        choice_key(&mut marked, key(KeyCode::Tab), 2);
        assert_eq!(marked, 1, "stops at the right end");
        choice_key(&mut marked, key(KeyCode::Char('h')), 2);
        assert_eq!(marked, 0);
    }

    #[test]
    fn enter_takes_the_marked_button_and_esc_cancels() {
        let mut marked = 1;
        assert!(matches!(
            choice_key(&mut marked, key(KeyCode::Enter), 2),
            PopupFlow::Done(1)
        ));
        assert!(matches!(
            choice_key(&mut marked, key(KeyCode::Esc), 2),
            PopupFlow::Cancelled
        ));
    }

    #[test]
    fn chords_do_nothing() {
        let mut marked = 0;
        let ctrl_l = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL);
        assert!(matches!(
            choice_key(&mut marked, ctrl_l, 2),
            PopupFlow::Continue
        ));
        assert_eq!(marked, 0);
    }

    #[test]
    fn the_buttons_are_measured_with_their_padding_and_gaps() {
        assert_eq!(buttons_width(&["ab", "c"]), 4 + 3 + 3);
    }
}
