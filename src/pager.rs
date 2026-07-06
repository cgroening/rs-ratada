//! A read-only text pager modal: scroll long text with an incremental search.

use std::{cell::Cell, io};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    layout::centered_rect,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup},
    scroll, style,
    terminal::Tui,
};
use crate::theme::Skin;

/// The scroll and search state of the pager.
struct Pager {
    lines: Vec<String>,
    offset: usize,
    query: String,
    searching: bool,
    matches: Vec<usize>,
    match_pos: usize,
    /// The visible line count, set during rendering and read by navigation.
    viewport: Cell<usize>,
}

impl Pager {
    /// The largest first-line offset that still fills the viewport.
    fn max_offset(&self) -> usize {
        self.lines.len().saturating_sub(self.viewport.get().max(1))
    }

    fn handle_key(&mut self, key: KeyEvent) -> PopupFlow<()> {
        let view = self.viewport.get().max(1);
        let max_offset = self.max_offset();
        if self.searching {
            self.handle_search_key(key, max_offset);
            return PopupFlow::Continue;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return PopupFlow::Done(()),
            KeyCode::Down | KeyCode::Char('j') => {
                self.offset = (self.offset + 1).min(max_offset);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.offset = self.offset.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.offset = (self.offset + view).min(max_offset);
            }
            KeyCode::PageUp => self.offset = self.offset.saturating_sub(view),
            KeyCode::Home | KeyCode::Char('g') => self.offset = 0,
            KeyCode::End | KeyCode::Char('G') => self.offset = max_offset,
            KeyCode::Char('/') => {
                self.searching = true;
                self.query.clear();
                self.matches.clear();
            }
            KeyCode::Char('n') if !self.matches.is_empty() => {
                self.match_pos = (self.match_pos + 1) % self.matches.len();
                self.jump_to_match(max_offset);
            }
            KeyCode::Char('N') if !self.matches.is_empty() => {
                let len = self.matches.len();
                self.match_pos = (self.match_pos + len - 1) % len;
                self.jump_to_match(max_offset);
            }
            _ => {}
        }
        PopupFlow::Continue
    }

    fn handle_search_key(&mut self, key: KeyEvent, max_offset: usize) {
        match key.code {
            KeyCode::Enter => self.searching = false,
            KeyCode::Esc => {
                self.searching = false;
                self.query.clear();
                self.matches.clear();
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.recompute();
                self.jump_to_match(max_offset);
            }
            KeyCode::Char(ch) => {
                self.query.push(ch);
                self.recompute();
                self.jump_to_match(max_offset);
            }
            _ => {}
        }
    }

    fn recompute(&mut self) {
        self.matches = search_matches(&self.lines, &self.query);
        self.match_pos = 0;
    }

    /// Scrolls so the current match line is in view (placed at the top).
    fn jump_to_match(&mut self, max_offset: usize) {
        if let Some(&line) = self.matches.get(self.match_pos) {
            self.offset = line.min(max_offset);
        }
    }
}

/// Shows `text` in a scrollable, searchable viewer until the user closes it.
///
/// Scroll with `j`/`k`/arrows, `PageUp`/`PageDown`, `g`/`G` (top/bottom). `/`
/// starts a case-insensitive search; `n`/`N` jump between matches. `Esc` leaves
/// the search, then the viewer.
pub fn pager(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    text: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<()>> {
    let mut state = Pager {
        lines: text.lines().map(str::to_string).collect(),
        offset: 0,
        query: String::new(),
        searching: false,
        matches: Vec::new(),
        match_pos: 0,
        viewport: Cell::new(1),
    };
    popup(
        tui,
        &mut state,
        |area, _| {
            centered_rect(
                (area.width * 3 / 4).clamp(40, area.width),
                (area.height * 3 / 4).clamp(8, area.height),
                area,
            )
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &Pager| {
            let inner = overlay::framed(frame, rect, skin, title);
            render_body(frame, inner, skin, state);
        },
        Pager::handle_key,
    )
}

/// The line indices that contain `query`, case-insensitively. An empty query
/// matches nothing.
pub(crate) fn search_matches(lines: &[String], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let needle = query.to_lowercase();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.to_lowercase().contains(&needle))
        .map(|(index, _)| index)
        .collect()
}

fn render_body(frame: &mut Frame, inner: Rect, skin: &Skin, state: &Pager) {
    let palette = &skin.palette;
    let lines = &state.lines;
    let offset = state.offset;
    let query = &state.query;
    let searching = state.searching;
    let viewport = &state.viewport;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let text_area = rows[0];
    let view = text_area.height as usize;
    viewport.set(view);

    let visible: Vec<Line> = lines
        .iter()
        .skip(offset)
        .take(view)
        .map(|line| highlight_line(line, query, skin))
        .collect();
    frame.render_widget(Paragraph::new(visible), text_area);
    scroll::render_scrollbar(frame, text_area, skin, lines.len(), offset, view);

    let footer = if searching {
        Line::from(vec![
            Span::styled("/", style::fg(palette.accent)),
            Span::raw(state.query.clone()),
            Span::styled(" ", style::bg(palette.cursor)),
        ])
    } else {
        let percent = scroll_percent(offset, lines.len(), view);
        Line::from(Span::styled(
            format!(
                " {percent}%  \u{b7}  j/k scroll \u{b7} / search \u{b7} n/N next \u{b7} q close"
            ),
            style::secondary(palette),
        ))
    };
    frame.render_widget(Paragraph::new(footer), rows[1]);
}

/// Highlights case-insensitive occurrences of `query` within a single line.
fn highlight_line(line: &str, query: &str, skin: &Skin) -> Line<'static> {
    if query.is_empty() {
        return Line::from(line.to_string());
    }
    let needle = query.to_lowercase();
    let haystack = line.to_lowercase();
    let accent = style::fg(skin.palette.accent).add_modifier(Modifier::BOLD);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut start = 0usize;
    while let Some(found) = haystack[start..].find(&needle) {
        let at = start + found;
        if at > start {
            spans.push(Span::raw(line[start..at].to_string()));
        }
        let end = at + needle.len();
        spans.push(Span::styled(line[at..end].to_string(), accent));
        start = end;
    }
    if start < line.len() {
        spans.push(Span::raw(line[start..].to_string()));
    }
    Line::from(spans)
}

/// The scroll position as a percentage (0 when everything fits).
fn scroll_percent(offset: usize, total: usize, viewport: usize) -> usize {
    let max_offset = total.saturating_sub(viewport);
    if max_offset == 0 {
        return 100;
    }
    offset * 100 / max_offset
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines() -> Vec<String> {
        ["alpha", "Beta line", "gamma", "beta again"]
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    #[test]
    fn search_is_case_insensitive() {
        assert_eq!(search_matches(&lines(), "beta"), vec![1, 3]);
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(search_matches(&lines(), "").is_empty());
    }

    #[test]
    fn scroll_percent_is_zero_at_top_and_full_when_fitting() {
        assert_eq!(scroll_percent(0, 100, 10), 0);
        assert_eq!(scroll_percent(90, 100, 10), 100);
        assert_eq!(scroll_percent(5, 8, 20), 100);
    }
}
