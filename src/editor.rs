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

/// Edits `initial` in `editor` via a plain-text temp file, returning the text
/// (trailing newlines trimmed), or `None` when the editor could not be run.
///
/// Use [`edit_in_editor_as`] when the text has a syntax the editor should
/// recognise.
///
/// # Errors
///
/// Returns an I/O error if the temp file or terminal cannot be handled.
pub fn edit_in_editor(
    tui: &mut Tui,
    editor: &str,
    initial: &str,
) -> io::Result<Option<String>> {
    edit_in_editor_as(tui, editor, initial, "txt")
}

/// Like [`edit_in_editor`], but the temp file carries `extension` (without a
/// leading dot), so the editor picks the right syntax and filetype settings -
/// `md` for Markdown, `rs` for Rust.
///
/// `extension` is a bare suffix, not a path fragment: it must be non-empty and
/// ASCII alphanumeric, so it cannot escape the temp directory.
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidInput`] if `extension` is empty or holds
/// anything but ASCII alphanumerics, and an I/O error if the temp file or
/// terminal cannot be handled.
pub fn edit_in_editor_as(
    tui: &mut Tui,
    editor: &str,
    initial: &str,
    extension: &str,
) -> io::Result<Option<String>> {
    check_extension(extension)?;

    // A pid plus a nanosecond stamp keeps the name unique; `create_new` then
    // refuses to open a pre-existing file or symlink, so we never write through
    // one an attacker planted at this predictable path.
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "ratada-{}-{unique}.{extension}",
        std::process::id(),
    ));
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

/// Rejects an extension that is not a bare ASCII-alphanumeric suffix.
///
/// It lands in a file name unescaped, so a separator or a `..` would place the
/// temp file outside `temp_dir()`. `create_new` guards the target file, not the
/// path that leads to it. An empty extension is rejected too - `chars().all()`
/// holds vacuously on `""`, which would yield a trailing-dot name.
fn check_extension(extension: &str) -> io::Result<()> {
    if !extension.is_empty()
        && extension.chars().all(|ch| ch.is_ascii_alphanumeric())
    {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("extension must be ASCII alphanumeric, got {extension:?}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_extension_accepts_a_bare_alphanumeric_suffix() {
        for extension in ["txt", "md", "rs", "html", "mp3"] {
            assert!(check_extension(extension).is_ok(), "{extension}");
        }
    }

    #[test]
    fn check_extension_rejects_a_path_escaping_the_temp_dir() {
        for extension in ["../../etc/passwd", "txt/../evil", "a/b", "..", "a.b"]
        {
            let error = check_extension(extension)
                .expect_err("a path fragment must not pass");
            assert_eq!(
                error.kind(),
                io::ErrorKind::InvalidInput,
                "{extension}"
            );
        }
    }

    #[test]
    fn check_extension_rejects_an_empty_extension() {
        let error = check_extension("").expect_err("empty must not pass");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
