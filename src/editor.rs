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
use super::terminal::normalize_newlines;

/// Resolves the editor command: `$VISUAL`, then `$EDITOR`, then `vi`.
pub fn resolve_editor() -> String {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string())
}

/// Edits `initial` in `editor` via a plain-text temp file, returning the text
/// (line endings normalised to LF, trailing newlines trimmed), or `None` when
/// the editor could not be run.
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

    let (program, args) = split_editor(editor);
    let status =
        tui.suspend(|| Command::new(program).args(args).arg(&path).status())?;

    let result = match status {
        Ok(code) if code.success() => {
            Some(edited_text(&std::fs::read_to_string(&path)?))
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

/// The text an editor session produced: line endings normalised to LF and
/// trailing blank lines dropped.
///
/// Normalising *before* trimming is what makes an editor configured for CRLF
/// (Notepad, `files.eol=\r\n`, `fileformat=dos`) behave: trimming only `\n`
/// would leave a lone `\r` behind on `"text\r\n"`, and every interior `\r\n`
/// would ride out into the caller's buffer.
fn edited_text(raw: &str) -> String {
    normalize_newlines(raw).trim_end_matches('\n').to_string()
}

/// Splits an `$EDITOR` value into the program and its leading arguments.
///
/// `$EDITOR` is conventionally a command line, not a bare program name -
/// `code --wait`, `subl -w`, `emacs -nw` are all common. Splitting on
/// whitespace here rather than handing the string to a shell keeps the file
/// name out of any shell's reach, so no injection is possible (§2.4.9). A
/// program path containing a space is not supported; that is the accepted
/// trade for not spawning `sh -c`.
fn split_editor(editor: &str) -> (&str, Vec<&str>) {
    let mut words = editor.split_whitespace();
    let program = words.next().unwrap_or("vi");
    (program, words.collect())
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

    /// `$EDITOR` commonly carries arguments (`code --wait`, `subl -w`). Passing
    /// the whole string to `Command::new` looks for a program whose file name
    /// contains a space, fails with `ENOENT`, and silently discards the edit.
    #[test]
    fn a_multi_word_editor_splits_into_program_and_arguments() {
        let (program, args) = split_editor("code --wait");
        assert_eq!(program, "code");
        assert_eq!(args, vec!["--wait"]);

        let (program, args) = split_editor("emacs -nw -Q");
        assert_eq!(program, "emacs");
        assert_eq!(args, vec!["-nw", "-Q"]);
    }

    /// An editor that writes CRLF must not leak either an interior `\r\n` or
    /// a trailing lone `\r` into the returned text.
    #[test]
    fn edited_text_normalises_crlf_and_trims_the_trailing_break() {
        assert_eq!(edited_text("text\r\n"), "text");
        assert_eq!(edited_text("a\r\nb\r\n"), "a\nb");
        assert!(!edited_text("a\r\nb\r\n").contains('\r'));
    }

    /// LF input is already right, and a body's interior blank lines are
    /// content - only the trailing ones go.
    #[test]
    fn edited_text_keeps_lf_input_and_interior_blank_lines() {
        assert_eq!(edited_text("a\n\nb\n\n\n"), "a\n\nb");
        assert_eq!(edited_text("plain"), "plain");
        assert_eq!(edited_text(""), "");
    }

    #[test]
    fn a_single_word_editor_has_no_arguments() {
        let (program, args) = split_editor("vim");
        assert_eq!(program, "vim");
        assert!(args.is_empty());
    }

    /// Surrounding and repeated whitespace must not produce an empty program
    /// name or empty argument entries.
    #[test]
    fn surrounding_whitespace_is_ignored() {
        let (program, args) = split_editor("  code   --wait  ");
        assert_eq!(program, "code");
        assert_eq!(args, vec!["--wait"]);
    }

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
