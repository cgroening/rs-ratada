//! Best-effort clipboard access via the platform's command-line tools.
//!
//! Spawning a small helper process avoids an extra dependency. Failures are
//! reported as `false`/`None` and logged, never propagated, so a missing
//! clipboard tool degrades gracefully instead of breaking the TUI.

use std::{
    io::Write,
    process::{Command, Stdio},
};

/// Copies `text` to the system clipboard. Returns whether it succeeded.
pub fn copy(text: &str) -> bool {
    for (program, args) in copy_candidates() {
        if pipe_to(program, args, text).is_ok() {
            return true;
        }
    }
    log::warn!("clipboard copy failed: no working clipboard tool found");
    false
}

/// Reads the system clipboard, or `None` if it cannot be read.
pub fn paste() -> Option<String> {
    for (program, args) in paste_candidates() {
        if let Some(text) = read_from(program, args) {
            return Some(text);
        }
    }
    None
}

fn pipe_to(program: &str, args: &[&str], text: &str) -> std::io::Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

fn read_from(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn copy_candidates() -> &'static [(&'static str, &'static [&'static str])] {
    #[cfg(target_os = "macos")]
    {
        &[("pbcopy", &[])]
    }
    #[cfg(target_os = "windows")]
    {
        &[("clip", &[])]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    }
}

fn paste_candidates() -> &'static [(&'static str, &'static [&'static str])] {
    #[cfg(target_os = "macos")]
    {
        &[("pbpaste", &[])]
    }
    #[cfg(target_os = "windows")]
    {
        &[("powershell", &["-NoProfile", "-Command", "Get-Clipboard"])]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        &[
            ("wl-paste", &["--no-newline"]),
            ("xclip", &["-selection", "clipboard", "-o"]),
            ("xsel", &["--clipboard", "--output"]),
        ]
    }
}
