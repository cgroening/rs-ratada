//! The yes/no dialog whose frame title and body the caller styles. The body
//! scrolls when it is taller than the screen allows, and a second view of it
//! can be switched to with `←`/`→`.

use std::{cell::Cell, io};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};
use unicode_width::UnicodeWidthStr;

use super::{
    ModalSignal,
    confirm::{
        BOX_CHROME, MIN_BOX_WIDTH, TITLE_CHROME, answer_hints, confirm_key,
    },
    render::{HINT_BLOCK_ROWS, hint_block, hinted_box_height},
};
use crate::{
    layout::{centered_rect, fit},
    nav::{ScrollView, step_clamped},
    overlay::{self, PopupFlow, popup},
    scroll, shortcut_hints,
    terminal::Tui,
    theme::{Color, Skin},
};

/// The hint shown while the body overflows the box.
const SCROLL_HINT: (&str, &str) = ("\u{2191}\u{2193}", "scroll");

/// The hint shown when the body has a second view.
const VIEW_HINT: (&str, &str) = ("\u{2190}\u{2192}", "view");

/// Builds a body for the given body width, in columns.
type BodyFn<'a> = &'a dyn Fn(u16) -> Vec<Line<'static>>;

/// A yes/no question whose frame title and body the caller supplies.
///
/// Where [`Question`](super::Question) shows one plain prompt under a fixed
/// title, this one lets the frame carry the actual question and the body carry
/// styled lines - label/value rows, a coloured warning, anything a [`Line`]
/// can express.
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

/// A [`StyledQuestion`] with a second view of its body, switched with `←`/`→`
/// (for example a detailed and a compact list of the same items).
pub struct StyledViews<'a> {
    /// The question; its `lines` are the view the dialog opens with.
    pub question: StyledQuestion<'a>,
    /// The other view of the body, built like `question.lines`.
    pub alternate: &'a dyn Fn(u16) -> Vec<Line<'static>>,
}

/// Asks a yes/no question with a caller-styled body under its own title.
///
/// The keys are those of [`confirm_default`](super::confirm_default): `y`
/// confirms, `n` and `Esc` decline, and `Enter` gives the answer
/// `question.default_yes` names. The box is as wide as the longest body line
/// (or the title) and as tall as the body; a body taller than the screen
/// scrolls with `↑`/`↓`, `PgUp`/`PgDn` and `Home`/`End`.
pub fn confirm_styled(
    tui: &mut Tui,
    skin: &Skin,
    question: &StyledQuestion<'_>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<bool>> {
    let dialog = Dialog {
        question,
        alternate: None,
    };
    run(tui, skin, &dialog, render_bg)
}

/// Asks a yes/no question like [`confirm_styled`], whose body has two views
/// that `←`/`→` switch between. It opens on `views.question.lines`; the box is
/// sized for the larger of both views, so switching never resizes it.
pub fn confirm_styled_views(
    tui: &mut Tui,
    skin: &Skin,
    views: &StyledViews<'_>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<bool>> {
    let dialog = Dialog {
        question: &views.question,
        alternate: Some(views.alternate),
    };
    run(tui, skin, &dialog, render_bg)
}

/// What a styled dialog shows: the question and, optionally, a second view.
pub(super) struct Dialog<'q, 'a> {
    /// The title, the first view and the `Enter` answer.
    pub question: &'q StyledQuestion<'a>,
    /// The second view, if the body has one.
    pub alternate: Option<BodyFn<'a>>,
}

/// The scroll position and the shown view of an open styled dialog.
#[derive(Debug, Default)]
pub(super) struct DialogState {
    /// The first body row on screen.
    pub offset: usize,
    /// Whether the second view is shown.
    pub is_alternate: bool,
    /// Body rows on screen at the last draw; paging and clamping need it.
    viewport: Cell<usize>,
    /// Body rows in total at the last draw.
    total: Cell<usize>,
}

impl DialogState {
    /// The largest offset that still fills the viewport.
    fn max_offset(&self) -> usize {
        self.total.get().saturating_sub(self.viewport.get())
    }
}

impl Dialog<'_, '_> {
    /// The body of the view `state` shows, for `width` columns.
    fn body(&self, state: &DialogState, width: u16) -> Vec<Line<'static>> {
        match self.alternate {
            Some(alternate) if state.is_alternate => alternate(width),
            _ => (self.question.lines)(width),
        }
    }

    /// Every view of the body, for `width` columns (for sizing the box).
    fn bodies(&self, width: u16) -> Vec<Vec<Line<'static>>> {
        let mut bodies = vec![(self.question.lines)(width)];
        if let Some(alternate) = self.alternate {
            bodies.push(alternate(width));
        }
        bodies
    }

    /// The footer hints: the answers, plus scrolling while the body overflows
    /// and switching when there is a second view.
    fn hints(&self, is_overflowing: bool) -> Vec<(&'static str, &'static str)> {
        let mut hints = answer_hints(self.question.default_yes).to_vec();
        if is_overflowing {
            hints.push(SCROLL_HINT);
        }
        if self.alternate.is_some() {
            hints.push(VIEW_HINT);
        }
        hints
    }
}

