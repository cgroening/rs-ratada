//! RAII terminal guard and event reader.

use std::{
    cell::RefCell,
    collections::VecDeque,
    convert::Infallible,
    io::{self, Stdout},
    sync::Once,
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
// Bracketed paste is only wired up where crossterm can actually deliver it as
// an `Event::Paste`, which its Windows event source never does (see
// `enter_screen`).
#[cfg(not(windows))]
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use ratatui::{
    Frame, Terminal,
    backend::{ClearType, CrosstermBackend, TestBackend, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};

use crate::input;

/// What a [`Tui`] renders into: the real terminal, or an in-memory buffer.
///
/// An enum rather than a type parameter on [`Tui`]: every consuming app takes
/// `&mut Tui`, so a `Tui<B>` would ripple a generic through each of those
/// signatures to serve a case only tests need. The variant also *is* the fact
/// [`Tui::drop`] asks about - whether this guard ever took the screen - so no
/// separate flag has to be kept in sync with it.
enum Backend {
    /// The real terminal: raw mode plus alternate screen, restored on drop.
    Crossterm(CrosstermBackend<Stdout>),
    /// An in-memory buffer for tests, touching no real terminal.
    Test(TestBackend),
}

/// Widens a [`TestBackend`]'s [`Infallible`] error into the enum's `io::Error`.
///
/// The empty `match` is the proof rather than a claim: `Infallible` has no
/// variants, so there is no value to convert and no panic to reach.
fn never(error: Infallible) -> io::Error {
    match error {}
}

impl ratatui::backend::Backend for Backend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        match self {
            Backend::Crossterm(backend) => backend.draw(content),
            Backend::Test(backend) => backend.draw(content).map_err(never),
        }
    }

    fn append_lines(&mut self, lines: u16) -> io::Result<()> {
        match self {
            Backend::Crossterm(backend) => backend.append_lines(lines),
            Backend::Test(backend) => {
                backend.append_lines(lines).map_err(never)
            }
        }
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        match self {
            Backend::Crossterm(backend) => backend.hide_cursor(),
            Backend::Test(backend) => backend.hide_cursor().map_err(never),
        }
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        match self {
            Backend::Crossterm(backend) => backend.show_cursor(),
            Backend::Test(backend) => backend.show_cursor().map_err(never),
        }
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        match self {
            Backend::Crossterm(backend) => backend.get_cursor_position(),
            Backend::Test(backend) => {
                backend.get_cursor_position().map_err(never)
            }
        }
    }

    fn set_cursor_position<P: Into<Position>>(
        &mut self,
        position: P,
    ) -> io::Result<()> {
        match self {
            Backend::Crossterm(backend) => {
                backend.set_cursor_position(position)
            }
            Backend::Test(backend) => {
                backend.set_cursor_position(position).map_err(never)
            }
        }
    }

    fn clear(&mut self) -> io::Result<()> {
        match self {
            Backend::Crossterm(backend) => backend.clear(),
            Backend::Test(backend) => backend.clear().map_err(never),
        }
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        match self {
            Backend::Crossterm(backend) => backend.clear_region(clear_type),
            Backend::Test(backend) => {
                backend.clear_region(clear_type).map_err(never)
            }
        }
    }

    fn size(&self) -> io::Result<Size> {
        match self {
            Backend::Crossterm(backend) => backend.size(),
            Backend::Test(backend) => backend.size().map_err(never),
        }
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        match self {
            Backend::Crossterm(backend) => backend.window_size(),
            Backend::Test(backend) => backend.window_size().map_err(never),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Backend::Crossterm(backend) => backend.flush(),
            Backend::Test(backend) => backend.flush().map_err(never),
        }
    }
}

/// A terminal input event relevant to the app.
///
/// `Quit` is produced for the global `Ctrl+Q` so any loop (main or modal) can
/// exit, instead of threading a quit error through `Result`.
pub enum TuiEvent {
    /// A key press.
    Key(KeyEvent),
    /// A bracketed paste from the terminal, with newlines normalized to `\n`.
    ///
    /// Only produced on macOS and Linux. On Windows crossterm's event source
    /// emits key events only and never a paste, so a paste there arrives
    /// through the `Ctrl+V` key path (which reads the clipboard directly) and
    /// bracketed paste is left disabled - see `enter_screen`.
    Paste(String),
    /// The terminal was resized; the surface should redraw.
    Resize,
    /// The global quit chord (`Ctrl+Q`) was pressed.
    Quit,
}

