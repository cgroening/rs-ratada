//! RAII terminal guard and event reader.

use std::{
    io::{self, Stdout},
    time::Duration,
};

use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode,
        KeyEvent, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use ratatui::{Frame, Terminal, backend::CrosstermBackend};

type Backend = CrosstermBackend<Stdout>;

/// A terminal input event relevant to the app.
///
/// `Quit` is produced for the global `Ctrl+Q` so any loop (main or modal) can
/// exit, instead of threading a quit error through `Result`.
pub enum TuiEvent {
    Key(KeyEvent),
    Resize,
    Quit,
}

/// Owns the alternate screen and raw mode for the lifetime of the TUI.
///
/// Enables raw mode and the alternate screen on creation and restores both on
/// `Drop`, so the terminal is always left clean, even on panic. The `on_enter`
/// and `on_leave` hooks fire whenever the TUI takes or releases the screen,
/// letting the host mute side effects (e.g. stderr logging) without coupling
/// this guard to the application.
pub struct Tui {
    terminal: Terminal<Backend>,
    on_enter: Box<dyn Fn()>,
    on_leave: Box<dyn Fn()>,
}

impl Tui {
    /// Enters raw mode and the alternate screen with no lifecycle hooks.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the terminal cannot be reconfigured.
    pub fn new() -> io::Result<Self> {
        Self::with_hooks(|| {}, || {})
    }

    /// Like [`Tui::new`], but runs `on_enter` whenever the TUI acquires the
    /// screen and `on_leave` whenever it releases it (drop and `suspend`).
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the terminal cannot be reconfigured.
    pub fn with_hooks(
        on_enter: impl Fn() + 'static,
        on_leave: impl Fn() + 'static,
    ) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen, EnableBracketedPaste)?;
        let on_enter: Box<dyn Fn()> = Box::new(on_enter);
        let on_leave: Box<dyn Fn()> = Box::new(on_leave);
        on_enter();
        let terminal = Terminal::new(CrosstermBackend::new(out))?;
        Ok(Self {
            terminal,
            on_enter,
            on_leave,
        })
    }

    /// Renders one frame.
    pub fn draw<F: FnOnce(&mut Frame)>(&mut self, render: F) -> io::Result<()> {
        self.terminal.draw(render)?;
        Ok(())
    }

    /// Blocks for the next key or resize event, skipping key releases.
    pub fn read_event(&self) -> io::Result<TuiEvent> {
        loop {
            let event = event::read()?;
            if let Some(classified) = classify(&event) {
                return Ok(classified);
            }
        }
    }

    /// Like [`Tui::read_event`] but waits at most `timeout`; returns `None` on
    /// timeout (or on an ignored event), so callers can drive animations.
    pub fn poll_event(
        &self,
        timeout: Duration,
    ) -> io::Result<Option<TuiEvent>> {
        if event::poll(timeout)? {
            let event = event::read()?;
            return Ok(classify(&event));
        }
        Ok(None)
    }

    /// Restores the terminal, runs `action` (e.g. an external editor), then
    /// re-enters the alternate screen and clears the canvas.
    pub fn suspend<T>(&mut self, action: impl FnOnce() -> T) -> io::Result<T> {
        restore()?;
        (self.on_leave)();
        let result = action();
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
        (self.on_enter)();
        self.terminal.clear()?;
        Ok(result)
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        // Drop cannot return the error, so log it: a failed restore leaves the
        // terminal in raw mode / the alternate screen, which the user must
        // otherwise recover blindly.
        if let Err(error) = restore() {
            log::error!("failed to restore the terminal on exit: {error}");
        }
        (self.on_leave)();
    }
}

/// Maps a crossterm event to a [`TuiEvent`], or `None` for events the app
/// ignores (key releases, mouse, focus, paste).
fn classify(event: &Event) -> Option<TuiEvent> {
    match event {
        Event::Resize(_, _) => Some(TuiEvent::Resize),
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if is_global_quit(key) {
                Some(TuiEvent::Quit)
            } else {
                Some(TuiEvent::Key(*key))
            }
        }
        _ => None,
    }
}

fn is_global_quit(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('q')
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn restore() -> io::Result<()> {
    execute!(io::stdout(), LeaveAlternateScreen, DisableBracketedPaste)?;
    disable_raw_mode()
}
