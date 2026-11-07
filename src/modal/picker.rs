//! Selection modals: single, multi, styled and reorderable pickers.

use std::{cell::Cell, io};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, style::Style};

use super::ModalSignal;
use super::multi_state::{CONFIRM_HINTS, MultiSelect};
use super::render::{
    CHECK_PREFIX_WIDTH, PickerChrome, SELECT_HINTS, labels_width, picker_area,
    render_picker, render_styled_picker,
};
use crate::{
    input::{self},
    nav,
    overlay::{PopupFlow, popup},
    terminal::Tui,
    theme::Skin,
};

/// The keys of [`select_reorderable`].
const REORDER_HINTS: [(&str, &str); 3] = [
    ("alt+\u{2191}\u{2193}", "move"),
    ("enter", "pick"),
    ("esc", "cancel"),
];

/// Applies the shared list-navigation keys to `cursor` over `len` items,
/// returning whether the key was one of them. `Up`/`Down` (and `k`/`j`) wrap
/// cyclically; `PageUp`/`PageDown` move by `page` rows and `Home`/`End` jump to
/// the ends, both clamped. The caller handles the picker's own keys (`Enter`,
/// `Esc`, ...) for the keys this leaves unconsumed.
///
/// Navigation is bare keys only: a Ctrl chord is left unconsumed, so `Ctrl+J`
/// cannot move the cursor and a caller stays free to bind `Ctrl+<key>` itself.
pub(super) fn navigate_list(
    cursor: &mut usize,
    key: KeyEvent,
    len: usize,
    page: usize,
) -> bool {
    navigate_list_without_vim(cursor, vim_as_arrow(key), len, page)
}

/// Like [`navigate_list`] but without `j`/`k`, which stay free for the caller
/// (a list whose items answer to letters).
pub(super) fn navigate_list_without_vim(
    cursor: &mut usize,
    key: KeyEvent,
    len: usize,
    page: usize,
) -> bool {
    if input::is_command(key) {
        return false;
    }
    let page = page.max(1) as isize;
    match key.code {
        KeyCode::Up => *cursor = nav::cycle(*cursor, len, -1),
        KeyCode::Down => *cursor = nav::cycle(*cursor, len, 1),
        KeyCode::PageUp => *cursor = nav::step_clamped(*cursor, len, -page),
        KeyCode::PageDown => *cursor = nav::step_clamped(*cursor, len, page),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = len.saturating_sub(1),
        _ => return false,
    }
    true
}

/// Maps `k`/`j` onto `Up`/`Down`, keeping the modifiers, so a Ctrl chord
/// stays a chord.
fn vim_as_arrow(key: KeyEvent) -> KeyEvent {
    let code = match key.code {
        KeyCode::Char('k') => KeyCode::Up,
        KeyCode::Char('j') => KeyCode::Down,
        other => other,
    };
    KeyEvent { code, ..key }
}

/// Lets the user pick one entry from a list. `Enter` picks, `Esc` cancels;
/// both keys are shown in a hint line at the bottom.
pub fn select(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[String],
    initial: usize,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<usize>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut cursor = initial.min(items.len() - 1);
    let page_rows = Cell::new(1usize);
    popup(
        tui,
        &mut cursor,
        |area, _| {
            picker_area(area, items.len(), plain_width(items), &SELECT_HINTS)
        },
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            let chrome = PickerChrome {
                title,
                cursor: *cursor,
                checked: None,
                hints: &SELECT_HINTS,
            };
            page_rows.set(render_picker(frame, skin, items, &chrome, rect));
        },
        |cursor, key| pick_key(cursor, key, items.len(), page_rows.get()),
    )
}

