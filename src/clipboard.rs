//! Best-effort clipboard access.
//!
//! On Windows this talks to the native Win32 clipboard (via `clipboard-win`),
//! so a copy/paste is instant and returns correct Unicode - no `PowerShell`
//! subprocess, no OEM-codepage mojibake, no BOM. On macOS and Linux it spawns
//! the platform's small clipboard tool (`pbcopy`/`pbpaste`,
//! `wl-copy`/`xclip`/`xsel`), which is fast enough and needs no dependency.
//!
//! Failures are reported as `false`/`None` and logged, never propagated, so a
//! missing clipboard tool degrades gracefully instead of breaking the TUI.
//!
//! Line endings: a paste is normalised to `\n`; on Windows a copy writes
//! `\r\n`, which Windows programs (SAP GUI, Excel) need to paste several
//! lines as separate rows instead of one cell.

/// Copies `text` to the system clipboard. Returns whether it succeeded.
pub fn copy(text: &str) -> bool {
    copy_impl(text)
}

/// Reads the system clipboard, or `None` if it cannot be read.
pub fn paste() -> Option<String> {
    paste_impl()
}

// ---------------------------------------------------------------------------
// Windows: native Win32 clipboard.
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn copy_impl(text: &str) -> bool {
    match clipboard_win::set_clipboard_string(&to_crlf(text)) {
        Ok(()) => true,
        Err(error) => {
            log::warn!("clipboard copy failed: {error}");
            false
        }
    }
}

#[cfg(windows)]
fn paste_impl() -> Option<String> {
    match clipboard_win::get_clipboard_string() {
        Ok(text) => Some(crate::terminal::normalize_newlines(&text)),
        Err(error) => {
            log::warn!("clipboard paste failed: {error}");
            None
        }
    }
}

/// `text` with every line ending as `\r\n` (an existing `\r\n` is not
/// doubled, a lone `\r` or `\n` becomes `\r\n`).
#[cfg(any(windows, test))]
fn to_crlf(text: &str) -> String {
    crate::terminal::normalize_newlines(text).replace('\n', "\r\n")
}

// ---------------------------------------------------------------------------
// macOS / Linux: command-line clipboard tools.
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[cfg(not(windows))]
fn copy_impl(text: &str) -> bool {
    for (program, args) in copy_candidates() {
        if pipe_to(program, args, text).is_ok() {
            return true;
        }
    }
    log::warn!("clipboard copy failed: no working clipboard tool found");
    false
}

#[cfg(not(windows))]
fn paste_impl() -> Option<String> {
    for (program, args) in paste_candidates() {
        if let Some(text) = read_from(program, args) {
            return Some(text);
        }
    }
    log::warn!("clipboard paste failed: no working clipboard tool found");
    None
}

#[cfg(not(windows))]
fn pipe_to(program: &str, args: &[&str], text: &str) -> std::io::Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    // Close stdin before waiting: tools that read to EOF block until the pipe
    // closes, so holding the handle open across `wait` would deadlock.
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        log::debug!("clipboard tool '{program}' exited with {status}");
    }
    Ok(())
}

#[cfg(not(windows))]
fn read_from(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        log::debug!("clipboard tool '{program}' exited with {}", output.status);
        return None;
    }
    match String::from_utf8(output.stdout) {
        // Normalized like every other paste seam: an X11/Wayland selection can
        // carry CRLF just as the Windows clipboard does (anything copied out of
        // a Windows VM, an editor set to CRLF, or a browser text area), and a
        // stray `\r` would reach a text field as a control character.
        Ok(text) => Some(crate::terminal::normalize_newlines(&text)),
        Err(error) => {
            log::debug!(
                "clipboard tool '{program}' returned invalid UTF-8: {error}"
            );
            None
        }
    }
}

#[cfg(not(windows))]
fn copy_candidates() -> &'static [(&'static str, &'static [&'static str])] {
    #[cfg(target_os = "macos")]
    {
        &[("pbcopy", &[])]
    }
    #[cfg(not(target_os = "macos"))]
    {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    }
}

#[cfg(not(windows))]
fn paste_candidates() -> &'static [(&'static str, &'static [&'static str])] {
    #[cfg(target_os = "macos")]
    {
        &[("pbpaste", &[])]
    }
    #[cfg(not(target_os = "macos"))]
    {
        &[
            ("wl-paste", &["--no-newline"]),
            ("xclip", &["-selection", "clipboard", "-o"]),
            ("xsel", &["--clipboard", "--output"]),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::to_crlf;
    use crate::terminal::normalize_newlines;

    #[test]
    fn to_crlf_ends_every_line_with_crlf() {
        assert_eq!(to_crlf("a\nb\nc"), "a\r\nb\r\nc");
        assert_eq!(to_crlf("a\rb"), "a\r\nb");
    }

    #[test]
    fn to_crlf_keeps_crlf_and_text_without_breaks() {
        assert_eq!(to_crlf("a\r\nb"), "a\r\nb");
        assert_eq!(to_crlf("no breaks here"), "no breaks here");
    }

    #[test]
    fn normalize_newlines_collapses_crlf_and_lone_cr() {
        assert_eq!(normalize_newlines("a\r\nb\rc\nd"), "a\nb\nc\nd");
    }

    #[test]
    fn normalize_newlines_leaves_plain_text_untouched() {
        assert_eq!(normalize_newlines("no breaks here"), "no breaks here");
    }
}
