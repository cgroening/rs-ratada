//! Generic event-loop driver: a [`Screen`] trait and the [`run`] loop.

use std::{io, time::Duration};

use crossterm::event::KeyEvent;
use ratatui::Frame;

use super::{
    quit::{self, QuitKind},
    shortcut_hints,
    terminal::{Tui, TuiEvent},
};

/// Redraw cadence when no input arrives, so [`Screen::tick`] can animate.
const TICK: Duration = Duration::from_millis(100);

/// Control-flow signal returned from a screen's key handler.
pub enum Flow {
    /// Keep running.
    Continue,
    /// Exit the loop.
    Quit,
}

/// A full-screen UI that [`run`] can drive.
///
/// `render` is `&self` so it can be reused as the background of a modal while
/// the handler holds `&mut Tui`. The associated `Error` keeps this kit free of
/// any concrete error crate; it only needs to absorb I/O errors.
pub trait Screen {
    /// The host's error type; only needs to absorb I/O errors.
    type Error: From<io::Error>;

    /// Draws the screen into `frame`.
    fn render(&self, frame: &mut Frame);

    /// Handles a key press, returning whether to keep running or quit.
    ///
    /// # Errors
    ///
    /// Returns the host's error if handling the key fails.
    fn handle_key(
        &mut self,
        key: KeyEvent,
        tui: &mut Tui,
    ) -> Result<Flow, Self::Error>;

    /// Called on each idle tick (no input within `TICK`); use it to advance
    /// animations. Default: do nothing.
    fn tick(&mut self) {}
}

/// Runs the draw/read/handle loop until the screen or the user quits.
///
/// Redraws every iteration; when no event arrives within `TICK`, calls
/// [`Screen::tick`] so animated widgets keep moving. The global hints toggle
/// (see `shortcut_hints::set_toggle_key`) is consumed here, so every screen
/// inherits it and never sees the key. The hard quit chord is put through
/// [`quit::request`], which asks only when the host opted in.
///
/// # Errors
///
/// Propagates any error from drawing, reading input or the screen's handler.
pub fn run<S: Screen>(tui: &mut Tui, screen: &mut S) -> Result<(), S::Error> {
    loop {
        tui.draw(|frame| screen.render(frame))?;
        match tui.poll_event(TICK)? {
            None => screen.tick(),
            Some(TuiEvent::Quit) => {
                let repaint = |frame: &mut Frame| screen.render(frame);
                if quit::request(tui, QuitKind::Hard, &repaint) {
                    break;
                }
            }
            Some(TuiEvent::Resize) => {}
            // The next iteration redraws with the new visibility.
            Some(TuiEvent::Key(key)) if shortcut_hints::consume_toggle(key) => {
            }
            Some(TuiEvent::Key(key)) => {
                if let Flow::Quit = screen.handle_key(key, tui)? {
                    break;
                }
            }
        }
    }
    Ok(())
}
