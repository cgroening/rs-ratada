//! Key dispatch for the swatch picker: mode switching, filtering, grid
//! movement and the exit paths.

use crossterm::event::{KeyCode, KeyEvent};

use super::{Choice, GRID_LIGHT_STEP, Mode, State, VISIBLE_ROWS};
use crate::{clipboard, input, nav, overlay::PopupFlow};

/// Routes a key to the active mode.
pub(super) fn handle(state: &mut State, key: KeyEvent) -> PopupFlow<Choice> {
    if state.mode == Mode::Names && state.filtering {
        return handle_filter(state, key);
    }
    // This picker binds no Ctrl chord of its own, so one must not reach the
    // bare keys below (Ctrl+Y would copy the hex, Ctrl+Space pick and close).
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    let len = state.cells.len();
    match key.code {
        KeyCode::Enter => return done(state, false),
        KeyCode::Char(' ') => return done(state, true),
        KeyCode::Esc => return PopupFlow::Cancelled,
        KeyCode::Char('m') => state.switch_mode(),
        KeyCode::Char('y') => {
            let _ = clipboard::copy(&state.focus_color().to_hex());
        }
        KeyCode::Up | KeyCode::Char('k') => move_vertical(state, -1),
        KeyCode::Down | KeyCode::Char('j') => move_vertical(state, 1),
        KeyCode::Left | KeyCode::Char('h') => move_horizontal(state, -1),
        KeyCode::Right | KeyCode::Char('l') => move_horizontal(state, 1),
        KeyCode::PageUp if state.mode.is_list() => {
            state.cursor =
                nav::step_clamped(state.cursor, len, -(VISIBLE_ROWS as isize));
        }
        KeyCode::PageDown if state.mode.is_list() => {
            state.cursor =
                nav::step_clamped(state.cursor, len, VISIBLE_ROWS as isize);
        }
        KeyCode::Home => state.cursor = 0,
        KeyCode::End => state.cursor = len.saturating_sub(1),
        KeyCode::Char('[') if state.mode == Mode::Grid => {
            adjust_light(state, -GRID_LIGHT_STEP);
        }
        KeyCode::Char(']') if state.mode == Mode::Grid => {
            adjust_light(state, GRID_LIGHT_STEP);
        }
        KeyCode::Char('/') if state.mode == Mode::Names => {
            state.filtering = true;
        }
        _ => {}
    }
    PopupFlow::Continue
}

/// Edits the named-list filter; any change resets the cursor to the first match.
pub(super) fn handle_filter(
    state: &mut State,
    key: KeyEvent,
) -> PopupFlow<Choice> {
    match key.code {
        KeyCode::Esc => {
            state.filter.clear();
            state.filtering = false;
            state.rebuild();
            state.cursor = 0;
        }
        KeyCode::Enter => return done(state, false),
        KeyCode::Up => {
            state.cursor = nav::cycle(state.cursor, state.cells.len(), -1);
        }
        KeyCode::Down => {
            state.cursor = nav::cycle(state.cursor, state.cells.len(), 1);
        }
        KeyCode::Backspace => {
            state.filter.pop();
            state.rebuild();
            state.cursor = 0;
        }
        // Anything that is not a command chord is filter text: without this,
        // Ctrl+U would insert a `u` instead of being left for a line-clear.
        // Deliberately not `is_bare_character`, which would also reject AltGr -
        // `@`, `\` and `[` must reach the filter, exactly as in
        // `input::apply_edit_key`.
        KeyCode::Char(ch) if !input::is_command(key) => {
            state.filter.push(ch);
            state.rebuild();
            state.cursor = 0;
        }
        _ => {}
    }
    PopupFlow::Continue
}

/// Finishes with the focused color, either taken directly or sent to the editor.
pub(super) fn done(state: &State, pick: bool) -> PopupFlow<Choice> {
    match state.cells.get(state.cursor) {
        Some(cell) if pick => PopupFlow::Done(Choice::Pick(cell.color)),
        Some(cell) => PopupFlow::Done(Choice::Edit(cell.color)),
        None => PopupFlow::Continue,
    }
}

/// Moves the cursor a row up/down: list modes wrap, grid modes clamp.
pub(super) fn move_vertical(state: &mut State, direction: isize) {
    let len = state.cells.len();
    if len == 0 {
        return;
    }
    if state.mode.is_list() {
        state.cursor = nav::cycle(state.cursor, len, direction);
        return;
    }
    let step = direction * state.cols as isize;
    let next = state.cursor as isize + step;
    if next >= 0 && (next as usize) < len {
        state.cursor = next as usize;
    }
}

/// Moves the cursor within its row: the hue grid wraps, others clamp; list modes
/// have a single column and ignore it.
pub(super) fn move_horizontal(state: &mut State, direction: isize) {
    if state.mode.is_list() {
        return;
    }
    let cols = state.cols.max(1);
    let row = state.cursor / cols;
    let col = state.cursor % cols;
    let next_col = if state.mode == Mode::Grid {
        (col as isize + direction).rem_euclid(cols as isize) as usize
    } else {
        (col as isize + direction).clamp(0, cols as isize - 1) as usize
    };
    let candidate = row * cols + next_col;
    if candidate < state.cells.len() {
        state.cursor = candidate;
    }
}

/// Shifts the grid's lightness plane and rebuilds it (keeping the cursor cell).
fn adjust_light(state: &mut State, delta: f32) {
    state.grid_light = (state.grid_light + delta).clamp(0.05, 0.95);
    state.rebuild();
}
