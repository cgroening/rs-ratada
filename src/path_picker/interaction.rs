//! Key dispatch for the picker: navigation, the hidden-entry toggle, and the
//! confinement-checked selection.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};

use super::{State, fs::confined_selection};
use crate::{input, nav, overlay::PopupFlow};

/// Applies one key to the picker, or reports the chosen path.
///
/// A named function rather than a closure inside [`popup_with_paste`], so the
/// `Ctrl+H` guard is reachable from a test: everything in the popup needs a
/// live terminal. `allow_files` decides whether `Enter` may return a file.
pub(super) fn handle_key(
    state: &mut State,
    key: KeyEvent,
    allow_files: bool,
) -> PopupFlow<PathBuf> {
    match key.code {
        KeyCode::Esc => PopupFlow::Cancelled,
        KeyCode::Up => {
            state.cursor = nav::cycle(state.cursor, state.visible.len(), -1);
            PopupFlow::Continue
        }
        KeyCode::Down => {
            state.cursor = nav::cycle(state.cursor, state.visible.len(), 1);
            PopupFlow::Continue
        }
        KeyCode::PageUp => {
            let page = state.viewport.get().max(1) as isize;
            state.cursor =
                nav::step_clamped(state.cursor, state.visible.len(), -page);
            PopupFlow::Continue
        }
        KeyCode::PageDown => {
            let page = state.viewport.get().max(1) as isize;
            state.cursor =
                nav::step_clamped(state.cursor, state.visible.len(), page);
            PopupFlow::Continue
        }
        KeyCode::Home => {
            state.cursor = 0;
            PopupFlow::Continue
        }
        KeyCode::End => {
            state.cursor = state.visible.len().saturating_sub(1);
            PopupFlow::Continue
        }
        KeyCode::Right => {
            state.descend();
            PopupFlow::Continue
        }
        KeyCode::Left => {
            state.ascend();
            PopupFlow::Continue
        }
        KeyCode::Backspace if state.filter.value().is_empty() => {
            state.ascend();
            PopupFlow::Continue
        }
        // `is_command`, so AltGr (Control+Alt) types instead of toggling.
        KeyCode::Char('h') if input::is_command(key) => {
            state.toggle_hidden();
            PopupFlow::Continue
        }
        KeyCode::Enter => match state.selected() {
            Some(entry) if entry.is_dir || allow_files => {
                match confined_selection(&entry.path, state.root.as_deref()) {
                    Some(path) => PopupFlow::Done(path),
                    None => PopupFlow::Continue,
                }
            }
            _ => PopupFlow::Continue,
        },
        _ => {
            if state.filter.handle_key(key) {
                state.refilter();
            }
            PopupFlow::Continue
        }
    }
}
