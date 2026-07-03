//! Generic event-loop driver: a [`Screen`] trait and the [`run`] loop.

use std::{io, time::Duration};

use crossterm::event::KeyEvent;
use ratatui::Frame;

use super::terminal::{Tui, TuiEvent};

/// Redraw cadence when no input arrives, so [`Screen::tick`] can animate.
const TICK: Duration = Duration::from_millis(100);

/// Control-flow signal returned from a screen's key handler.
pub enum Flow {
    Continue,
    Quit,
}

/// A full-screen UI that [`run`] can drive.
///
/// `render` is `&self` so it can be reused as the background of a modal while
/// the handler holds `&mut Tui`. The associated `Error` keeps this kit free of
/// any concrete error crate; it only needs to absorb I/O errors.
pub trait Screen {
    type Error: From<io::Error>;

    fn render(&self, frame: &mut Frame);

    fn handle_key(
        &mut self,
        key: KeyEvent,
        tui: &mut Tui,
    ) -> Result<Flow, Self::Error>;

    /// Called on each idle tick (no input within [`TICK`]); use it to advance
    /// animations. Default: do nothing.
    fn tick(&mut self) {}
}

/// Runs the draw/read/handle loop until the screen or the user quits.
///
/// Redraws every iteration; when no event arrives within [`TICK`], calls
/// [`Screen::tick`] so animated widgets keep moving.
///
/// # Errors
///
/// Propagates any error from drawing, reading input or the screen's handler.
pub fn run<S: Screen>(tui: &mut Tui, screen: &mut S) -> Result<(), S::Error> {
    loop {
        tui.draw(|frame| screen.render(frame))?;
        match tui.poll_event(TICK)? {
            None => screen.tick(),
            Some(TuiEvent::Quit) => break,
            Some(TuiEvent::Resize) => {}
            Some(TuiEvent::Key(key)) => {
                if let Flow::Quit = screen.handle_key(key, tui)? {
                    break;
                }
            }
        }
    }
    Ok(())
}
