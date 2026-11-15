//! The mouse wheel: whether the terminal reports it, and how a reported
//! scroll enters the input seam.
//!
//! Without mouse reporting, a terminal in the alternate screen turns each
//! wheel notch into `↑`/`↓` presses (alternate scroll mode) that cannot be
//! told apart from the keys, so a list that wraps on `↓` wraps on the wheel
//! too and jumps at its ends. With reporting on, a scroll arrives as its own
//! event; [`admit`] hands it on as the arrow key every list already handles
//! and notes that it came from the wheel, so [`crate::nav::step`] clamps
//! where a key wraps.

use std::{cell::Cell, io};

#[cfg(windows)]
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::{
    event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind},
    execute,
};

/// Who handles the mouse wheel while a [`crate::Tui`] holds the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseWheel {
    /// The terminal reports the wheel and the buttons (never mere motion) to
    /// the app. A scroll moves the cursor one row and stops at the ends of a
    /// list that wraps on the arrow keys. Selecting text with the mouse then
    /// needs the terminal's modifier key (Option in iTerm2, Shift in most
    /// others).
    #[default]
    Reported,
    /// The terminal keeps the mouse: text selection needs no modifier, and
    /// the wheel arrives as the terminal sends it - in the alternate screen
    /// usually as `↑`/`↓` presses, on which a cyclic list wraps.
    Terminal,
}

thread_local! {
    /// The active setting. Per thread, like the quit policy: a TUI owns one
    /// terminal and drives it from one thread.
    static MOUSE_WHEEL: Cell<MouseWheel> = const {
        Cell::new(MouseWheel::Reported)
    };

    /// Whether this thread turned mouse reporting on, so leaving the screen
    /// turns off exactly what entering it turned on, even if the setting
    /// changed in between.
    static IS_REPORTING: Cell<bool> = const { Cell::new(false) };

    /// Whether the event last read through the input seam was a wheel
    /// scroll.
    static IS_FROM_WHEEL: Cell<bool> = const { Cell::new(false) };
}

/// The active mouse-wheel setting.
pub fn mouse_wheel() -> MouseWheel {
    MOUSE_WHEEL.with(Cell::get)
}

/// Sets who handles the mouse wheel.
///
/// It takes effect the next time this thread takes the screen - when a
/// [`crate::Tui`] is created or returns from [`crate::Tui::suspend`] - so a
/// host sets it before creating its `Tui`.
///
/// # Examples
///
/// ```
/// use ratada::terminal::{MouseWheel, mouse_wheel, set_mouse_wheel};
///
/// assert_eq!(mouse_wheel(), MouseWheel::Reported);
/// set_mouse_wheel(MouseWheel::Terminal);
/// assert_eq!(mouse_wheel(), MouseWheel::Terminal);
/// ```
pub fn set_mouse_wheel(wheel: MouseWheel) {
    MOUSE_WHEEL.with(|setting| setting.set(wheel));
}

/// Whether the event last read through [`super::read_raw_event`] or
/// [`super::poll_raw_event`] on this thread was a wheel scroll (handed on as
/// an `↑`/`↓` key).
///
/// [`crate::nav::step`] covers the common case; this is for a host whose own
/// navigation does more at a list's end than wrap, and must not do it for
/// the wheel.
pub fn is_from_wheel() -> bool {
    IS_FROM_WHEEL.with(Cell::get)
}

/// Passes `event` through the input seam: a vertical wheel scroll becomes
/// the arrow key it stands for, and the wheel flag records whether `event`
/// was one.
pub(super) fn admit(event: Event) -> Event {
    let arrow = wheel_arrow(&event);
    IS_FROM_WHEEL.with(|flag| flag.set(arrow.is_some()));
    match arrow {
        Some(code) => Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
        None => event,
    }
}

/// Notes that a poll timed out, so no wheel step is pending any more.
pub(super) fn admit_timeout() {
    IS_FROM_WHEEL.with(|flag| flag.set(false));
}

/// Turns mouse reporting on when the setting asks for it.
pub(super) fn start_reporting(out: &mut impl io::Write) -> io::Result<()> {
    if mouse_wheel() == MouseWheel::Terminal {
        return Ok(());
    }
    #[cfg(not(windows))]
    execute!(out, EnableWheelReporting)?;
    #[cfg(windows)]
    execute!(out, EnableMouseCapture)?;
    IS_REPORTING.with(|reporting| reporting.set(true));
    Ok(())
}

/// Turns mouse reporting off again if [`start_reporting`] turned it on.
pub(super) fn stop_reporting(out: &mut impl io::Write) -> io::Result<()> {
    if !IS_REPORTING.with(|reporting| reporting.replace(false)) {
        return Ok(());
    }
    #[cfg(not(windows))]
    execute!(out, DisableWheelReporting)?;
    #[cfg(windows)]
    execute!(out, DisableMouseCapture)?;
    Ok(())
}

/// The arrow key a vertical wheel scroll stands for; `None` for any other
/// event, a horizontal scroll included.
fn wheel_arrow(event: &Event) -> Option<KeyCode> {
    let Event::Mouse(mouse) = event else {
        return None;
    };
    match mouse.kind {
        MouseEventKind::ScrollUp => Some(KeyCode::Up),
        MouseEventKind::ScrollDown => Some(KeyCode::Down),
        _ => None,
    }
}

/// Button tracking (`?1000`) in SGR encoding (`?1006`), which carries the
/// wheel. Narrower than crossterm's `EnableMouseCapture`, which also turns
/// on motion tracking (`?1002`, `?1003`): every mouse move would then wake
/// each polling loop for an event nobody reads. Windows uses crossterm's
/// command, which goes through the console API instead.
#[cfg(not(windows))]
struct EnableWheelReporting;

#[cfg(not(windows))]
impl crossterm::Command for EnableWheelReporting {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        f.write_str("\x1b[?1000h\x1b[?1006h")
    }
}

/// The inverse of [`EnableWheelReporting`], in reverse order.
#[cfg(not(windows))]
struct DisableWheelReporting;

#[cfg(not(windows))]
impl crossterm::Command for DisableWheelReporting {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        f.write_str("\x1b[?1006l\x1b[?1000l")
    }
}
