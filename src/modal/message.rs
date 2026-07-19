//! The informational message modal.

use std::io;

use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use super::ModalSignal;
use crate::{
    layout::{centered_rect, fit},
    overlay::{self, PopupFlow, popup},
    terminal::Tui,
    theme::Skin,
};

/// Shows an informational message until any key is pressed.
pub fn message(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    body: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<()>> {
    let mut state = ();
    popup(
        tui,
        &mut state,
        |area, (): &()| {
            let width = fit(body.width() as u16 + 6, 28, area.width);
            centered_rect(width, 5, area)
        },
        |frame, (): &()| render_bg(frame),
        |frame, rect, (): &()| render_message(frame, skin, title, body, rect),
        |(): &mut (), _| PopupFlow::Done(()),
    )
}

fn render_message(
    frame: &mut Frame,
    skin: &Skin,
    title: &str,
    body: &str,
    rect: Rect,
) {
    let inner = overlay::framed(frame, rect, skin, title);
    let paragraph = Paragraph::new(body.to_string()).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}
