//! Reusable modal widgets. Each is a thin wrapper over [`overlay::popup`]: it
//! sets up its state and closures and returns a [`ModalSignal`]. The dimmed
//! backdrop, box centering and event loop live in [`overlay`], not here.
//!
//! Yes/no questions go through [`confirm`], which lets `Enter` mean yes. A
//! destructive action goes through [`confirm_default`] with
//! [`Question::declining`] instead, so a stray `Enter` cannot confirm the
//! deletion.
//!
//! Every modal takes a [`Skin`], whose palette drives the colors.

use std::{cell::Cell, collections::HashSet, io};

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
    chrome,
    input::{self, TextCursor},
    layout::{centered_rect, fit},
    nav,
    overlay::{self, PopupFlow, popup, popup_with_paste},
    scroll, shortcut_hints, style,
    terminal::Tui,
};
use crate::theme::{Palette, Skin};

/// Rows a modal's hint block occupies while the hints are shown: a blank
/// spacer and the hint line itself.
const HINT_BLOCK_ROWS: u16 = 2;

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
///
/// For a destructive prompt reach for [`confirm_default`] with
/// [`Question::declining`], which makes `Enter` decline instead.
pub fn confirm(
    tui: &mut Tui,
    skin: &Skin,
    prompt: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<bool>> {
    confirm_default(tui, skin, &Question::new(prompt), render_bg)
}

/// A yes/no question and which way a bare `Enter` answers it.
#[derive(Debug, Clone, Copy)]
pub struct Question<'a> {
    /// The question shown in the dialog.
    pub prompt: &'a str,
    /// What `Enter` answers. `y`/`n` always answer explicitly, `Esc` declines.
    pub default_yes: bool,
}

impl<'a> Question<'a> {
    /// A question a bare `Enter` confirms.
    #[must_use]
    pub fn new(prompt: &'a str) -> Self {
        Self {
            prompt,
            default_yes: true,
        }
    }

    /// A question a bare `Enter` declines - the safe default for a destructive
    /// action, where an absent-minded `Enter` must not delete anything.
    #[must_use]
    pub fn declining(prompt: &'a str) -> Self {
        Self {
            prompt,
            default_yes: false,
        }
    }

    /// The footer hints, binding `enter` to whichever answer it gives.
    fn hints(&self) -> [(&'static str, &'static str); 2] {
        if self.default_yes {
            [("enter/y", "yes"), ("n", "no")]
        } else {
            [("y", "yes"), ("enter/n", "no")]
        }
    }
}

/// Asks a yes/no question whose `Enter` answer the caller chooses.
///
/// `y` confirms and `n` declines regardless; `Esc` always declines. Use
/// [`Question::declining`] for a destructive prompt so `Enter` cannot confirm
/// it by accident, and [`confirm`] when `Enter` should mean yes.
pub fn confirm_default(
    tui: &mut Tui,
    skin: &Skin,
    question: &Question<'_>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<bool>> {
    let prompt = question.prompt;
    let default_yes = question.default_yes;
    let mut state = ();
    popup(
        tui,
        &mut state,
        |area, (): &()| {
            let width = fit(prompt.width() as u16 + 6, 28, area.width);
            centered_rect(width, hinted_box_height(), area)
        },
        |frame, (): &()| render_bg(frame),
        |frame, rect, (): &()| render_confirm(frame, skin, question, rect),
        |(): &mut (), key| match key.code {
            KeyCode::Char('y' | 'Y') => PopupFlow::Done(true),
            KeyCode::Char('n' | 'N') | KeyCode::Esc => PopupFlow::Done(false),
            KeyCode::Enter => PopupFlow::Done(default_yes),
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
    input_impl(tui, skin, title, initial, input_area, render_bg)
}

/// Like [`input()`], but the box spans most of the terminal width, so a long
/// value (such as a file path) stays visible instead of scrolling in a narrow
/// box. `Enter` accepts, `Esc` cancels.
pub fn input_wide(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: &str,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<String>> {
    input_impl(tui, skin, title, initial, input_area_wide, render_bg)
}

/// Shared single-line text prompt; `area` sizes the box from the frame.
fn input_impl(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: &str,
    area: impl Fn(Rect) -> Rect,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<String>> {
    let mut state = TextField {
        cursor: TextCursor::at_end(initial),
        text: initial.to_string(),
    };
    popup_with_paste(
        tui,
        &mut state,
        |rect, _| area(rect),
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
                    input::EditMode::SingleLine,
                    None,
                );
                PopupFlow::Continue
            }
        },
        |field: &mut TextField, text| {
            input::paste_text(
                &mut field.text,
                &mut field.cursor,
                input::EditMode::SingleLine,
                None,
                &text,
            );
            PopupFlow::Continue
        },
    )
}

/// Applies the shared list-navigation keys to `cursor` over `len` items,
/// returning whether the key was one of them. `Up`/`Down` (and `k`/`j`) wrap
/// cyclically; `PageUp`/`PageDown` move by `page` rows and `Home`/`End` jump to
/// the ends, both clamped. The caller handles the picker's own keys (`Enter`,
/// `Esc`, ...) for the keys this leaves unconsumed.
fn navigate_list(
    cursor: &mut usize,
    key: KeyEvent,
    len: usize,
    page: usize,
) -> bool {
    let page = page.max(1) as isize;
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            *cursor = nav::cycle(*cursor, len, -1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            *cursor = nav::cycle(*cursor, len, 1);
        }
        KeyCode::PageUp => *cursor = nav::step_clamped(*cursor, len, -page),
        KeyCode::PageDown => *cursor = nav::step_clamped(*cursor, len, page),
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = len.saturating_sub(1),
        _ => return false,
    }
    true
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
    let page_rows = Cell::new(1usize);
    popup(
        tui,
        &mut cursor,
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            let viewport =
                render_picker(frame, skin, title, items, *cursor, None, rect);
            page_rows.set(viewport);
        },
        |cursor, key| {
            if navigate_list(cursor, key, items.len(), page_rows.get()) {
                return PopupFlow::Continue;
            }
            match key.code {
                KeyCode::Enter => PopupFlow::Done(*cursor),
                KeyCode::Esc => PopupFlow::Cancelled,
                _ => PopupFlow::Continue,
            }
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
            let viewport = render_picker(
                frame,
                skin,
                title,
                items,
                state.cursor,
                checked,
                rect,
            );
            state.viewport.set(viewport);
        },
        |state, key| state.handle_key(key, items.len()),
    )
}

