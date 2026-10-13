//! The informational message modal.

use std::io;

use super::ModalSignal;
use super::render::text_extent;
use crate::{
    layout::{centered_rect, fit},
    overlay::{self, PopupFlow, popup},
    terminal::Tui,
    theme::Skin,
};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};

/// The height a message box never goes below: two border rows around three
/// body rows, so a one-line message still reads as a dialog.
const MIN_BOX_HEIGHT: u16 = 5;

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
        |area, (): &()| message_area(body, area),
        |frame, (): &()| render_bg(frame),
        |frame, rect, (): &()| render_message(frame, skin, title, body, rect),
        |(): &mut (), _| PopupFlow::Done(()),
    )
}

/// The box of a message showing `body`: as wide as its longest line and tall
/// enough for every line, so a multi-line body keeps its line breaks.
pub(super) fn message_area(body: &str, area: Rect) -> Rect {
    let (widest, rows) = text_extent(body);
    let width = fit(widest.saturating_add(6), 28, area.width);
    let height = rows.saturating_add(2).max(MIN_BOX_HEIGHT);
    centered_rect(width, height, area)
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
