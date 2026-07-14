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
    log::warn!("clipboard paste failed: no working clipboard tool found");
    None
}

fn pipe_to(program: &str, args: &[&str], text: &str) -> std::io::Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    // Close stdin before waiting: tools that read to EOF (e.g. PowerShell's
    // `[Console]::In.ReadToEnd()`) block until the pipe closes, so holding the
    // handle open across `wait` would deadlock.
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        log::debug!("clipboard tool '{program}' exited with {status}");
    }
    Ok(())
}

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
        Ok(text) => Some(strip_bom(&text).to_string()),
        Err(error) => {
            log::debug!(
                "clipboard tool '{program}' returned invalid UTF-8: {error}"
            );
            None
        }
    }
}

/// Removes a leading UTF-8 byte-order mark, if present.
///
/// Forcing a Windows tool's output encoding to UTF-8 can prepend a BOM
/// (`U+FEFF`); it must not leak into pasted text as a stray leading character.
fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// PowerShell one-liner that stores stdin verbatim in the clipboard.
///
/// Forcing `InputEncoding` to UTF-8 without a BOM lets it read the UTF-8 bytes
/// we pipe in correctly, where `clip` would misread them as the OEM code page
/// and corrupt umlauts. `ReadToEnd` keeps the text (order, line breaks) whole.
#[cfg(target_os = "windows")]
const WINDOWS_COPY_SCRIPT: &str = concat!(
    "[Console]::InputEncoding = ",
    "[System.Text.UTF8Encoding]::new($false); ",
    "Set-Clipboard -Value ([Console]::In.ReadToEnd())",
);

/// PowerShell one-liner that reads the clipboard as one UTF-8 string.
///
/// `Get-Clipboard -Raw` returns the whole clipboard verbatim (order and line
/// breaks intact); forcing `OutputEncoding` to UTF-8 without a BOM and writing
/// through `[Console]::Out` avoids both the OEM-codepage mojibake and the
/// trailing newline the normal output stream would append.
#[cfg(target_os = "windows")]
const WINDOWS_PASTE_SCRIPT: &str = concat!(
    "[Console]::OutputEncoding = ",
    "[System.Text.UTF8Encoding]::new($false); ",
    "[Console]::Out.Write([string](Get-Clipboard -Raw))",
);

fn copy_candidates() -> &'static [(&'static str, &'static [&'static str])] {
    #[cfg(target_os = "macos")]
    {
        &[("pbcopy", &[])]
    }
    #[cfg(target_os = "windows")]
    {
        &[(
            "powershell",
            &["-NoProfile", "-Command", WINDOWS_COPY_SCRIPT],
        )]
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
        &[(
            "powershell",
            &["-NoProfile", "-Command", WINDOWS_PASTE_SCRIPT],
        )]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_bom_removes_a_leading_byte_order_mark() {
        assert_eq!(strip_bom("\u{feff}hello"), "hello");
    }

    #[test]
    fn strip_bom_leaves_text_without_a_mark_untouched() {
        assert_eq!(strip_bom("hello"), "hello");
    }

    #[test]
    fn strip_bom_only_removes_a_leading_mark() {
        assert_eq!(strip_bom("a\u{feff}b"), "a\u{feff}b");
    }
}
