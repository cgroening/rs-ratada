//! Key dispatch for the colour picker: moving focus, editing a channel or the
//! hex field, and picking a preset.

use crossterm::event::{KeyCode, KeyModifiers};

use super::{Focus, Outcome, State};
use crate::{clipboard, input, nav, overlay::PopupFlow, theme::parse_color};

/// Routes a key press to the focused control.
pub(super) fn handle(
    state: &mut State,
    key: crossterm::event::KeyEvent,
) -> PopupFlow<Outcome> {
    match key.code {
        KeyCode::Enter => {
            return PopupFlow::Done(Outcome::Done(state.current_color()));
        }
        KeyCode::Esc => {
            return PopupFlow::Done(Outcome::Back(state.current_color()));
        }
        KeyCode::Tab | KeyCode::Down => {
            state.cycle_focus(1);
            return PopupFlow::Continue;
        }
        KeyCode::BackTab | KeyCode::Up => {
            state.cycle_focus(-1);
            return PopupFlow::Continue;
        }
        _ => {}
    }
    match state.focus {
        Focus::Hex => handle_hex(state, key),
        Focus::Channel(index) => handle_channel(state, index, key),
        Focus::Presets => handle_presets(state, key),
    }
}

/// Edits the hex field; a valid entry updates the channels live.
fn handle_hex(
    state: &mut State,
    key: crossterm::event::KeyEvent,
) -> PopupFlow<Outcome> {
    if state.hex.handle_key(key)
        && let Some(color) = parse_color(state.hex.value())
    {
        state.adopt(color);
    }
    PopupFlow::Continue
}

/// Routes a paste to the hex field, but only while it is focused; a valid entry
/// updates the channels live, like [`handle_hex`].
pub(super) fn handle_paste(
    state: &mut State,
    text: &str,
) -> PopupFlow<Outcome> {
    if state.focus == Focus::Hex {
        state.hex.paste(text);
        if let Some(color) = parse_color(state.hex.value()) {
            state.adopt(color);
        }
    }
    PopupFlow::Continue
}

/// Adjusts the focused channel, or handles the model/copy chords.
fn handle_channel(
    state: &mut State,
    index: usize,
    key: crossterm::event::KeyEvent,
) -> PopupFlow<Outcome> {
    // This control binds no Ctrl chord of its own, so one must not reach the
    // bare keys below (Ctrl+Y would copy, Ctrl+S leave for the swatches).
    // `Shift` stays honoured: it only picks the fine step.
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    let channel = state.model.channels()[index];
    let step = if key.modifiers.contains(KeyModifiers::SHIFT) {
        1.0
    } else {
        channel.coarse
    };
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => state.adjust(index, -step),
        KeyCode::Right | KeyCode::Char('l') => state.adjust(index, step),
        KeyCode::Home => state.set(index, channel.min),
        KeyCode::End => state.set(index, channel.max),
        KeyCode::PageUp => state.adjust(index, -channel.page()),
        KeyCode::PageDown => state.adjust(index, channel.page()),
        KeyCode::Char('m') => state.set_model(state.model.next()),
        KeyCode::Char('y') => copy_hex(state),
        KeyCode::Char('s') => {
            return PopupFlow::Done(Outcome::Swatches(state.current_color()));
        }
        _ => {}
    }
    PopupFlow::Continue
}

/// Picks a preset (live) or handles the model/copy chords.
fn handle_presets(
    state: &mut State,
    key: crossterm::event::KeyEvent,
) -> PopupFlow<Outcome> {
    // This control binds no Ctrl chord of its own, so one must not reach the
    // bare keys below (Ctrl+Y would copy, Ctrl+S leave for the swatches).
    if input::is_command(key) {
        return PopupFlow::Continue;
    }
    let count = state.presets.len();
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            select_preset(state, nav::cycle(state.preset, count, -1));
        }
        KeyCode::Right | KeyCode::Char('l') => {
            select_preset(state, nav::cycle(state.preset, count, 1));
        }
        KeyCode::Home => select_preset(state, 0),
        KeyCode::End => select_preset(state, count.saturating_sub(1)),
        KeyCode::Char('m') => state.set_model(state.model.next()),
        KeyCode::Char('y') => copy_hex(state),
        KeyCode::Char('s') => {
            return PopupFlow::Done(Outcome::Swatches(state.current_color()));
        }
        _ => {}
    }
    PopupFlow::Continue
}

/// Adopts the preset at `index` as the live color and refreshes the hex field.
fn select_preset(state: &mut State, index: usize) {
    state.preset = index;
    state.adopt(state.presets[index]);
    state.sync_hex();
}

/// Copies the current color's hex code to the clipboard (best effort).
fn copy_hex(state: &State) {
    let _ = clipboard::copy(&state.current_color().to_hex());
}
