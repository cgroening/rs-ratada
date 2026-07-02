//! Launching an external editor (`$EDITOR`) on a piece of text via a temp file.
//!
//! The terminal is suspended around the editor process and restored afterwards
//! through [`Tui::suspend`].

use std::{
    io::{self, Write},
    process::Command,
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
/// Returns an I/O error if the temp file or terminal cannot be handled.
pub fn edit_in_editor(
    tui: &mut Tui,
    editor: &str,
    initial: &str,
) -> io::Result<Option<String>> {
    let path = std::env::temp_dir()
        .join(format!("clibase-{}.txt", std::process::id()));
    std::fs::File::create(&path)?.write_all(initial.as_bytes())?;

    let status = tui.suspend(|| Command::new(editor).arg(&path).status())?;

    let result = match status {
        Ok(code) if code.success() => {
            let text = std::fs::read_to_string(&path)?;
            Some(text.trim_end_matches('\n').to_string())
        }
        _ => None,
    };
    let _ = std::fs::remove_file(&path);
    Ok(result)
}
