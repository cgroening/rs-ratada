//! The single-select modal whose items can answer to a key: [`select_with`].
//!
//! A unique key picks its item at once; a key shared by several items walks
//! the cursor through them instead, so the user can still choose with `Enter`.

use std::cell::Cell;
use std::io;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::ListItem,
};

use super::ModalSignal;
use super::picker::{navigate_list, navigate_list_without_vim};
use super::render::{picker_area, render_picker_badge, render_picker_list};
use crate::{
    input,
    overlay::{self, PopupFlow, popup},
    style,
    terminal::Tui,
    theme::Skin,
};

/// Columns the `[K] ` key prefix takes, also for a row without a key.
const KEY_PREFIX_BLANK: &str = "    ";

/// One entry of [`select_with`]: its label, its style and the key that picks
/// it.
///
/// # Examples
///
/// ```
/// use ratada::modal::SelectItem;
/// use ratatui::style::{Color, Style};
///
/// let item = SelectItem::new("Kinder")
///     .style(Style::default().fg(Color::Yellow))
///     .hotkey('K');
/// assert_eq!(item.hotkey, Some('K'));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectItem {
    /// The text shown for the entry.
    pub label: String,
    /// The style of the label; the default draws it plain.
    pub style: Style,
    /// The key that picks the entry, or `None` for none. Letters match
    /// regardless of case; a space or a control character counts as none.
    pub hotkey: Option<char>,
}

impl SelectItem {
    /// Creates a plain entry without a key.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: Style::default(),
            hotkey: None,
        }
    }

    /// Returns the entry drawn in `style`.
    #[must_use]
    pub fn style(self, style: Style) -> Self {
        Self { style, ..self }
    }

    /// Returns the entry picked by `key`.
    #[must_use]
    pub fn hotkey(self, key: char) -> Self {
        Self {
            hotkey: Some(key),
            ..self
        }
    }

    /// The entry's key, unless it is one no keypress can match.
    fn usable_hotkey(&self) -> Option<char> {
        self.hotkey
            .filter(|key| !key.is_control() && !key.is_whitespace())
    }

    /// Whether `pressed` is this entry's key (letters case-insensitively).
    fn answers_to(&self, pressed: char) -> bool {
        self.usable_hotkey()
            .is_some_and(|key| key.to_lowercase().eq(pressed.to_lowercase()))
    }
}

/// Opt-in behaviour of [`select_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectOptions {
    /// `j`/`k` move the cursor, as in [`super::select`]. Turn this off when an
    /// item's key is `j` or `k`, which navigation would otherwise take first.
    pub vim_keys: bool,
}

impl Default for SelectOptions {
    /// The behaviour of [`super::select`]: `j`/`k` move the cursor.
    fn default() -> Self {
        Self { vim_keys: true }
    }
}

/// Lets the user pick one entry from `items`, each with its own style and an
/// optional key; `Esc` cancels.
///
/// A key that belongs to exactly one entry picks it at once, like `Enter` on
/// it. A key shared by several entries moves the cursor to the next of them
/// below the cursor, wrapping to the first, and leaves the list open. Ctrl and
/// Alt chords are never keys. When any entry has a key, every row starts with
/// a dimmed `[K] ` (letters shown in upper case), or blanks where an entry has
/// none, so the labels stay aligned. With no keys and default options this is
/// [`super::select_styled`].
///
/// # Errors
///
/// Returns an I/O error when drawing or reading input fails.
pub fn select_with(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[SelectItem],
    initial: usize,
    options: &SelectOptions,
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
            let list = KeyedList { title, items };
            page_rows.set(list.render(frame, skin, *cursor, rect));
        },
        |cursor, key| {
            if navigate(cursor, key, items.len(), page_rows.get(), *options) {
                return PopupFlow::Continue;
            }
            match key.code {
                KeyCode::Enter => PopupFlow::Done(*cursor),
                KeyCode::Esc => PopupFlow::Cancelled,
                _ => apply_hotkey(cursor, key, items),
            }
        },
    )
}

/// Where a pressed key leads in a keyed list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HotkeyHit {
    /// The key belongs to this entry alone: pick it.
    Pick(usize),
    /// The key is shared: move the cursor to this entry.
    Move(usize),
    /// No entry has the key.
    Miss,
}

/// Resolves `pressed` against the keys of `items`, the cursor standing on
/// `cursor`: a unique key picks, a shared one steps to the next match below
/// the cursor (wrapping to the first).
pub(super) fn hotkey_hit(
    items: &[SelectItem],
    cursor: usize,
    pressed: char,
) -> HotkeyHit {
    let matches: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.answers_to(pressed))
        .map(|(index, _)| index)
        .collect();
    match matches.as_slice() {
        [] => HotkeyHit::Miss,
        [only] => HotkeyHit::Pick(*only),
        [first, ..] => {
            let next = matches.iter().find(|&&index| index > cursor);
            HotkeyHit::Move(*next.unwrap_or(first))
        }
    }
}

