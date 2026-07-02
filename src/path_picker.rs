//! A filesystem path picker modal.
//!
//! Browses directories: `Up`/`Down` move (cyclic), `Right` descends into a
//! folder, `Left`/`Backspace` (empty filter) ascends, typing filters the
//! entries, `Ctrl+H` toggles hidden (dot-prefixed) entries (hidden by default),
//! `Enter` selects the highlighted entry (a folder, or a file when
//! `allow_files`), `Esc` cancels.

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
    footer, fuzzy,
    input::InputField,
    layout::centered_rect,
    list,
    modal::ModalSignal,
    nav,
    overlay::{self, PopupFlow, popup},
    style,
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
    allow_files: bool,
    show_hidden: bool,
    entries: Vec<Entry>,
    visible: Vec<usize>,
    filter: InputField,
    cursor: usize,
    offset: Cell<usize>,
}

impl State {
    fn new(start: &Path, allow_files: bool) -> Self {
        let dir = first_existing(start);
        let mut state = Self {
            dir,
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
            self.dir = entry.path.clone();
            self.reload();
        }
    }

    fn ascend(&mut self) {
        if let Some(parent) = self.dir.parent() {
            self.dir = parent.to_path_buf();
            self.reload();
        }
    }
}

/// Opens the picker at `start`. Returns `Value(path)` on selection, `Cancelled`
/// on `Esc`, `Quit` on the global quit chord.
pub fn path_picker(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    start: &Path,
    allow_files: bool,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<PathBuf>> {
    let mut state = State::new(start, allow_files);
    popup(
        tui,
        &mut state,
        |area, _| {
            let width = (area.width * 2 / 3).clamp(36, area.width);
            let height = (area.height * 2 / 3).clamp(8, area.height);
            centered_rect(width, height, area)
        },
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
            style::dim(),
        ))),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("filter ", style::dim()),
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
    list::render(frame, rows[2], skin, entries, state.cursor, &state.offset);

    frame.render_widget(
        Paragraph::new(
            footer::lines(
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
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
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
    use super::is_hidden;

    #[test]
    fn dot_prefixed_names_are_hidden() {
        assert!(is_hidden(".git"));
        assert!(is_hidden(".config"));
        assert!(!is_hidden("src"));
        assert!(!is_hidden("Cargo.toml"));
    }
}
