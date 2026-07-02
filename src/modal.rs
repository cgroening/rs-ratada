//! Reusable modal widgets. Each is a thin wrapper over [`overlay::popup`]: it
//! sets up its state and closures and returns a [`ModalSignal`]. The dimmed
//! backdrop, box centering and event loop live in [`overlay`], not here.
//! Destructive actions go through [`confirm`].
//!
//! Every modal takes a [`Skin`]: the palette drives the colors, while the
//! [`Mode`](crate::theme::Mode) decides chrome details (the `Fancy` mode insets
//! the body with a little padding; `Minimal` stays compact).

use std::{collections::HashSet, io};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use super::{
    footer,
    input::{self, TextCursor},
    layout::centered_rect,
    nav,
    overlay::{self, PopupFlow, popup},
    scroll, style,
    terminal::Tui,
};
use crate::theme::{Palette, Skin};

/// Outcome of a modal interaction.
pub enum ModalSignal<T> {
    /// The user confirmed with a value.
    Value(T),
    /// The user dismissed the modal (Esc).
    Cancelled,
    /// The global quit chord was pressed inside the modal.
    Quit,
}

/// Asks a yes/no question. `Enter`/`y` confirm, `Esc`/`n` decline.
pub fn confirm(
    tui: &mut Tui,
    skin: &Skin,
    prompt: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<bool>> {
    let mut state = ();
    popup(
        tui,
        &mut state,
        |area, (): &()| {
            let width = (prompt.width() as u16 + 6).clamp(28, area.width);
            centered_rect(width, modal_height(skin, 5), area)
        },
        |frame, (): &()| render_bg(frame),
        |frame, rect, (): &()| render_confirm(frame, skin, prompt, rect),
        |(): &mut (), key| match key.code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => PopupFlow::Done(true),
            KeyCode::Char('n' | 'N') | KeyCode::Esc => PopupFlow::Done(false),
            _ => PopupFlow::Continue,
        },
    )
}

/// Prompts for a single line of text. `Enter` accepts, `Esc` cancels.
pub fn input(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<String>> {
    let mut state = TextField {
        cursor: TextCursor::at_end(initial),
        text: initial.to_string(),
    };
    popup(
        tui,
        &mut state,
        |area, _| input_area(skin, area),
        |frame, _| render_bg(frame),
        |frame, rect, field: &TextField| {
            render_input(frame, skin, title, &field.text, &field.cursor, rect);
        },
        |field, key| match key.code {
            KeyCode::Enter => PopupFlow::Done(field.text.clone()),
            KeyCode::Esc => PopupFlow::Cancelled,
            _ => {
                input::apply_edit_key(
                    &mut field.text,
                    &mut field.cursor,
                    key,
                    None,
                );
                PopupFlow::Continue
            }
        },
    )
}

/// Lets the user pick one entry from a list. `Esc` cancels.
pub fn select(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[String],
    initial: usize,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<usize>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut cursor = initial.min(items.len() - 1);
    popup(
        tui,
        &mut cursor,
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            render_picker(frame, skin, title, items, *cursor, None, rect);
        },
        |cursor, key| match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                *cursor = nav::cycle(*cursor, items.len(), -1);
                PopupFlow::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                *cursor = nav::cycle(*cursor, items.len(), 1);
                PopupFlow::Continue
            }
            KeyCode::Enter => PopupFlow::Done(*cursor),
            KeyCode::Esc => PopupFlow::Cancelled,
            _ => PopupFlow::Continue,
        },
    )
}

/// Lets the user toggle several entries. `Space` toggles, `Enter` confirms the
/// selected set, `Esc` cancels.
pub fn multi_select(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[String],
    initial: &[usize],
    check: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Vec<usize>>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut state = MultiSelect::new(initial);
    popup(
        tui,
        &mut state,
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, state: &MultiSelect| {
            let checked = Some((&state.checked, check));
            render_picker(
                frame,
                skin,
                title,
                items,
                state.cursor,
                checked,
                rect,
            );
        },
        |state, key| state.handle_key(key, items.len()),
    )
}

/// Outcome of [`select_reorderable`]: a final pick or a reorder request that
/// the caller applies before reopening.
pub enum ListAction {
    Pick(usize),
    Move { index: usize, delta: i32 },
}

