//! A filesystem path picker modal.
//!
//! Browses directories: `Up`/`Down` move (cyclic), `Right` descends into a
//! folder, `Left`/`Backspace` (empty filter) ascends, typing filters the
//! entries, `Ctrl+H` toggles hidden (dot-prefixed) entries (hidden by default),
//! `Enter` selects the highlighted entry (a folder, or a file when
//! `allow_files`), `Esc` cancels.
//!
//! An optional confinement root ([`PathPickerConfig::root`]) bounds navigation:
//! the picker never ascends above it and never follows a symlinked folder out
//! of it (both are checked with `canonicalize` + `starts_with`).

use std::{
    cell::Cell,
    io,
    path::{Path, PathBuf},
};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    fuzzy,
    input::InputField,
    layout::centered_fraction,
    list,
    modal::ModalSignal,
    nav,
    overlay::{self, PopupFlow, popup},
    shortcut_hints, style,
    terminal::Tui,
    text::truncate,
};
use crate::theme::Skin;

struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

struct State {
    dir: PathBuf,
    /// The confinement root (canonicalized), above which navigation is barred.
    root: Option<PathBuf>,
    allow_files: bool,
    show_hidden: bool,
    entries: Vec<Entry>,
    visible: Vec<usize>,
    filter: InputField,
    cursor: usize,
    offset: Cell<usize>,
}

impl State {
    fn new(start: &Path, allow_files: bool, root: Option<&Path>) -> Self {
        let root = root.map(|root| match root.canonicalize() {
            Ok(canonical) => canonical,
            Err(error) => {
                log::warn!(
                    "could not canonicalize confinement root {}: {error}; \
                     using the path as given",
                    root.display()
                );
                root.to_path_buf()
            }
        });
        let dir = confine(first_existing(start), root.as_deref());
        let mut state = Self {
            dir,
            root,
            allow_files,
            show_hidden: false,
            entries: Vec::new(),
            visible: Vec::new(),
            filter: InputField::default(),
            cursor: 0,
            offset: Cell::new(0),
        };
        state.reload();
        state
    }

    fn reload(&mut self) {
        self.entries =
            read_entries(&self.dir, self.allow_files, self.show_hidden);
        self.filter = InputField::default();
        self.refilter();
    }

    /// Toggles hidden (dot-prefixed) entries, keeping the current directory and
    /// filter, then re-reads the directory.
    fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.entries =
            read_entries(&self.dir, self.allow_files, self.show_hidden);
        self.refilter();
    }

    fn refilter(&mut self) {
        let query = self.filter.value();
        self.visible = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                query.trim().is_empty()
                    || fuzzy::score(&entry.name, query).is_some()
            })
            .map(|(index, _)| index)
            .collect();
        self.cursor = 0;
    }

    fn selected(&self) -> Option<&Entry> {
        self.visible
            .get(self.cursor)
            .map(|&index| &self.entries[index])
    }

    fn descend(&mut self) {
        if let Some(entry) = self.selected()
            && entry.is_dir
        {
            // Canonicalize so a symlinked folder cannot escape the root.
            let target = match entry.path.canonicalize() {
                Ok(canonical) => canonical,
                Err(error) => {
                    log::warn!(
                        "could not canonicalize {}: {error}; \
                         checking the path as given",
                        entry.path.display()
                    );
                    entry.path.clone()
                }
            };
            if !within_root(&target, self.root.as_deref()) {
                return;
            }
            self.dir = target;
            self.reload();
        }
    }

    fn ascend(&mut self) {
        let Some(parent) = self.dir.parent() else {
            return;
        };
        let parent = parent.to_path_buf();
        if !within_root(&parent, self.root.as_deref()) {
            return;
        }
        self.dir = parent;
        self.reload();
    }
}

/// Whether `path` is allowed given an optional confinement `root`: always so
/// without a root, otherwise only when `path` lies at or below it.
fn within_root(path: &Path, root: Option<&Path>) -> bool {
    root.is_none_or(|root| path.starts_with(root))
}

/// Clamps `dir` into `root` (canonicalizing it first): returns the canonical
/// `dir` when it lies within `root`, otherwise `root` itself. Without a `root`,
/// returns `dir` unchanged.
fn confine(dir: PathBuf, root: Option<&Path>) -> PathBuf {
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

/// How to open the [`path_picker`]: the modal `title`, the `start` directory,
/// whether files (not just folders) are selectable, and an optional confinement
/// `root` that navigation may not leave.
#[derive(Debug, Clone, Copy)]
pub struct PathPickerConfig<'a> {
    /// The modal title shown in the top border.
    pub title: &'a str,
    /// The directory to open at (clamped into `root` when set).
    pub start: &'a Path,
    /// Whether files, not only folders, may be selected.
    pub allow_files: bool,
    /// An optional root the picker may not ascend above or symlink out of.
    pub root: Option<&'a Path>,
}