/// Applies the list-navigation keys, with or without `j`/`k` as `options`
/// say.
fn navigate(
    cursor: &mut usize,
    key: KeyEvent,
    len: usize,
    page: usize,
    options: SelectOptions,
) -> bool {
    if options.vim_keys {
        navigate_list(cursor, key, len, page)
    } else {
        navigate_list_without_vim(cursor, key, len, page)
    }
}

/// Turns a bare character into a pick or a cursor move; any other key leaves
/// the list as it is.
fn apply_hotkey(
    cursor: &mut usize,
    key: KeyEvent,
    items: &[SelectItem],
) -> PopupFlow<usize> {
    let KeyCode::Char(pressed) = key.code else {
        return PopupFlow::Continue;
    };
    if !input::is_bare_character(key) {
        return PopupFlow::Continue;
    }
    match hotkey_hit(items, *cursor, pressed) {
        HotkeyHit::Pick(index) => PopupFlow::Done(index),
        HotkeyHit::Move(index) => {
            *cursor = index;
            PopupFlow::Continue
        }
        HotkeyHit::Miss => PopupFlow::Continue,
    }
}

/// What the keyed picker draws: its frame title and its entries.
struct KeyedList<'a> {
    title: &'a str,
    items: &'a [SelectItem],
}

impl KeyedList<'_> {
    /// Draws the framed list with its key column and position badge, and
    /// returns the viewport height for page-wise navigation.
    fn render(
        &self,
        frame: &mut Frame,
        skin: &Skin,
        cursor: usize,
        rect: Rect,
    ) -> usize {
        let inner = overlay::framed(frame, rect, skin, self.title);
        let has_keys =
            self.items.iter().any(|item| item.usable_hotkey().is_some());
        let key_style = style::fg(skin.palette.foreground_dim);
        let entries: Vec<ListItem> = self
            .items
            .iter()
            .map(|item| {
                let mut spans = Vec::with_capacity(2);
                if has_keys {
                    spans.push(Span::styled(key_prefix(item), key_style));
                }
                spans.push(Span::styled(item.label.clone(), item.style));
                ListItem::new(Line::from(spans))
            })
            .collect();
        let total = self.items.len();
        let viewport =
            render_picker_list(frame, inner, entries, total, cursor, skin);
        render_picker_badge(frame, rect, skin, total, cursor);
        viewport
    }
}