/// Owns the alternate screen and raw mode for the lifetime of the TUI.
///
/// Enables raw mode and the alternate screen on creation and restores both on
/// `Drop`, so the terminal is always left clean, even on panic. A panic hook
/// (installed with the first guard) gives the terminal back before the panic
/// message is printed, so the message lands on the normal screen instead of
/// vanishing with the alternate one. The `on_enter`
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
        install_panic_restore();
        enable_raw_mode()?;
        let mut out = io::stdout();
        enter_screen(&mut out)?;
        set_holds_screen(true);
        let on_enter: Box<dyn Fn()> = Box::new(on_enter);
        let on_leave: Box<dyn Fn()> = Box::new(on_leave);
        on_enter();
        let terminal =
            Terminal::new(Backend::Crossterm(CrosstermBackend::new(out)))?;
        Ok(Self {
            terminal,
            on_enter,
            on_leave,
        })
    }

    /// Creates a guard over an in-memory `width` by `height` buffer for tests.
    ///
    /// Touches no real terminal: no raw mode, no alternate screen, no hooks,
    /// and `Drop` restores nothing. It exists so a consuming app can build the
    /// context its key handlers take and drive them from a test, which is
    /// otherwise impossible - the only other constructors reconfigure the
    /// developer's own terminal.
    ///
    /// Only terminal-free key paths can be exercised this way. A handler that
    /// opens a modal reaches [`Tui::read_event`], which blocks on real stdin
    /// regardless of the backend, and one that shells out reaches
    /// [`Tui::suspend`], which does reconfigure the real terminal.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the in-memory terminal cannot be created.
    pub fn for_test(width: u16, height: u16) -> io::Result<Self> {
        let backend = Backend::Test(TestBackend::new(width, height));
        Ok(Self {
            terminal: Terminal::new(backend)?,
            on_enter: Box::new(|| {}),
            on_leave: Box::new(|| {}),
        })
    }

    /// Renders one frame.
    ///
    /// A [`crate::dim::DimRender`] set on this thread is applied to the
    /// finished frame, so the terminal `DIM` attribute takes its colour.
    pub fn draw<F: FnOnce(&mut Frame)>(&mut self, render: F) -> io::Result<()> {
        self.terminal.draw(|frame| {
            render(frame);
            crate::dim::apply(frame.buffer_mut());
        })?;
        Ok(())
    }

    /// Blocks for the next key or resize event, skipping key releases.
    ///
    /// Reads through [`read_raw_event`], so a scripted queue drives it.
    pub fn read_event(&self) -> io::Result<TuiEvent> {
        loop {
            let event = read_raw_event()?;
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
        match poll_raw_event(timeout)? {
            Some(event) => Ok(classify(&event)),
            None => Ok(None),
        }
    }

    /// Restores the terminal, runs `action` (e.g. an external editor), then
    /// re-enters the alternate screen and clears the canvas.
    pub fn suspend<T>(&mut self, action: impl FnOnce() -> T) -> io::Result<T> {
        restore()?;
        set_holds_screen(false);
        (self.on_leave)();
        let result = action();
        enable_raw_mode()?;
        enter_screen(&mut io::stdout())?;
        set_holds_screen(true);
        (self.on_enter)();
        self.terminal.clear()?;
        Ok(result)
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        // A test guard never took the screen, so it has nothing to give back.
        // Restoring anyway would disable raw mode for the whole test process
        // and write escape sequences to the real stdout.
        if matches!(self.terminal.backend(), Backend::Test(_)) {
            return;
        }
        // Drop cannot return the error, so log it: a failed restore leaves the
        // terminal in raw mode / the alternate screen, which the user must
        // otherwise recover blindly.
        if let Err(error) = restore() {
            log::error!("failed to restore the terminal on exit: {error}");
        }
        set_holds_screen(false);
        (self.on_leave)();
    }
}

thread_local! {
    /// Whether this thread holds the terminal (raw mode and the alternate
    /// screen) through a live [`Tui`]. Only then does a panic here give the
    /// terminal back: a panic on a worker thread leaves the running TUI alone.
    static HOLDS_SCREEN: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

/// Notes whether this thread holds the terminal.
fn set_holds_screen(holds: bool) {
    HOLDS_SCREEN.with(|held| held.set(holds));
}

/// Installs, once per process, a panic hook that restores the terminal
/// before the previous hook prints the message. Without it the message is
/// written to the alternate screen, which the guard's `Drop` then leaves -
/// the panic looks like a silent exit. The guard's later restore finds
/// nothing left to do. A host that catches a panic and carries on must take
/// the screen again (drop the guard and create a new one).
fn install_panic_restore() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if HOLDS_SCREEN.with(std::cell::Cell::get) {
                set_holds_screen(false);
                // Best effort: a panic is already under way
                let _ = restore();
                let _ = execute!(io::stdout(), crossterm::cursor::Show);
            }
            previous(info);
        }));
    });
}

thread_local! {
    /// This thread's scripted input, or `None` when reads go to the terminal.
    ///
    /// Thread-local rather than a global lock because the test harness runs
    /// each `#[test]` on its own thread: a per-thread queue needs no locking
    /// and cannot leak between tests running in parallel.
    static SCRIPT: RefCell<Option<VecDeque<Event>>> = const {
        RefCell::new(None)
    };
}

/// Installs a scripted key sequence for this thread, replacing any previous
/// one, so a test can answer a modal instead of blocking on stdin.
///
/// Every reader in this crate draws from it, and so does any consuming app
/// that reads through [`read_raw_event`] - one ordered queue, which is what
/// lets a single keypress cross an app's own loop and a modal of this crate's
/// in the right order.
///
/// Once installed, a queue that runs dry is an **error**, never a fall back to
/// the terminal: an under-fed test must fail in milliseconds rather than hang
/// waiting for a key nobody will press. Call [`clear_scripted_input`] to go
/// back to the real terminal.
pub fn script_keys(keys: impl IntoIterator<Item = KeyEvent>) {
    let queued = keys.into_iter().map(Event::Key).collect();
    SCRIPT.with(|script| *script.borrow_mut() = Some(queued));
}

