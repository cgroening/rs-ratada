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

/// The dim rendering reaches every frame drawn through the guard, and
/// only while it is set. One test owns the thread-local.
#[test]
fn a_draw_resolves_dim_only_while_a_render_is_set() {
    use ratatui::style::{Color as RatColor, Modifier};

    use crate::dim::{DimRender, set_dim_render};

    let draw = || {
        let mut tui = Tui::for_test(1, 1).expect("in-memory terminal");
        tui.draw(|frame| {
            frame.buffer_mut().content[0].modifier = Modifier::DIM;
        })
        .expect("draw");
        let Backend::Test(backend) = tui.terminal.backend() else {
            panic!("for_test must build a test backend");
        };
        backend.buffer()[(0, 0)].clone()
    };

    assert!(draw().modifier.contains(Modifier::DIM));
    set_dim_render(Some(DimRender {
        foreground: crate::theme::Color::Rgb(1, 2, 3),
        amount: 0.5,
    }));
    let cell = draw();
    set_dim_render(None);

    assert_eq!(cell.fg, RatColor::Rgb(1, 2, 3));
    assert!(!cell.modifier.contains(Modifier::DIM));
}

/// A test guard must not run the real restore on drop: that would disable
/// raw mode for the whole test process and write escape sequences to the
/// developer's stdout, corrupting whatever else runs in it.
#[test]
fn dropping_a_test_guard_leaves_the_real_terminal_alone() {
    drop(Tui::for_test(4, 1).expect("in-memory terminal"));
    assert!(
        !crossterm::terminal::is_raw_mode_enabled().expect("raw mode state"),
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
    let event =
        Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
    assert!(matches!(classify(&event), Some(TuiEvent::Quit)));
}

/// A wheel scroll as the terminal reports it.
fn wheel_event(kind: crossterm::event::MouseEventKind) -> Event {
    Event::Mouse(crossterm::event::MouseEvent {
        kind,
        column: 3,
        row: 4,
        modifiers: KeyModifiers::NONE,
    })
}

/// The seam hands a vertical scroll on as the arrow key every list already
/// handles, and flags it as a wheel step.
#[test]
fn a_wheel_scroll_is_read_as_an_arrow_key_from_the_wheel() {
    use crossterm::event::MouseEventKind;

    script_events([
        wheel_event(MouseEventKind::ScrollDown),
        wheel_event(MouseEventKind::ScrollUp),
    ]);

    let down = read_raw_event().expect("a scripted event");
    assert!(is_from_wheel());
    let up = poll_raw_event(Duration::from_secs(0))
        .expect("a scripted event")
        .expect("not a timeout");
    assert!(is_from_wheel());

    assert_eq!(
        down,
        Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
    );
    assert_eq!(
        up,
        Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
    );
    clear_scripted_input();
}

/// The flag describes the event read last: a key after a scroll is a key,
/// and other mouse events pass the seam unchanged and unflagged.
#[test]
fn a_key_or_other_mouse_event_clears_the_wheel_flag() {
    use crossterm::event::{MouseButton, MouseEventKind};

    let click = wheel_event(MouseEventKind::Down(MouseButton::Left));
    let sideways = wheel_event(MouseEventKind::ScrollRight);
    script_events([
        wheel_event(MouseEventKind::ScrollDown),
        Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        wheel_event(MouseEventKind::ScrollDown),
        click.clone(),
        sideways.clone(),
    ]);

    let _ = read_raw_event().expect("a scripted event");
    let _ = read_raw_event().expect("a scripted event");
    assert!(!is_from_wheel(), "a key after a scroll");
    let _ = read_raw_event().expect("a scripted event");
    assert_eq!(read_raw_event().expect("a scripted event"), click);
    assert!(!is_from_wheel(), "a click");
    assert_eq!(read_raw_event().expect("a scripted event"), sideways);
    assert!(!is_from_wheel(), "a horizontal scroll");
    clear_scripted_input();
}

/// A classified wheel scroll is an ordinary key event, so `Tui::read_event`
/// and every loop over it see the arrow.
#[test]
fn classify_passes_a_translated_wheel_scroll_as_a_key() {
    use crossterm::event::MouseEventKind;

    script_events([wheel_event(MouseEventKind::ScrollDown)]);
    let event = read_raw_event().expect("a scripted event");
    clear_scripted_input();

    assert!(matches!(
        classify(&event),
        Some(TuiEvent::Key(key)) if key.code == KeyCode::Down,
    ));
}

/// Entering the screen asks for button and wheel reports only - no motion
/// tracking - and leaving turns off exactly that. One test owns the
/// thread-local setting. Windows goes through the console API instead.
#[cfg(not(windows))]
#[test]
fn reporting_follows_the_setting_and_is_undone_on_leave() {
    let run = |step: fn(&mut Vec<u8>) -> io::Result<()>| {
        let mut out = Vec::new();
        step(&mut out).expect("writing to a buffer");
        String::from_utf8(out).expect("escape sequences are ASCII")
    };
    let start = |out: &mut Vec<u8>| wheel::start_reporting(out);
    let stop = |out: &mut Vec<u8>| wheel::stop_reporting(out);

    assert_eq!(mouse_wheel(), MouseWheel::Reported, "the default");
    assert_eq!(run(start), "\x1b[?1000h\x1b[?1006h");
    assert_eq!(run(stop), "\x1b[?1006l\x1b[?1000l");
    assert_eq!(run(stop), "", "nothing left to turn off");

    set_mouse_wheel(MouseWheel::Terminal);
    assert_eq!(run(start), "", "the terminal keeps the mouse");
    assert_eq!(run(stop), "");
    set_mouse_wheel(MouseWheel::Reported);
}