/// Runs the dialog until it is answered.
fn run(
    tui: &mut Tui,
    skin: &Skin,
    dialog: &Dialog<'_, '_>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<bool>> {
    let mut state = DialogState::default();
    popup(
        tui,
        &mut state,
        |area, _: &DialogState| dialog_area(dialog, area),
        |frame, _: &DialogState| render_bg(frame),
        |frame, rect, state: &DialogState| {
            render_dialog(frame, skin, &Drawn { dialog, state }, rect);
        },
        |state, key| dialog_key(dialog, state, key),
    )
}

/// A dialog together with its state, as one frame draws it.
pub(super) struct Drawn<'d, 'q, 'a> {
    /// What the dialog shows.
    pub dialog: &'d Dialog<'q, 'a>,
    /// Where it is scrolled to and which view it shows.
    pub state: &'d DialogState,
}

/// The box of a styled dialog: wide enough for the longest line of any view
/// and the title, one row taller per further body row of the taller view, and
/// never taller than `area` (the body scrolls then).
pub(super) fn dialog_area(dialog: &Dialog<'_, '_>, area: Rect) -> Rect {
    let bodies = dialog.bodies(body_width(area.width));
    let rows = bodies.iter().map(Vec::len).max().unwrap_or(0);
    let further_rows = u16::try_from(rows.saturating_sub(1));
    let height =
        hinted_box_height().saturating_add(further_rows.unwrap_or(u16::MAX));
    let is_overflowing = height > area.height;
    let widest = bodies
        .iter()
        .flatten()
        .map(Line::width)
        .chain([hints_width(&dialog.hints(is_overflowing))])
        .max()
        .unwrap_or(0);
    let widest = u16::try_from(widest).unwrap_or(u16::MAX);
    let title =
        u16::try_from(dialog.question.title.width()).unwrap_or(u16::MAX);
    let wanted = widest
        .saturating_add(BOX_CHROME)
        .max(title.saturating_add(TITLE_CHROME));
    let width = fit(wanted, MIN_BOX_WIDTH, area.width);
    centered_rect(width, height, area)
}

/// The columns `hints` take on one row, so the box never cuts them off.
fn hints_width(hints: &[(&str, &str)]) -> usize {
    shortcut_hints::lines(hints, Color::Default, usize::MAX)
        .first()
        .map_or(0, Line::width)
}

/// Draws the visible part of the body, the hints and, while the body
/// overflows, a scrollbar. Records the viewport for the key handler.
pub(super) fn render_dialog(
    frame: &mut Frame,
    skin: &Skin,
    drawn: &Drawn<'_, '_, '_>,
    rect: Rect,
) {
    let Drawn { dialog, state } = *drawn;
    let inner = overlay::framed(frame, rect, skin, dialog.question.title);
    let body = dialog.body(state, body_width(rect.width));
    let viewport = usize::from(inner.height)
        .saturating_sub(usize::from(HINT_BLOCK_ROWS))
        .max(1);
    state.viewport.set(viewport);
    state.total.set(body.len());
    let offset = state.offset.min(state.max_offset());
    let is_overflowing = body.len() > viewport;

    let mut lines: Vec<Line<'static>> =
        body.into_iter().skip(offset).take(viewport).collect();
    // A shorter view keeps the hints where the taller one puts them.
    lines.resize(viewport, Line::default());
    let hints = dialog.hints(is_overflowing);
    lines.extend(hint_block(&hints, &skin.palette, usize::from(inner.width)));
    frame.render_widget(Paragraph::new(lines), inner);

    if is_overflowing {
        let bar = Rect {
            height: u16::try_from(viewport).unwrap_or(inner.height),
            ..inner
        };
        let view = ScrollView {
            total: state.total.get(),
            offset,
            viewport,
        };
        scroll::render_scrollbar(frame, bar, skin, view);
    }
}

/// Scrolls or switches the view, or answers the question.
fn dialog_key(
    dialog: &Dialog<'_, '_>,
    state: &mut DialogState,
    key: KeyEvent,
) -> PopupFlow<bool> {
    let positions = state.max_offset() + 1;
    let page = isize::try_from(state.viewport.get().max(1)).unwrap_or(1);
    let step = |delta: isize| step_clamped(state.offset, positions, delta);
    let offset = match key.code {
        KeyCode::Up => step(-1),
        KeyCode::Down => step(1),
        KeyCode::PageUp => step(-page),
        KeyCode::PageDown => step(page),
        KeyCode::Home => 0,
        KeyCode::End => state.max_offset(),
        KeyCode::Left | KeyCode::Right if dialog.alternate.is_some() => {
            state.is_alternate = !state.is_alternate;
            0
        }
        _ => return confirm_key(key, dialog.question.default_yes),
    };
    state.offset = offset;
    PopupFlow::Continue
}

/// The columns the body of a `box_width` wide confirm box may fill. Sizing
/// and drawing both ask the caller for exactly this width, so the body that
/// was measured is the body that is drawn.
fn body_width(box_width: u16) -> u16 {
    box_width.saturating_sub(BOX_CHROME)
}