/// Like [`select`] but `Alt+Up`/`Alt+Down` return a [`ListAction::Move`] so the
/// caller can reorder the list and reopen.
pub fn select_reorderable(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[String],
    initial: usize,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<ListAction>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut cursor = initial.min(items.len() - 1);
    popup(
        tui,
        &mut cursor,
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            render_picker(frame, skin, title, items, *cursor, None, rect);
        },
        |cursor, key| {
            let alt = key.modifiers.contains(KeyModifiers::ALT);
            match key.code {
                KeyCode::Up if alt => PopupFlow::Done(ListAction::Move {
                    index: *cursor,
                    delta: -1,
                }),
                KeyCode::Down if alt => PopupFlow::Done(ListAction::Move {
                    index: *cursor,
                    delta: 1,
                }),
                KeyCode::Up | KeyCode::Char('k') => {
                    *cursor = nav::cycle(*cursor, items.len(), -1);
                    PopupFlow::Continue
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    *cursor = nav::cycle(*cursor, items.len(), 1);
                    PopupFlow::Continue
                }
                KeyCode::Enter => PopupFlow::Done(ListAction::Pick(*cursor)),
                KeyCode::Esc => PopupFlow::Cancelled,
                _ => PopupFlow::Continue,
            }
        },
    )
}

/// Like [`select`] but each item carries its own style (for coloured glyphs).
pub fn select_styled(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[(String, Style)],
    initial: usize,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<usize>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut cursor = initial.min(items.len() - 1);
    popup(
        tui,
        &mut cursor,
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            render_styled_picker(
                frame, skin, title, items, *cursor, None, rect,
            );
        },
        |cursor, key| match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                *cursor = nav::cycle(*cursor, items.len(), -1);
                PopupFlow::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                *cursor = nav::cycle(*cursor, items.len(), 1);
                PopupFlow::Continue
            }
            KeyCode::Enter => PopupFlow::Done(*cursor),
            KeyCode::Esc => PopupFlow::Cancelled,
            _ => PopupFlow::Continue,
        },
    )
}

/// Like [`multi_select`] but each item carries its own style.
pub fn multi_select_styled(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[(String, Style)],
    initial: &[usize],
    check: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Vec<usize>>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let mut state = MultiSelect::new(initial);
    popup(
        tui,
        &mut state,
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, state: &MultiSelect| {
            let checked = Some((&state.checked, check));
            let cursor = state.cursor;
            render_styled_picker(
                frame, skin, title, items, cursor, checked, rect,
            );
        },
        |state, key| state.handle_key(key, items.len()),
    )
}

/// Prompts for an integer, accepting digits (and a leading minus) only.
pub fn number_input(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: i64,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<i64>> {
    let mut text = initial.to_string();
    popup(
        tui,
        &mut text,
        |area, _| input_area(skin, area),
        |frame, _| render_bg(frame),
        |frame, rect, text: &String| {
            let cursor = TextCursor::at_end(text);
            render_input(frame, skin, title, text, &cursor, rect);
        },
        |text, key| match key.code {
            KeyCode::Enter => PopupFlow::Done(text.parse::<i64>().unwrap_or(0)),
            KeyCode::Esc => PopupFlow::Cancelled,
            KeyCode::Backspace => {
                text.pop();
                PopupFlow::Continue
            }
            KeyCode::Char(ch)
                if ch.is_ascii_digit() || (ch == '-' && text.is_empty()) =>
            {
                text.push(ch);
                PopupFlow::Continue
            }
            _ => PopupFlow::Continue,
        },
    )
}

/// Shows an informational message until any key is pressed.
pub fn message(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    body: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<()>> {
    let mut state = ();
    popup(
        tui,
        &mut state,
        |area, (): &()| {
            let width = (body.width() as u16 + 6).clamp(28, area.width);
            centered_rect(width, modal_height(skin, 5), area)
        },
        |frame, (): &()| render_bg(frame),
        |frame, rect, (): &()| render_message(frame, skin, title, body, rect),
        |(): &mut (), _| PopupFlow::Done(()),
    )
}

/// The text field state shared by [`input`]: an edit buffer plus its caret.
struct TextField {
    text: String,
    cursor: TextCursor,
}

/// The state shared by the multi-select modals: the cursor plus the toggled set.
struct MultiSelect {
    cursor: usize,
    checked: HashSet<usize>,
}

impl MultiSelect {
    fn new(initial: &[usize]) -> Self {
        Self {
            cursor: 0,
            checked: initial.iter().copied().collect(),
        }
    }

    fn handle_key(
        &mut self,
        key: KeyEvent,
        len: usize,
    ) -> PopupFlow<Vec<usize>> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = nav::cycle(self.cursor, len, -1);
                PopupFlow::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor = nav::cycle(self.cursor, len, 1);
                PopupFlow::Continue
            }
            KeyCode::Char(' ') => {
                if !self.checked.insert(self.cursor) {
                    self.checked.remove(&self.cursor);
                }
                PopupFlow::Continue
            }
            KeyCode::Enter => {
                let mut chosen: Vec<usize> =
                    self.checked.iter().copied().collect();
                chosen.sort_unstable();
                PopupFlow::Done(chosen)
            }
            KeyCode::Esc => PopupFlow::Cancelled,
            _ => PopupFlow::Continue,
        }
    }
}

