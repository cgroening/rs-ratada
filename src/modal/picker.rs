//! Selection modals: single, multi, styled and reorderable pickers.

use std::io;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, style::Style};

use std::{cell::Cell, collections::HashSet};

use super::ModalSignal;
use super::render::{picker_area, render_picker, render_styled_picker};
use crate::{
    input::{self},
    nav,
    overlay::{PopupFlow, popup},
    terminal::Tui,
    theme::Skin,
};

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
    if input::is_command(key) {
        return false;
    }
    let page = page.max(1) as isize;
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            *cursor = nav::cycle(*cursor, len, -1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            *cursor = nav::cycle(*cursor, len, 1);
        }
        KeyCode::PageUp => *cursor = nav::step_clamped(*cursor, len, -page),
        KeyCode::PageDown => *cursor = nav::step_clamped(*cursor, len, page),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = len.saturating_sub(1),
        _ => return false,
    }
    true
}

/// Lets the user pick one entry from a list. `Esc` cancels.
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
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            let viewport =
                render_picker(frame, skin, title, items, *cursor, None, rect);
            page_rows.set(viewport);
        },
        |cursor, key| {
            if navigate_list(cursor, key, items.len(), page_rows.get()) {
                return PopupFlow::Continue;
            }
            match key.code {
                KeyCode::Enter => PopupFlow::Done(*cursor),
                KeyCode::Esc => PopupFlow::Cancelled,
                _ => PopupFlow::Continue,
            }
        },
    )
}

/// Lets the user toggle several entries. `Space` toggles, `Enter` confirms the
/// selected set, `Esc` cancels.
pub fn multi_select(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[String],
    initial: &[usize],
    check: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Vec<usize>>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut state = MultiSelect::new(initial);
    popup(
        tui,
        &mut state,
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, state: &MultiSelect| {
            let checked = Some((&state.checked, check));
            let viewport = render_picker(
                frame,
                skin,
                title,
                items,
                state.cursor,
                checked,
                rect,
            );
            state.viewport.set(viewport);
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
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            let viewport =
                render_picker(frame, skin, title, items, *cursor, None, rect);
            page_rows.set(viewport);
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
            if navigate_list(cursor, key, items.len(), page_rows.get()) {
                return PopupFlow::Continue;
            }
            match key.code {
                KeyCode::Enter => PopupFlow::Done(ListAction::Pick(*cursor)),
                KeyCode::Esc => PopupFlow::Cancelled,
                _ => PopupFlow::Continue,
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
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            let viewport = render_styled_picker(
                frame, skin, title, items, *cursor, None, rect,
            );
            page_rows.set(viewport);
        },
        |cursor, key| {
            if navigate_list(cursor, key, items.len(), page_rows.get()) {
                return PopupFlow::Continue;
            }
            match key.code {
                KeyCode::Enter => PopupFlow::Done(*cursor),
                KeyCode::Esc => PopupFlow::Cancelled,
                _ => PopupFlow::Continue,
            }
        },
    )
}

/// Like [`multi_select`] but each item carries its own style.
pub fn multi_select_styled(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[(String, Style)],
    initial: &[usize],
    check: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Vec<usize>>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut state = MultiSelect::new(initial);
    popup(
        tui,
        &mut state,
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, state: &MultiSelect| {
            let checked = Some((&state.checked, check));
            let cursor = state.cursor;
            let viewport = render_styled_picker(
                frame, skin, title, items, cursor, checked, rect,
            );
            state.viewport.set(viewport);
        },
        |state, key| state.handle_key(key, items.len()),
    )
}

/// The state shared by the multi-select modals: the cursor, the toggled set and
/// the list viewport height captured at render (for page-wise navigation).
struct MultiSelect {
    cursor: usize,
    checked: HashSet<usize>,
    viewport: Cell<usize>,
}

impl MultiSelect {
    fn new(initial: &[usize]) -> Self {
        Self {
            cursor: 0,
            checked: initial.iter().copied().collect(),
            viewport: Cell::new(1),
        }
    }

    fn handle_key(
        &mut self,
        key: KeyEvent,
        len: usize,
    ) -> PopupFlow<Vec<usize>> {
        if navigate_list(&mut self.cursor, key, len, self.viewport.get()) {
            return PopupFlow::Continue;
        }
        // This picker binds no Ctrl chord of its own, so one must not reach the
        // bare keys below (Ctrl+Space would toggle the checked row).
        if input::is_command(key) {
            return PopupFlow::Continue;
        }
        match key.code {
            KeyCode::Char(' ') => {
                if !self.checked.insert(self.cursor) {
                    self.checked.remove(&self.cursor);
                }
                PopupFlow::Continue
            }
            KeyCode::Enter => {
                let mut chosen: Vec<usize> =
                    self.checked.iter().copied().collect();
                chosen.sort_unstable();
                PopupFlow::Done(chosen)
            }
            KeyCode::Esc => PopupFlow::Cancelled,
            _ => PopupFlow::Continue,
        }
    }
}
