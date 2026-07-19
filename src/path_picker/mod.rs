//! A filesystem path picker modal.
//!
//! Browses directories: `Up`/`Down` move (cyclic), `PageUp`/`PageDown` move by
//! a page and `Home`/`End` jump to the first/last entry (both clamped), `Right`
//! descends into a folder, `Left`/`Backspace` (empty filter) ascends, typing
//! filters the entries, `Ctrl+H` toggles hidden (dot-prefixed) entries (hidden
//! by default), `Enter` selects the highlighted entry (a folder, or a file when
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

use ratatui::Frame;

use super::{
    chrome, fuzzy,
    input::InputField,
    layout::centered_fraction,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup_with_paste},
    terminal::Tui,
};
use crate::theme::Skin;

mod fs;
mod interaction;
mod render;

use fs::{confine, first_existing, read_entries, within_root};
use interaction::handle_key;
use render::render_body;

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
    viewport: Cell<usize>,
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
            viewport: Cell::new(1),
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
    popup_with_paste(
        tui,
        &mut state,
        |area, _| centered_fraction(area, 2, 3, 36, 8),
        |frame, _| render_bg(frame),
        |frame, rect, state: &State| {
            let inner = overlay::framed(frame, rect, skin, title);
            render_body(frame, inner, skin, state);
            let badge =
                chrome::position_badge(state.cursor, state.visible.len());
            chrome::render_badge(frame, rect, skin, &badge);
        },
        |state, key| handle_key(state, key, allow_files),
        |state, text| {
            state.filter.paste(&text);
            state.refilter();
            PopupFlow::Continue
        },
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use crossterm::event::{KeyCode, KeyEvent};

    use crossterm::event::KeyModifiers;

    use super::{fs::is_hidden, *};

    /// A picker over a fresh temp dir holding one visible and one hidden entry.
    /// The directory name carries the pid, so parallel test binaries cannot
    /// collide.
    fn picker_over_a_temp_dir() -> (State, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ratada-path-picker-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("visible")).expect("temp dir is writable");
        fs::create_dir_all(dir.join(".hidden")).expect("temp dir is writable");
        (State::new(&dir, false, None), dir)
    }

    /// A confined picker whose root holds one real folder plus a symlink
    /// pointing at a directory *outside* the root. Returns the state and the
    /// symlink's own path inside the root.
    #[cfg(unix)]
    fn picker_with_an_escaping_symlink() -> State {
        let base = std::env::temp_dir().join(format!(
            "ratada-path-picker-escape-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("root");
        let outside = base.join("outside");
        fs::create_dir_all(root.join("inside")).expect("temp dir is writable");
        fs::create_dir_all(&outside).expect("temp dir is writable");
        std::os::unix::fs::symlink(&outside, root.join("escape"))
            .expect("symlink is allowed");
        let canonical_root = root.canonicalize().expect("the root resolves");
        State::new(&root, false, Some(&canonical_root))
    }

    /// Moves the cursor onto the visible entry named `name`. Matching by name
    /// rather than by path: the picker lists the *canonicalized* directory, so
    /// the entry paths do not equal the ones the test just built.
    #[cfg(unix)]
    fn select_by_name(state: &mut State, name: &str) {
        state.cursor = state
            .visible
            .iter()
            .position(|&index| state.entries[index].name == name)
            .unwrap_or_else(|| panic!("{name} is listed"));
    }

    /// `descend` refuses to follow a symlink out of the root, but `Enter` is
    /// the only path that hands a value back to the caller - so it must check
    /// the confinement too, or the guarantee in the module doc is void.
    #[cfg(unix)]
    #[test]
    fn enter_does_not_return_a_symlink_leading_out_of_the_root() {
        let mut state = picker_with_an_escaping_symlink();
        select_by_name(&mut state, "escape");

        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert!(
            matches!(handle_key(&mut state, enter, false), PopupFlow::Continue),
            "Enter handed an out-of-root path to the caller"
        );
    }

    /// The counterpart: a folder genuinely inside the root is still selectable,
    /// so the guard above is not simply refusing everything.
    #[cfg(unix)]
    #[test]
    fn enter_still_returns_a_folder_inside_the_root() {
        let mut state = picker_with_an_escaping_symlink();
        select_by_name(&mut state, "inside");

        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let PopupFlow::Done(path) = handle_key(&mut state, enter, false) else {
            panic!("a folder inside the root must be selectable");
        };
        assert_eq!(path.file_name(), Some("inside".as_ref()));
    }

    /// `Ctrl+H` toggles hidden entries, but the picker also has a filter field
    /// right there - so `AltGr+H` (Control+Alt) must type into the filter
    /// instead of toggling. That distinction is the whole reason this arm went
    /// through `is_command` rather than a bare CONTROL check.
    #[test]
    fn altgr_h_types_into_the_filter_instead_of_toggling_hidden() {
        let (mut state, dir) = picker_over_a_temp_dir();
        assert!(!state.show_hidden);

        let altgr_h = KeyEvent::new(
            KeyCode::Char('h'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );
        handle_key(&mut state, altgr_h, false);
        assert!(!state.show_hidden, "AltGr+H toggled the hidden entries");
        assert_eq!(state.filter.value(), "h", "AltGr+H did not type");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ctrl_h_toggles_the_hidden_entries() {
        let (mut state, dir) = picker_over_a_temp_dir();
        let ctrl_h = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL);

        handle_key(&mut state, ctrl_h, false);
        assert!(state.show_hidden);
        assert!(
            state.entries.iter().any(|entry| entry.name == ".hidden"),
            "the hidden entry should be listed now"
        );

        handle_key(&mut state, ctrl_h, false);
        assert!(!state.show_hidden);

        let _ = fs::remove_dir_all(dir);
    }

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