/// Outcome of [`select_reorderable`]: a final pick or a reorder request that
/// the caller applies before reopening.
pub enum ListAction {
    /// The user picked the item at this index.
    Pick(usize),
    /// The user asked to move the item at `index` by `delta` positions.
    Move {
        /// The index of the item to move.
        index: usize,
        /// The signed number of positions to move it by.
        delta: i32,
    },
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
    let page_rows = Cell::new(1usize);
    popup(
        tui,
        &mut cursor,
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            let viewport =
                render_picker(frame, skin, title, items, *cursor, None, rect);
            page_rows.set(viewport);
        },
        |cursor, key| {
            let alt = key.modifiers.contains(KeyModifiers::ALT);
            // Alt+Up/Down reorder; the plain motions fall to the shared nav.
            match key.code {
                KeyCode::Up if alt => {
                    return PopupFlow::Done(ListAction::Move {
                        index: *cursor,
                        delta: -1,
                    });
                }
                KeyCode::Down if alt => {
                    return PopupFlow::Done(ListAction::Move {
                        index: *cursor,
                        delta: 1,
                    });
                }
                _ => {}
            }
            if navigate_list(cursor, key, items.len(), page_rows.get()) {
                return PopupFlow::Continue;
            }
            match key.code {
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
    let page_rows = Cell::new(1usize);
    popup(
        tui,
        &mut cursor,
        |area, _| picker_area(area, items.len()),
        |frame, _| render_bg(frame),
        |frame, rect, cursor: &usize| {
            let viewport = render_styled_picker(
                frame, skin, title, items, *cursor, None, rect,
            );
            page_rows.set(viewport);
        },
        |cursor, key| {
            if navigate_list(cursor, key, items.len(), page_rows.get()) {
                return PopupFlow::Continue;
            }
            match key.code {
                KeyCode::Enter => PopupFlow::Done(*cursor),
                KeyCode::Esc => PopupFlow::Cancelled,
                _ => PopupFlow::Continue,
            }
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
            let viewport = render_styled_picker(
                frame, skin, title, items, cursor, checked, rect,
            );
            state.viewport.set(viewport);
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
    number_impl(tui, skin, title, initial, None, render_bg)
}

/// Like [`number_input`], but the accepted value is clamped to `[min, max]`.
/// `Enter` accepts (clamping), `Esc` cancels.
pub fn number_input_bounded(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: i64,
    min: i64,
    max: i64,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<i64>> {
    number_impl(tui, skin, title, initial, Some((min, max)), render_bg)
}

/// Shared integer prompt; `bounds` clamps the accepted value when set.
fn number_impl(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: i64,
    bounds: Option<(i64, i64)>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<i64>> {
    let mut text = initial.to_string();
    popup_with_paste(
        tui,
        &mut text,
        |area, _| input_area(area),
        |frame, _| render_bg(frame),
        |frame, rect, text: &String| {
            let cursor = TextCursor::at_end(text);
            render_input(frame, skin, title, text, &cursor, rect);
        },
        |text, key| match key.code {
            KeyCode::Enter => {
                let value = text.parse::<i64>().unwrap_or(initial);
                let value =
                    bounds.map_or(value, |(min, max)| value.clamp(min, max));
                PopupFlow::Done(value)
            }
            KeyCode::Esc => PopupFlow::Cancelled,
            KeyCode::Backspace => {
                text.pop();
                PopupFlow::Continue
            }
            KeyCode::Char(ch) if is_number_char(ch, text) => {
                text.push(ch);
                PopupFlow::Continue
            }
            _ => PopupFlow::Continue,
        },
        |text: &mut String, pasted| {
            for ch in pasted.chars() {
                if is_number_char(ch, text) {
                    text.push(ch);
                }
            }
            PopupFlow::Continue
        },
    )
}

/// Whether `ch` may extend the number buffer `text`: a digit, or a leading `-`
/// only while the buffer is still empty.
fn is_number_char(ch: char, text: &str) -> bool {
    ch.is_ascii_digit() || (ch == '-' && text.is_empty())
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
            let width = fit(body.width() as u16 + 6, 28, area.width);
            centered_rect(width, 5, area)
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

/// The state shared by the multi-select modals: the cursor, the toggled set and
/// the list viewport height captured at render (for page-wise navigation).
struct MultiSelect {
    cursor: usize,
    checked: HashSet<usize>,
    viewport: Cell<usize>,
}

impl MultiSelect {
    fn new(initial: &[usize]) -> Self {
        Self {
            cursor: 0,
            checked: initial.iter().copied().collect(),
            viewport: Cell::new(1),
        }
    }

    fn handle_key(
        &mut self,
        key: KeyEvent,
        len: usize,
    ) -> PopupFlow<Vec<usize>> {
        if navigate_list(&mut self.cursor, key, len, self.viewport.get()) {
            return PopupFlow::Continue;
        }
        match key.code {
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

fn render_confirm(
    frame: &mut Frame,
    skin: &Skin,
    question: &Question<'_>,
    rect: Rect,
) {
    let inner = overlay::framed(frame, rect, skin, " Confirm ");
    let width = inner.width as usize;
    let mut lines = vec![Line::from(question.prompt.to_string())];
    lines.extend(hint_block(&question.hints(), &skin.palette, width));
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
    let mut lines = vec![line];
    lines.extend(hint_block(
        &[("enter", "ok"), ("esc", "cancel")],
        &skin.palette,
        width,
    ));
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

/// Renders a single-column picker and returns its viewport height (visible
/// rows), for page-wise navigation.
fn render_picker(
    frame: &mut Frame,
    skin: &Skin,
    title: &str,
    items: &[String],
    cursor: usize,
    checked: Option<(&HashSet<usize>, &str)>,
    rect: Rect,
) -> usize {
    let inner = overlay::framed(frame, rect, skin, title);
    let entries: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let prefix = check_prefix(checked, index);
            ListItem::new(Line::from(format!("{prefix}{label}")))
        })
        .collect();
    let viewport =
        render_picker_list(frame, inner, entries, items.len(), cursor, skin);
    render_picker_badge(frame, rect, skin, items.len(), cursor);
    viewport
}

/// Renders a styled-label picker and returns its viewport height (visible
/// rows), for page-wise navigation.
fn render_styled_picker(
    frame: &mut Frame,
    skin: &Skin,
    title: &str,
    items: &[(String, Style)],
    cursor: usize,
    checked: Option<(&HashSet<usize>, &str)>,
    rect: Rect,
) -> usize {
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
    let viewport =
        render_picker_list(frame, inner, entries, items.len(), cursor, skin);
    render_picker_badge(frame, rect, skin, items.len(), cursor);
    viewport
}

/// Draws the `position/total` badge into the picker frame's bottom border.
/// `rect` is the framed box, not the list inside it.
fn render_picker_badge(
    frame: &mut Frame,
    rect: Rect,
    skin: &Skin,
    total: usize,
    cursor: usize,
) {
    let badge = chrome::position_badge(cursor, total);
    chrome::render_badge(frame, rect, skin, &badge);
}

/// Renders a picker's list into `inner` with the cursor highlighted, then a
/// scrollbar on the right whenever the entries overflow the visible rows.
/// Returns the viewport height (the number of visible rows), so the caller can
/// drive page-wise navigation.
fn render_picker_list(
    frame: &mut Frame,
    inner: Rect,
    entries: Vec<ListItem<'_>>,
    total: usize,
    cursor: usize,
    skin: &Skin,
) -> usize {
    let mut state = picker_state(cursor);
    frame.render_stateful_widget(picker_list(entries, skin), inner, &mut state);
    let viewport = inner.height as usize;
    scroll::render_scrollbar(
        frame,
        inner,
        skin,
        nav::ScrollView {
            total,
            offset: state.offset(),
            viewport,
        },
    );
    viewport
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
        style::bg(skin.palette.selection).add_modifier(Modifier::BOLD),
    )
}

fn picker_state(cursor: usize) -> ListState {
    let mut state = ListState::default();
    state.select(Some(cursor));
    state
}

/// The popup rect for the list pickers: half the width, one row per item.
fn picker_area(area: Rect, item_count: usize) -> Rect {
    let height = fit(item_count as u16 + 2, 5, area.height.saturating_sub(2));
    let width = fit(area.width / 2, 30, area.width.saturating_sub(4));
    centered_rect(width, height, area)
}

/// The popup rect for the single-line text inputs.
fn input_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(8).clamp(20, 60);
    centered_rect(width, hinted_box_height(), area)
}

/// A wide input box (~90% of the terminal width), for long values.
fn input_area_wide(area: Rect) -> Rect {
    let width = fit(area.width * 9 / 10, 20, area.width);
    centered_rect(width, hinted_box_height(), area)
}

/// The blank spacer and the hint line closing a modal body. Both vanish
/// together while the hints are hidden, which is why the spacer lives here and
/// not at the call site.
fn hint_block(
    items: &[(&str, &str)],
    palette: &Palette,
    width: usize,
) -> Vec<Line<'static>> {
    shortcut_hints::lines(items, palette.accent, width)
        .into_iter()
        .take(1)
        .flat_map(|hint| [Line::from(""), hint])
        .collect()
}

/// The height of a modal box whose body is one row plus a [`hint_block`]:
/// two border rows, the row itself, and the hint block when shown.
fn hinted_box_height() -> u16 {
    3 + shortcut_hints::footer_height(HINT_BLOCK_ROWS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn navigate_list_wraps_on_arrows_and_clamps_on_page_and_ends() {
        let mut cursor = 0;
        // Up wraps to the last item.
        assert!(navigate_list(&mut cursor, key(KeyCode::Up), 5, 2));
        assert_eq!(cursor, 4);
        // PageUp clamps at the top rather than wrapping.
        assert!(navigate_list(&mut cursor, key(KeyCode::PageUp), 5, 2));
        assert_eq!(cursor, 2);
        assert!(navigate_list(&mut cursor, key(KeyCode::PageUp), 5, 2));
        assert_eq!(cursor, 0);
        // End and Home jump to the last and first item.
        assert!(navigate_list(&mut cursor, key(KeyCode::End), 5, 2));
        assert_eq!(cursor, 4);
        assert!(navigate_list(&mut cursor, key(KeyCode::Home), 5, 2));
        assert_eq!(cursor, 0);
        // A non-navigation key is left for the caller.
        assert!(!navigate_list(&mut cursor, key(KeyCode::Enter), 5, 2));
    }

    /// Every popup wants a minimum width or height. A terminal smaller than
    /// that must shrink the popup, not panic: these helpers used to reach
    /// `clamp(min, max)` with `max < min`.
    #[test]
    fn popup_geometry_survives_a_terminal_below_its_minimum() {
        for (width, height) in [(1, 1), (4, 2), (20, 6), (27, 10)] {
            let area = Rect::new(0, 0, width, height);
            for rect in [
                picker_area(area, 40),
                input_area(area),
                input_area_wide(area),
            ] {
                assert!(rect.width <= area.width, "{rect:?} in {area:?}");
                assert!(rect.height <= area.height, "{rect:?} in {area:?}");
            }
        }
    }

    #[test]
    fn a_roomy_terminal_still_gets_the_preferred_size() {
        let area = Rect::new(0, 0, 100, 40);
        let picker = picker_area(area, 4);
        assert_eq!(picker.width, 50); // half the width
        assert_eq!(picker.height, 6); // one row per item, plus borders
    }

    #[test]
    fn a_plain_question_lets_enter_confirm() {
        let question = Question::new("Save the file?");
        assert!(question.default_yes);
        assert_eq!(question.hints(), [("enter/y", "yes"), ("n", "no")]);
    }

    /// The point of `declining`: a stray `Enter` on a destructive prompt must
    /// answer "no", and the footer must advertise that binding.
    #[test]
    fn a_declining_question_lets_enter_decline() {
        let question = Question::declining("Delete everything?");
        assert!(!question.default_yes);
        assert_eq!(question.hints(), [("y", "yes"), ("enter/n", "no")]);
    }
}