/// Like [`script_keys`], but queues any terminal events - a bracketed
/// [`Event::Paste`] between keys, say - so a test can drive a host's paste
/// handling the way a terminal delivers it.
pub fn script_events(events: impl IntoIterator<Item = Event>) {
    let queued = events.into_iter().collect();
    SCRIPT.with(|script| *script.borrow_mut() = Some(queued));
}

/// Removes this thread's scripted input, so reads go to the terminal again.
pub fn clear_scripted_input() {
    SCRIPT.with(|script| *script.borrow_mut() = None);
}

/// How many scripted events are still queued, or `None` when no script is
/// installed.
///
/// A test asserts on this to prove the keys it queued were actually consumed:
/// a leftover answer means the modal never opened, which would otherwise pass
/// as a green test proving nothing.
#[must_use]
pub fn scripted_remaining() -> Option<usize> {
    SCRIPT.with(|script| script.borrow().as_ref().map(VecDeque::len))
}

/// Takes the next scripted event, or reports that the script is installed but
/// exhausted. `Ok(None)` means no script is installed at all.
fn next_scripted_event() -> io::Result<Option<Event>> {
    SCRIPT.with(|script| match script.borrow_mut().as_mut() {
        None => Ok(None),
        Some(queue) => queue.pop_front().map(Some).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "scripted input exhausted",
            )
        }),
    })
}

/// The next terminal event: from this thread's scripted queue when one is
/// installed, else from the real terminal.
///
/// The single seam through which input enters. A consuming app with its own
/// event source must read through this rather than calling crossterm, or its
/// loops cannot be driven from a test - and, because the queue is shared, its
/// keys would fall out of order against this crate's modals.
///
/// # Errors
///
/// Returns an I/O error when the terminal cannot be read, or `UnexpectedEof`
/// when a scripted queue is exhausted.
pub fn read_raw_event() -> io::Result<Event> {
    match next_scripted_event()? {
        Some(event) => Ok(event),
        None => event::read(),
    }
}

/// Like [`read_raw_event`], but gives up after `timeout`.
///
/// A scripted queue answers at once either way: it either has an event or is
/// exhausted, so it never waits out the timeout.
///
/// # Errors
///
/// As [`read_raw_event`].
pub fn poll_raw_event(timeout: Duration) -> io::Result<Option<Event>> {
    if let Some(event) = next_scripted_event()? {
        return Ok(Some(event));
    }
    if event::poll(timeout)? {
        return event::read().map(Some);
    }
    Ok(None)
}

/// Maps a crossterm event to a [`TuiEvent`], or `None` for events the app
/// ignores (key releases, mouse, focus).
fn classify(event: &Event) -> Option<TuiEvent> {
    match event {
        Event::Resize(_, _) => Some(TuiEvent::Resize),
        Event::Paste(text) => Some(TuiEvent::Paste(normalize_newlines(text))),
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

/// Collapses `\r\n` and lone `\r` line endings to `\n`.
///
/// Bracketed pastes carry whatever line endings the source used (Windows text
/// arrives as `\r\n`); normalizing here means every consumer sees `\n`-only,
/// regardless of the platform the clipboard content came from.
pub(crate) fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Whether `key` is the app-wide quit chord.
///
/// Goes through `is_command`, not a bare CONTROL check, so `AltGr` (reported as
/// `Control + Alt`) can never quit. No German `AltGr` glyph is a `q` today, so
/// this cannot fire by accident - but the rule should not depend on that.
fn is_global_quit(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('q') && input::is_command(*key)
}

/// Enters the alternate screen, additionally enabling bracketed paste where
/// crossterm can deliver it.
///
/// crossterm's Windows event source reads console key records and never emits
/// an `Event::Paste`, so enabling bracketed paste there would only make the
/// terminal send `\e[200~ … \e[201~` sequences the app cannot parse (they
/// surface as mangled key events). On Windows a paste therefore comes through
/// the `Ctrl+V` key path instead, which reads the clipboard directly.
fn enter_screen(out: &mut Stdout) -> io::Result<()> {
    execute!(out, EnterAlternateScreen)?;
    #[cfg(not(windows))]
    execute!(out, EnableBracketedPaste)?;
    Ok(())
}

/// Leaves the alternate screen, mirroring [`enter_screen`] by disabling
/// bracketed paste only where it was enabled.
fn leave_screen(out: &mut Stdout) -> io::Result<()> {
    #[cfg(not(windows))]
    execute!(out, DisableBracketedPaste)?;
    execute!(out, LeaveAlternateScreen)?;
    Ok(())
}

fn restore() -> io::Result<()> {
    leave_screen(&mut io::stdout())?;
    disable_raw_mode()
}

#[cfg(test)]
mod tests;