fn render_confirm(frame: &mut Frame, skin: &Skin, prompt: &str, rect: Rect) {
    let inner = overlay::framed(frame, rect, skin, " Confirm ");
    let width = inner.width as usize;
    let lines = vec![
        Line::from(prompt.to_string()),
        Line::from(""),
        hint(&[("y", "yes"), ("n", "no")], &skin.palette, width),
    ];
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn render_input(
    frame: &mut Frame,
    skin: &Skin,
    title: &str,
    text: &str,
    cursor: &TextCursor,
    rect: Rect,
) {
    let inner = overlay::framed(frame, rect, skin, title);
    let width = inner.width as usize;
    let line = input::render_line(text, cursor, &skin.palette, width, true);
    let lines = vec![
        line,
        Line::from(""),
        hint(&[("enter", "ok"), ("esc", "cancel")], &skin.palette, width),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_message(
    frame: &mut Frame,
    skin: &Skin,
    title: &str,
    body: &str,
    rect: Rect,
) {
    let inner = overlay::framed(frame, rect, skin, title);
    let paragraph = Paragraph::new(body.to_string()).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn render_picker(
    frame: &mut Frame,
    skin: &Skin,
    title: &str,
    items: &[String],
    cursor: usize,
    checked: Option<(&HashSet<usize>, &str)>,
    rect: Rect,
) {
    let inner = overlay::framed(frame, rect, skin, title);
    let entries: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let prefix = check_prefix(checked, index);
            ListItem::new(Line::from(format!("{prefix}{label}")))
        })
        .collect();
    render_picker_list(frame, inner, entries, items.len(), cursor, skin);
}

fn render_styled_picker(
    frame: &mut Frame,
    skin: &Skin,
    title: &str,
    items: &[(String, Style)],
    cursor: usize,
    checked: Option<(&HashSet<usize>, &str)>,
    rect: Rect,
) {
    let inner = overlay::framed(frame, rect, skin, title);
    let entries: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(index, (label, item_style))| {
            let prefix = check_prefix(checked, index);
            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(label.clone(), *item_style),
            ]))
        })
        .collect();
    render_picker_list(frame, inner, entries, items.len(), cursor, skin);
}

/// Renders a picker's list into `inner` with the cursor highlighted, then a
/// scrollbar on the right whenever the entries overflow the visible rows.
fn render_picker_list(
    frame: &mut Frame,
    inner: Rect,
    entries: Vec<ListItem<'_>>,
    total: usize,
    cursor: usize,
    skin: &Skin,
) {
    let mut state = picker_state(cursor);
    frame.render_stateful_widget(picker_list(entries, skin), inner, &mut state);
    scroll::render_scrollbar(
        frame,
        inner,
        total,
        state.offset(),
        inner.height as usize,
    );
}

/// The check-mark (or blank) prefix for a multi-select row, or empty for a
/// single-select list.
fn check_prefix(
    checked: Option<(&HashSet<usize>, &str)>,
    index: usize,
) -> String {
    match checked {
        Some((set, glyph)) if set.contains(&index) => format!("{glyph} "),
        Some(_) => "  ".to_string(),
        None => String::new(),
    }
}

fn picker_list<'a>(entries: Vec<ListItem<'a>>, skin: &Skin) -> List<'a> {
    List::new(entries).highlight_style(
        style::bg(skin.palette.selection_bg).add_modifier(Modifier::BOLD),
    )
}

fn picker_state(cursor: usize) -> ListState {
    let mut state = ListState::default();
    state.select(Some(cursor));
    state
}

/// The popup rect for the list pickers: half the width, one row per item.
fn picker_area(area: Rect, item_count: usize) -> Rect {
    let height =
        (item_count as u16 + 2).clamp(5, area.height.saturating_sub(2));
    let width = (area.width / 2).clamp(30, area.width.saturating_sub(4));
    centered_rect(width, height, area)
}

/// The popup rect for the single-line text inputs.
fn input_area(skin: &Skin, area: Rect) -> Rect {
    let width = area.width.saturating_sub(8).clamp(20, 60);
    centered_rect(width, modal_height(skin, 5), area)
}

/// The modal height for `base` content rows, with one extra row in `Fancy` mode
/// to make room for the vertical padding.
fn modal_height(skin: &Skin, base: u16) -> u16 {
    base + u16::from(skin.is_fancy())
}

fn hint(
    items: &[(&str, &str)],
    palette: &Palette,
    width: usize,
) -> Line<'static> {
    footer::lines(items, palette.accent, width)
        .into_iter()
        .next()
        .unwrap_or_default()
}
