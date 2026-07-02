//! A filesystem path picker modal.
//!
//! Browses directories: `Up`/`Down` move (cyclic), `Right` descends into a
//! folder, `Left`/`Backspace` (empty filter) ascends, typing filters the
//! entries, `Enter` selects the highlighted entry (a folder, or a file when
//! `allow_files`), `Esc` cancels.

use std::{
    io,
    path::{Path, PathBuf},
};

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    footer,
    input::InputField,
    layout::centered_rect,
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
    entries: Vec<Entry>,
    visible: Vec<usize>,
    filter: InputField,
    cursor: usize,
}

impl State {
    fn new(start: &Path, allow_files: bool) -> Self {
        let dir = first_existing(start);
        let mut state = Self {
            dir,
            allow_files,
            entries: Vec::new(),
            visible: Vec::new(),
            filter: InputField::default(),
            cursor: 0,
        };
        state.reload();
        state
    }

    fn reload(&mut self) {
        self.entries = read_entries(&self.dir, self.allow_files);
        self.filter = InputField::default();
        self.refilter();
    }

    fn refilter(&mut self) {
        let needle = self.filter.value().to_lowercase();
        self.visible = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.name.to_lowercase().contains(&needle))
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
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        truncate(&state.dir.display().to_string(), inner_width),
        style::dim(),
    )));
    lines.push(Line::from(vec![
        Span::styled("filter ", style::dim()),
        Span::raw(state.filter.value().to_string()),
    ]));

    let rows = inner.height.saturating_sub(3) as usize;
    for (row, &index) in state.visible.iter().enumerate().take(rows) {
        let entry = &state.entries[index];
        let marker = if entry.is_dir { "/" } else { " " };
        let text = format!("{marker} {}", entry.name);
        let line = Line::from(truncate(&text, inner_width));
        lines.push(if row == state.cursor {
            line.style(style::bg(palette.selection_bg))
        } else if entry.is_dir {
            line.style(style::fg(palette.accent))
        } else {
            line
        });
    }

    lines.push(
        footer::lines(
            &[("\u{2190}\u{2192}", "browse"), ("enter", "pick")],
            palette.accent_dim,
            inner_width,
        )
        .into_iter()
        .next()
        .unwrap_or_default(),
    );
    frame.render_widget(Paragraph::new(lines), inner);
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

fn read_entries(dir: &Path, allow_files: bool) -> Vec<Entry> {
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
