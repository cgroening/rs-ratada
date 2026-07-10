//! Opening a file in the operating system's default application.
//!
//! The platform opener is invoked with an argument list via
//! [`std::process::Command`], never a shell, so a path can never be
//! interpreted as a command (no injection).

use std::{io, path::Path, process::Command};

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
    let status = opener_command(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("opener exited with {status}")))
    }
}

/// Builds the platform-specific opener command for `path`.
#[cfg(target_os = "macos")]
fn opener_command(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg(path);
    command
}

#[cfg(target_os = "windows")]
fn opener_command(path: &Path) -> Command {
    // `start` is a cmd builtin; the empty first argument is the window title.
    let mut command = Command::new("cmd");
    command.args(["/C", "start", ""]).arg(path);
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
}