/// The `[K] ` column of one row: the key in upper case, or blanks.
pub(super) fn key_prefix(item: &SelectItem) -> String {
    match item.usable_hotkey() {
        Some(key) => format!("[{}] ", key.to_uppercase()),
        None => KEY_PREFIX_BLANK.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::terminal::{clear_scripted_input, script_keys};
    use crate::theme::{
        ColorOverrides, GlyphVariant, Glyphs, Palette, ThemeRegistry,
    };

    const WIDTH: u16 = 60;
    const HEIGHT: u16 = 16;

    fn skin() -> Skin {
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
        )
    }

    /// "-", Code, Familie, Finanzen, Kinder, plus one entry without a key.
    fn categories() -> Vec<SelectItem> {
        vec![
            SelectItem::new("-").hotkey('-'),
            SelectItem::new("Code").hotkey('C'),
            SelectItem::new("Familie").hotkey('F'),
            SelectItem::new("Finanzen").hotkey('F'),
            SelectItem::new("Kinder").hotkey('K'),
            SelectItem::new("Other"),
        ]
    }

    fn bare(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Runs the picker over `items` with `keys` scripted and returns its
    /// answer; every scripted key must have been read.
    fn run(
        items: &[SelectItem],
        options: SelectOptions,
        keys: Vec<KeyEvent>,
    ) -> ModalSignal<usize> {
        let mut tui = Tui::for_test(WIDTH, HEIGHT).expect("a test terminal");
        script_keys(keys);
        let answer =
            select_with(&mut tui, &skin(), "Pick", items, 0, &options, |_| {})
                .expect("the scripted modal runs");
        clear_scripted_input();
        answer
    }

    fn picked(answer: &ModalSignal<usize>) -> Option<usize> {
        match answer {
            ModalSignal::Value(index) => Some(*index),
            _ => None,
        }
    }

    #[test]
    fn a_unique_key_picks_its_entry() {
        assert_eq!(hotkey_hit(&categories(), 0, 'K'), HotkeyHit::Pick(4));
        assert_eq!(hotkey_hit(&categories(), 0, '-'), HotkeyHit::Pick(0));
    }

    #[test]
    fn letters_match_regardless_of_case() {
        assert_eq!(hotkey_hit(&categories(), 0, 'k'), HotkeyHit::Pick(4));
        assert_eq!(hotkey_hit(&categories(), 0, 'c'), HotkeyHit::Pick(1));
    }

    #[test]
    fn a_shared_key_steps_to_the_next_match_below_the_cursor_and_wraps() {
        let items = categories();
        assert_eq!(hotkey_hit(&items, 0, 'f'), HotkeyHit::Move(2));
        assert_eq!(hotkey_hit(&items, 2, 'f'), HotkeyHit::Move(3));
        assert_eq!(hotkey_hit(&items, 3, 'f'), HotkeyHit::Move(2));
        assert_eq!(hotkey_hit(&items, 5, 'f'), HotkeyHit::Move(2));
    }

    #[test]
    fn a_key_no_entry_has_misses() {
        assert_eq!(hotkey_hit(&categories(), 0, 'x'), HotkeyHit::Miss);
    }

    #[test]
    fn a_space_or_control_character_is_no_key() {
        let items = [
            SelectItem::new("space").hotkey(' '),
            SelectItem::new("tab").hotkey('\t'),
        ];
        assert_eq!(hotkey_hit(&items, 0, ' '), HotkeyHit::Miss);
        assert_eq!(hotkey_hit(&items, 0, '\t'), HotkeyHit::Miss);
        assert_eq!(key_prefix(&items[0]), KEY_PREFIX_BLANK);
    }

    #[test]
    fn the_key_column_shows_the_key_in_upper_case_or_blanks() {
        assert_eq!(key_prefix(&SelectItem::new("x").hotkey('k')), "[K] ");
        assert_eq!(key_prefix(&SelectItem::new("x").hotkey('-')), "[-] ");
        assert_eq!(key_prefix(&SelectItem::new("x")), "    ");
    }

    #[test]
    fn pressing_a_unique_key_closes_the_picker_on_its_entry() {
        let answer = run(
            &categories(),
            SelectOptions::default(),
            vec![bare(KeyCode::Char('K'))],
        );
        assert_eq!(picked(&answer), Some(4));
    }

    #[test]
    fn pressing_a_shared_key_moves_and_enter_confirms() {
        let keys = vec![
            bare(KeyCode::Char('f')),
            bare(KeyCode::Char('f')),
            bare(KeyCode::Enter),
        ];
        let answer = run(&categories(), SelectOptions::default(), keys);
        assert_eq!(picked(&answer), Some(3));
    }

    #[test]
    fn a_ctrl_chord_is_not_a_key() {
        let keys = vec![
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
            bare(KeyCode::Enter),
        ];
        let answer = run(&categories(), SelectOptions::default(), keys);
        assert_eq!(picked(&answer), Some(0));
    }

    /// With vim keys on, navigation takes `k` before the entry's key can.
    #[test]
    fn vim_keys_take_j_and_k_first() {
        let keys = vec![bare(KeyCode::Char('k')), bare(KeyCode::Enter)];
        let answer = run(&categories(), SelectOptions::default(), keys);
        assert_eq!(picked(&answer), Some(5));
    }

    #[test]
    fn without_vim_keys_k_picks_its_entry_and_j_does_not_move() {
        let options = SelectOptions { vim_keys: false };
        let keys = vec![bare(KeyCode::Char('j')), bare(KeyCode::Char('k'))];
        assert_eq!(picked(&run(&categories(), options, keys)), Some(4));
        let keys = vec![bare(KeyCode::Down), bare(KeyCode::Enter)];
        assert_eq!(picked(&run(&categories(), options, keys)), Some(1));
    }

    #[test]
    fn the_rows_carry_the_key_column_only_when_an_entry_has_a_key() {
        let keyed = rendered(&categories());
        assert!(keyed.iter().any(|row| row.contains("[K] Kinder")));
        assert!(keyed.iter().any(|row| row.contains("    Other")));
        let plain = rendered(&[SelectItem::new("Kinder")]);
        assert!(plain.iter().any(|row| row.contains("│Kinder")));
    }

    /// Draws the keyed list on a real frame and returns its rows as text.
    fn rendered(items: &[SelectItem]) -> Vec<String> {
        let backend = TestBackend::new(WIDTH, HEIGHT);
        let mut terminal = Terminal::new(backend).expect("a test terminal");
        terminal
            .draw(|frame| {
                let rect = picker_area(frame.area(), items.len());
                let list = KeyedList {
                    title: "Pick",
                    items,
                };
                list.render(frame, &skin(), 0, rect);
            })
            .expect("the frame draws");
        let buffer = terminal.backend().buffer();
        (0..HEIGHT)
            .map(|y| (0..WIDTH).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }
}