/// Lets the user tick several entries. `Space` toggles the cursor row and
/// `Esc` cancels. `Enter` confirms the ticks; with nothing ticked it takes the
/// cursor row instead, so one entry is picked without ticking it - unless the
/// user unticked everything, then the empty set is confirmed. The keys are
/// shown in a hint line at the bottom.
pub fn multi_select(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[String],
    initial: &[usize],
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Vec<usize>>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut state = MultiSelect::new(initial);
    popup(
        tui,
        &mut state,
        |area, _| {
            let width = plain_width(items) + CHECK_PREFIX_WIDTH;
            picker_area(area, items.len(), width, &CONFIRM_HINTS)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &MultiSelect| {
            let hints = state.hints();
            let chrome = PickerChrome {
                title,
                cursor: state.cursor,
                checked: Some((&state.checked, skin.glyphs.mark)),
                hints: &hints,
            };
            state
                .viewport
                .set(render_picker(frame, skin, items, &chrome, rect));
        },
        |state, key| state.handle_key(key, items.len()),
    )
}

/// Outcome of [`select_reorderable`]: a final pick or a reorder request that
/// the caller applies before reopening.
pub enum ListAction {
    /// The user picked the item at this index.
    Pick(usize),
    /// The user asked to move the item at `index` by `delta` positions.
    Move {
        /// The index of the item to move.
        index: usize,
        /// The signed number of positions to move it by.
        delta: i32,
    },
}

/// Like [`select`] but `Alt+Up`/`Alt+Down` return a [`ListAction::Move`] so the
/// caller can reorder the list and reopen.
pub fn select_reorderable(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[String],
    initial: usize,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<ListAction>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut cursor = initial.min(items.len() - 1);
    let page_rows = Cell::new(1usize);
    popup(
        tui,
        &mut cursor,
        |area, _| {
            picker_area(area, items.len(), plain_width(items), &REORDER_HINTS)
        },
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            let chrome = PickerChrome {
                title,
                cursor: *cursor,
                checked: None,
                hints: &REORDER_HINTS,
            };
            page_rows.set(render_picker(frame, skin, items, &chrome, rect));
        },
        |cursor, key| {
            // Alt+Up/Down reorder; the plain motions fall to the shared nav.
            // Alt *without* Ctrl, so AltGr (reported as Ctrl+Alt) is not read
            // as a reorder chord.
            let alt = key.modifiers.contains(KeyModifiers::ALT)
                && !key.modifiers.contains(KeyModifiers::CONTROL);
            match key.code {
                KeyCode::Up if alt => {
                    return PopupFlow::Done(ListAction::Move {
                        index: *cursor,
                        delta: -1,
                    });
                }
                KeyCode::Down if alt => {
                    return PopupFlow::Done(ListAction::Move {
                        index: *cursor,
                        delta: 1,
                    });
                }
                _ => {}
            }
            match pick_key(cursor, key, items.len(), page_rows.get()) {
                PopupFlow::Done(index) => {
                    PopupFlow::Done(ListAction::Pick(index))
                }
                PopupFlow::Cancelled => PopupFlow::Cancelled,
                PopupFlow::Continue => PopupFlow::Continue,
            }
        },
    )
}

/// Like [`select`] but each item carries its own style (for coloured glyphs).
pub fn select_styled(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[(String, Style)],
    initial: usize,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<usize>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut cursor = initial.min(items.len() - 1);
    let page_rows = Cell::new(1usize);
    popup(
        tui,
        &mut cursor,
        |area, _| {
            picker_area(area, items.len(), styled_width(items), &SELECT_HINTS)
        },
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            let chrome = PickerChrome {
                title,
                cursor: *cursor,
                checked: None,
                hints: &SELECT_HINTS,
            };
            let viewport =
                render_styled_picker(frame, skin, items, &chrome, rect);
            page_rows.set(viewport);
        },
        |cursor, key| pick_key(cursor, key, items.len(), page_rows.get()),
    )
}

/// Like [`multi_select`] but each item carries its own style.
pub fn multi_select_styled(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[(String, Style)],
    initial: &[usize],
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Vec<usize>>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut state = MultiSelect::new(initial);
    popup(
        tui,
        &mut state,
        |area, _| {
            let width = styled_width(items) + CHECK_PREFIX_WIDTH;
            picker_area(area, items.len(), width, &CONFIRM_HINTS)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &MultiSelect| {
            let hints = state.hints();
            let chrome = PickerChrome {
                title,
                cursor: state.cursor,
                checked: Some((&state.checked, skin.glyphs.mark)),
                hints: &hints,
            };
            let viewport =
                render_styled_picker(frame, skin, items, &chrome, rect);
            state.viewport.set(viewport);
        },
        |state, key| state.handle_key(key, items.len()),
    )
}

/// The keys of a single-choice list: navigation, `Enter` picks the cursor
/// row, `Esc` cancels.
fn pick_key(
    cursor: &mut usize,
    key: KeyEvent,
    len: usize,
    page: usize,
) -> PopupFlow<usize> {
    if navigate_list(cursor, key, len, page) {
        return PopupFlow::Continue;
    }
    match key.code {
        KeyCode::Enter => PopupFlow::Done(*cursor),
        KeyCode::Esc => PopupFlow::Cancelled,
        _ => PopupFlow::Continue,
    }
}

/// The widest plain label, for sizing a picker.
fn plain_width(items: &[String]) -> usize {
    labels_width(items.iter().map(String::as_str))
}

/// The widest styled label, for sizing a picker.
fn styled_width(items: &[(String, Style)]) -> usize {
    labels_width(items.iter().map(|(label, _)| label.as_str()))
}
