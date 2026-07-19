//! Filesystem access for the picker: reading a directory and the confinement
//! checks that keep a selection inside its root.
//!
//! Separated from the widget because this is the part with the security
//! obligation (§2.4.9): every path that crosses the widget boundary is
//! canonicalized and checked here, and nowhere else.

use std::path::{Path, PathBuf};

use super::Entry;

/// Whether `path` is allowed given an optional confinement `root`: always so
/// without a root, otherwise only when `path` lies at or below it.
pub(super) fn within_root(path: &Path, root: Option<&Path>) -> bool {
    root.is_none_or(|root| path.starts_with(root))
}

/// The path `Enter` may hand back for the selected `path`, or `None` when it
/// would leave `root`.
///
/// `Enter` is the only way a value leaves this widget, so the confinement has
/// to be checked here and not only while navigating: [`State::descend`] stops
/// the *cursor* from walking out through a symlink, but a symlinked folder is
/// still listed and could simply be selected.
///
/// Unlike [`confine`], a failing `canonicalize` rejects rather than falling
/// back to the path as given. There the fallback only affects which directory
/// is displayed; here an unresolvable path is exactly the case the check
/// exists for, and handing it out unverified would defeat the guarantee.
pub(super) fn confined_selection(
    path: &Path,
    root: Option<&Path>,
) -> Option<PathBuf> {
    let Some(root) = root else {
        return Some(path.to_path_buf());
    };
    let canonical = path
        .canonicalize()
        .inspect_err(|error| {
            log::warn!(
                "could not canonicalize {}: {error}; refusing the selection",
                path.display()
            );
        })
        .ok()?;
    within_root(&canonical, Some(root)).then_some(canonical)
}

/// Clamps `dir` into `root` (canonicalizing it first): returns the canonical
/// `dir` when it lies within `root`, otherwise `root` itself. Without a `root`,
/// returns `dir` unchanged.
pub(super) fn confine(dir: PathBuf, root: Option<&Path>) -> PathBuf {
    let Some(root) = root else {
        return dir;
    };
    let canonical = match dir.canonicalize() {
        Ok(canonical) => canonical,
        Err(error) => {
            log::warn!(
                "could not canonicalize {}: {error}; checking the path as given",
                dir.display()
            );
            dir
        }
    };
    if within_root(&canonical, Some(root)) {
        canonical
    } else {
        root.to_path_buf()
    }
}

/// Returns `start` if it exists, else its nearest existing ancestor, else the
/// current directory.
pub(super) fn first_existing(start: &Path) -> PathBuf {
    let mut candidate = Some(start);
    while let Some(path) = candidate {
        if path.is_dir() {
            return path.to_path_buf();
        }
        candidate = path.parent();
    }
    PathBuf::from(".")
}

pub(super) fn read_entries(
    dir: &Path,
    allow_files: bool,
    show_hidden: bool,
) -> Vec<Entry> {
    let read = match std::fs::read_dir(dir) {
        Ok(read) => read,
        Err(error) => {
            log::warn!("could not read directory {}: {error}", dir.display());
            return Vec::new();
        }
    };
    let mut entries: Vec<Entry> = read
        .flatten()
        .filter_map(|item| {
            let path = item.path();
            let is_dir = path.is_dir();
            if !is_dir && !allow_files {
                return None;
            }
            let name = item.file_name().to_string_lossy().into_owned();
            if !show_hidden && is_hidden(&name) {
                return None;
            }
            Some(Entry { name, path, is_dir })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

/// Whether an entry name is hidden (dot-prefixed, the Unix convention).
pub(super) fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}
