//! RAII terminal guard and event reader.

use std::{
    cell::RefCell,
    collections::VecDeque,
    convert::Infallible,
    io::{self, Stdout},
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
        enter_screen(&mut out)?;
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
    pub fn draw<F: FnOnce(&mut Frame)>(&mut self, render: F) -> io::Result<()> {
        self.terminal.draw(render)?;
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
        (self.on_leave)();
        let result = action();
        enable_raw_mode()?;
        enter_screen(&mut io::stdout())?;
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
        (self.on_leave)();
    }
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
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    /// The point of the test backend: a guard a test can build and draw
    /// through, without a terminal to reconfigure.
    #[test]
    fn a_test_guard_renders_into_its_own_buffer() {
        let mut tui = Tui::for_test(12, 2).expect("in-memory terminal");
        tui.draw(|frame| {
            frame.render_widget(
                ratatui::widgets::Paragraph::new("hello"),
                frame.area(),
            );
        })
        .expect("draw");
        let Backend::Test(backend) = tui.terminal.backend() else {
            panic!("for_test must build a test backend");
        };
        let rendered: String =
            (0..12).map(|x| backend.buffer()[(x, 0)].symbol()).collect();
        assert_eq!(rendered.trim_end(), "hello");
    }

    /// A test guard must not run the real restore on drop: that would disable
    /// raw mode for the whole test process and write escape sequences to the
    /// developer's stdout, corrupting whatever else runs in it.
    #[test]
    fn dropping_a_test_guard_leaves_the_real_terminal_alone() {
        drop(Tui::for_test(4, 1).expect("in-memory terminal"));
        assert!(
            !crossterm::terminal::is_raw_mode_enabled()
                .expect("raw mode state"),
        );
    }

    /// A scripted key is handed back in the order it was queued, so a test can
    /// answer a modal's prompts one after another.
    #[test]
    fn scripted_keys_are_read_back_in_order() {
        script_keys([
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        ]);

        let first = read_raw_event().expect("a scripted event");
        let second = read_raw_event().expect("a scripted event");

        assert_eq!(
            first,
            Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        );
        assert_eq!(
            second,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        );
        clear_scripted_input();
    }

    /// The property the whole seam rests on: an exhausted script must fail, not
    /// fall back to the terminal. A test that queues too few keys has to fail
    /// in milliseconds instead of hanging forever on a key nobody will press.
    ///
    /// This test would hang rather than fail if the rule were broken, which is
    /// exactly the failure it guards against.
    #[test]
    fn an_exhausted_script_errors_instead_of_reading_the_terminal() {
        script_keys([]);

        let error = read_raw_event().expect_err("an exhausted script");

        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        clear_scripted_input();
    }

    /// The timeout variant must answer from the script too, or a form loop
    /// polling for its toast expiry would fall through to the real terminal.
    #[test]
    fn polling_answers_from_the_script_without_waiting() {
        script_keys([KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)]);

        let polled = poll_raw_event(Duration::from_secs(0))
            .expect("a scripted event")
            .expect("not a timeout");

        assert_eq!(
            polled,
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        );
        clear_scripted_input();
    }

    /// `scripted_remaining` is how a test proves its answers were consumed;
    /// it must distinguish "no script" from "script, now empty".
    #[test]
    fn the_remaining_count_tracks_consumption_and_clearing() {
        assert_eq!(scripted_remaining(), None, "no script by default");

        script_keys([KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)]);
        assert_eq!(scripted_remaining(), Some(1));

        let _ = read_raw_event().expect("a scripted event");
        assert_eq!(scripted_remaining(), Some(0), "installed but drained");

        clear_scripted_input();
        assert_eq!(scripted_remaining(), None, "back to the terminal");
    }

    #[test]
    fn normalize_newlines_collapses_crlf_and_lone_cr() {
        assert_eq!(normalize_newlines("a\r\nb\rc\nd"), "a\nb\nc\nd");
    }

    #[test]
    fn classify_paste_normalizes_newlines() {
        let event = Event::Paste("a\r\nb".to_string());
        match classify(&event) {
            Some(TuiEvent::Paste(text)) => assert_eq!(text, "a\nb"),
            _ => panic!("expected a normalized paste event"),
        }
    }

    /// The quit is `Ctrl+Q` and nothing else. `Ctrl+Alt+Q` is `AltGr+Q`, which
    /// types a character on some layouts and must never quit - that no German
    /// `AltGr` glyph happens to be a `q` is luck, not a rule to lean on.
    #[test]
    fn only_a_bare_ctrl_q_is_the_global_quit() {
        let quit = |modifiers| KeyEvent::new(KeyCode::Char('q'), modifiers);
        assert!(is_global_quit(&quit(KeyModifiers::CONTROL)));
        assert!(!is_global_quit(&quit(
            KeyModifiers::CONTROL | KeyModifiers::ALT
        )));
        assert!(!is_global_quit(&quit(KeyModifiers::NONE)));
        assert!(!is_global_quit(&KeyEvent::new(
            KeyCode::Char('s'),
            KeyModifiers::CONTROL,
        )));
    }

    /// `shortcut_hints::global_bindings` is what a host splices into its footer
    /// and help overlay, so it is a second, hand-maintained statement of a rule
    /// this module enforces. This ties the two together: the advertised chord,
    /// parsed back into a key, must be the one actually intercepted. A comment
    /// asking to keep them in sync would go stale silently; this fails.
    #[test]
    fn the_advertised_quit_chord_is_the_one_that_quits() {
        let (chord, _) = crate::shortcut_hints::global_bindings()
            .into_iter()
            .find(|(_, label)| label.contains("quit"))
            .expect("the global bindings name a quit chord");
        let key = crate::keymap::KeyChord::parse(&chord)
            .expect("the advertised chord parses")
            .to_key();
        assert!(
            is_global_quit(&key),
            "global_bindings advertises {chord:?}, which does not quit"
        );
    }

    /// The quit is routed before any host or widget sees the key, which is why
    /// no key handler needs to guard against it.
    #[test]
    fn classify_turns_ctrl_q_into_a_quit_event() {
        let event = Event::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL,
        ));
        assert!(matches!(classify(&event), Some(TuiEvent::Quit)));
    }
}