/// Opens the picker per `config`. Returns `Value(path)` on selection,
/// `Cancelled` on `Esc`, `Quit` on the global quit chord.
pub fn path_picker(
    tui: &mut Tui,
    skin: &Skin,
    config: PathPickerConfig<'_>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<PathBuf>> {
    let PathPickerConfig {
        title,
        start,
        allow_files,
        root,
    } = config;
    let mut state = State::new(start, allow_files, root);
    popup(
        tui,
        &mut state,
        |area, _| centered_fraction(area, 2, 3, 36, 8),
        |frame, _| render_bg(frame),
        |frame, rect, state: &State| {
            let inner = overlay::framed(frame, rect, skin, title);
            render_body(frame, inner, skin, state);
        },
        |state, key| match key.code {
            KeyCode::Esc => PopupFlow::Cancelled,
            KeyCode::Up => {
                state.cursor =
                    nav::cycle(state.cursor, state.visible.len(), -1);
                PopupFlow::Continue
            }
            KeyCode::Down => {
                state.cursor = nav::cycle(state.cursor, state.visible.len(), 1);
                PopupFlow::Continue
            }
            KeyCode::Right => {
                state.descend();
                PopupFlow::Continue
            }
            KeyCode::Left => {
                state.ascend();
                PopupFlow::Continue
            }
            KeyCode::Backspace if state.filter.value().is_empty() => {
                state.ascend();
                PopupFlow::Continue
            }
            KeyCode::Char('h')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                state.toggle_hidden();
                PopupFlow::Continue
            }
            KeyCode::Enter => match state.selected() {
                Some(entry) if entry.is_dir || allow_files => {
                    PopupFlow::Done(entry.path.clone())
                }
                _ => PopupFlow::Continue,
            },
            _ => {
                if state.filter.handle_key(key) {
                    state.refilter();
                }
                PopupFlow::Continue
            }
        },
    )
}

fn render_body(frame: &mut Frame, inner: Rect, skin: &Skin, state: &State) {
    let palette = &skin.palette;
    let inner_width = inner.width as usize;

    // Header (current dir), filter line, the scrollable entry list, footer.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate(&state.dir.display().to_string(), inner_width),
            style::secondary(palette),
        ))),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("filter ", style::secondary(palette)),
            Span::raw(state.filter.value().to_string()),
        ])),
        rows[1],
    );

    // The list widget owns the cursor highlight, scroll-to-cursor and the
    // scrollbar on overflow; directories keep their accent color when not
    // under the cursor.
    let entries: Vec<Line<'static>> = state
        .visible
        .iter()
        .map(|&index| {
            let entry = &state.entries[index];
            let marker = if entry.is_dir { "/" } else { " " };
            let line = Line::from(truncate(
                &format!("{marker} {}", entry.name),
                inner_width,
            ));
            if entry.is_dir {
                line.style(style::fg(palette.accent))
            } else {
                line
            }
        })
        .collect();
    list::render(
        frame,
        rows[2],
        skin,
        list::ListView {
            rows: entries,
            selected: state.cursor,
            offset: &state.offset,
        },
    );

    frame.render_widget(
        Paragraph::new(
            shortcut_hints::lines(
                &[
                    ("\u{2190}\u{2192}", "browse"),
                    ("enter", "pick"),
                    ("ctrl+h", "hidden"),
                ],
                palette.accent_dim,
                inner_width,
            )
            .into_iter()
            .next()
            .unwrap_or_default(),
        ),
        rows[3],
    );
}

/// Returns `start` if it exists, else its nearest existing ancestor, else the
/// current directory.
fn first_existing(start: &Path) -> PathBuf {
    let mut candidate = Some(start);
    while let Some(path) = candidate {
        if path.is_dir() {
            return path.to_path_buf();
        }
        candidate = path.parent();
    }
    PathBuf::from(".")
}

fn read_entries(
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
fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{is_hidden, within_root};

    #[test]
    fn dot_prefixed_names_are_hidden() {
        assert!(is_hidden(".git"));
        assert!(is_hidden(".config"));
        assert!(!is_hidden("src"));
        assert!(!is_hidden("Cargo.toml"));
    }

    #[test]
    fn within_root_confines_to_the_root_subtree() {
        let root = Some(Path::new("/a/b"));
        assert!(within_root(Path::new("/a/b"), root));
        assert!(within_root(Path::new("/a/b/c/d"), root));
        assert!(!within_root(Path::new("/a"), root));
        assert!(!within_root(Path::new("/a/x"), root));
    }

    #[test]
    fn no_root_allows_any_path() {
        assert!(within_root(Path::new("/anywhere/at/all"), None));
    }
}
