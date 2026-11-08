//! Opening a file in the operating system's default application, and a web
//! link in the default browser.
//!
//! The platform opener is invoked with an argument list via
//! [`std::process::Command`], never a shell, so a path can never be
//! interpreted as a command (no injection). The path is made absolute first,
//! so it can never be mistaken for an option either. A link must be an
//! `http(s)` URL without blanks or control characters, so it can neither be
//! read as an option nor open a local file or another scheme.

use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
};

/// Opens `path` in the system's default application.
///
/// # Errors
///
/// Returns [`io::ErrorKind::NotFound`] if the file does not exist, or any I/O
/// error from launching the opener.
pub fn open(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("file not found: {}", path.display()),
        ));
    }
    let status = opener_command(&as_argument(path)).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("opener exited with {status}")))
    }
}

/// Opens the web link `url` in the default browser.
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidInput`] if `url` is not an `http://` or
/// `https://` link (or carries blanks or control characters), or any I/O
/// error from launching the opener.
pub fn open_url(url: &str) -> io::Result<()> {
    let url = checked_url(url)?;
    let status = url_command(url).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("opener exited with {status}")))
    }
}

/// `url` trimmed, if it is a plain `http(s)` link.
fn checked_url(url: &str) -> io::Result<&str> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    let is_web = lower.starts_with("https://") || lower.starts_with("http://");
    let is_plain = !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if is_web && is_plain {
        Ok(url)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not a web link: {url}"),
        ))
    }
}

/// Builds the platform-specific command that opens the web link `url`.
#[cfg(target_os = "macos")]
fn url_command(url: &str) -> Command {
    let mut command = Command::new("open");
    command.arg(url);
    command
}

#[cfg(target_os = "windows")]
fn url_command(url: &str) -> Command {
    // As for files: no shell, the link is one argument.
    let mut command = Command::new("rundll32.exe");
    command.arg("url.dll,FileProtocolHandler").arg(url);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn url_command(url: &str) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(url);
    command
}

/// Returns `path` in the form handed to the opener: absolute where possible.
///
/// A *relative* path is what makes this necessary. `open`/`xdg-open` parse
/// their arguments, so a file literally named `-a` in the working directory
/// would arrive as an option rather than as the file to open - `open -a` even
/// consumes the next token as an application name. An absolute path cannot
/// begin with `-`, which closes that off on every platform without relying on
/// a `--` separator (`xdg-open` implementations do not all honour one).
///
/// [`std::path::absolute`] is deliberate: unlike `canonicalize` it neither
/// resolves symlinks nor produces a `\\?\` verbatim path on Windows, both of
/// which some openers choke on. If it fails, the original path is used - the
/// opener reporting a bad path beats refusing to open a valid one.
fn as_argument(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|error| {
        log::debug!(
            "could not absolutize {}: {error}; passing it as given",
            path.display()
        );
        path.to_path_buf()
    })
}

/// Builds the platform-specific opener command for `path`.
#[cfg(target_os = "macos")]
fn opener_command(path: &Path) -> Command {
    let mut command = Command::new("open");
    // `--` so a path is never read as an option, belt and braces next to the
    // absolute path from `as_argument`.
    command.arg("--").arg(path);
    command
}

#[cfg(target_os = "windows")]
fn opener_command(path: &Path) -> Command {
    // Deliberately not `cmd /C start`: `cmd.exe` is a shell, and Rust's
    // argument escaping follows the CRT `argv` convention, not `cmd`'s own
    // parsing of `&`, `|`, `^` and `%VAR%`. A file name carrying one of those
    // could therefore break out (the class of CVE-2024-24576). `rundll32`
    // takes the path as a single argument and involves no shell.
    let mut command = Command::new("rundll32.exe");
    command.arg("url.dll,FileProtocolHandler").arg(path);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn opener_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_reported_as_not_found() {
        let error = open(Path::new("/no/such/file/at/all")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    /// A relative path whose name starts with `-` would otherwise reach
    /// `open`/`xdg-open` as an option instead of as the file to open.
    #[test]
    fn a_dash_leading_relative_path_becomes_absolute() {
        let argument = as_argument(Path::new("-a"));
        assert!(argument.is_absolute(), "{}", argument.display());
        assert_eq!(argument.file_name(), Some("-a".as_ref()));
    }

    #[test]
    fn only_plain_web_links_pass() {
        assert_eq!(
            checked_url(" https://example.org/x?y=1 ").unwrap(),
            "https://example.org/x?y=1"
        );
        assert!(checked_url("HTTP://example.org").is_ok());
        for bad in [
            "file:///etc/passwd",
            "-a",
            "https://a b",
            "javascript:x",
            "",
        ] {
            let error = checked_url(bad).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{bad}");
        }
    }

    #[test]
    fn an_absolute_path_stays_as_it_is() {
        let path = Path::new("/tmp/plain.txt");
        assert_eq!(as_argument(path), path);
    }
}
