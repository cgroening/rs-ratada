//! Launching an external editor (`$EDITOR`) on a piece of text via a temp file.
//!
//! The terminal is suspended around the editor process and restored afterwards
//! through [`Tui::suspend`].

use std::{
    fs::OpenOptions,
    io::{self, Write},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use super::terminal::Tui;

/// Resolves the editor command: `$VISUAL`, then `$EDITOR`, then `vi`.
pub fn resolve_editor() -> String {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string())
}

/// Edits `initial` in `editor` via a temp file, returning the text (trailing
/// newlines trimmed), or `None` when the editor could not be run.
///
/// # Errors
///
/// Returns an I/O error if the temp file or terminal cannot be handled.
pub fn edit_in_editor(
    tui: &mut Tui,
    editor: &str,
    initial: &str,
) -> io::Result<Option<String>> {
    // A pid plus a nanosecond stamp keeps the name unique; `create_new` then
    // refuses to open a pre-existing file or symlink, so we never write through
    // one an attacker planted at this predictable path.
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let path = std::env::temp_dir()
        .join(format!("ratada-{}-{unique}.txt", std::process::id()));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?
        .write_all(initial.as_bytes())?;

    let status = tui.suspend(|| Command::new(editor).arg(&path).status())?;

    let result = match status {
        Ok(code) if code.success() => {
            let text = std::fs::read_to_string(&path)?;
            Some(text.trim_end_matches('\n').to_string())
        }
        Ok(code) => {
            log::warn!("editor '{editor}' exited with {code}; discarding edit");
            None
        }
        Err(error) => {
            log::warn!("could not launch editor '{editor}': {error}");
            None
        }
    };
    if let Err(error) = std::fs::remove_file(&path) {
        log::debug!("could not remove temp file {}: {error}", path.display());
    }
    Ok(result)
}
