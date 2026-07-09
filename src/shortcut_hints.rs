//! Shortcut hints: wrapped `(key, description)` tokens, optionally grouped
//! under aligned labels.
//!
//! [`lines`] lays a flat list of hints into as few rows as fit a width, never
//! splitting a `key desc` token. [`group_lines`] and [`render`] arrange several
//! [`HintGroup`]s one per row with their labels aligned into a left column;
//! a group too wide for one row wraps onto continuation rows indented under
//! that column.
//!
//! # The global toggle
//!
//! Every hint footer in the toolkit is built here, so hiding hints is a single
//! switch: while [`visible`] is false, [`lines`] and [`group_lines`] yield
//! nothing, [`render`] draws nothing (not even its top margin) and [`height`]
//! reports zero rows. `driver::run` and `overlay::popup` consume the toggle
//! chord ([`default_toggle_key`], `F1`), so every screen and every modal
//! inherits it without the host wiring anything up. Hints start out shown.
//!
//! A host that needs the key for itself rebinds or unbinds the chord with
//! [`set_toggle_key`]; unbound, the key reaches its `handle_key` as usual.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::style;
use crate::theme::Color;

const SEPARATOR: &str = " \u{00b7} ";
/// Spaces between the aligned label column and the first hint.
const LABEL_GAP: usize = 2;

/// The description shown beside the hints toggle.
const TOGGLE_LABEL: &str = "toggle hints";
/// The hard quit chord, wired into `terminal::classify` and not rebindable.
const HARD_QUIT: (&str, &str) = ("ctrl+q", "force quit");

thread_local! {
    /// Whether the hint footers are shown.
    ///
    /// A TUI owns one terminal and drives it from one thread, so the preference
    /// lives per thread rather than behind an atomic. It cannot be threaded
    /// through the render calls instead: [`lines`] receives only a `Color`, and
    /// the event loop that flips it has neither a `Skin` nor host state.
    static VISIBLE: Cell<bool> = const { Cell::new(true) };

    /// The chord bound to the hints toggle, or `None` once a host unbound it.
    static TOGGLE: Cell<Option<KeyEvent>> = Cell::new(Some(default_toggle_key()));
}

/// Whether shortcut hints are currently shown.
pub fn visible() -> bool {
    VISIBLE.with(Cell::get)
}

/// Shows or hides every hint footer at once, e.g. to restore a saved session.
pub fn set_visible(show: bool) {
    VISIBLE.with(|flag| flag.set(show));
}

/// Flips the hint visibility; what the global toggle chord does.
pub fn toggle() {
    VISIBLE.with(|flag| flag.set(!flag.get()));
}

/// The chord bound to the hints toggle out of the box: `F1`, no modifiers.
///
/// A function key rather than a `Ctrl+…` chord: it can never be text input, so
/// it stays free inside every text field and modal. `Ctrl+Q` (quit), `Ctrl+S`,
/// `Ctrl+G`, `Ctrl+H` and the editing chords are already spoken for.
pub fn default_toggle_key() -> KeyEvent {
    KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)
}

/// The chord currently toggling the hints, or `None` while it is unbound.
pub fn toggle_key() -> Option<KeyEvent> {
    TOGGLE.with(Cell::get)
}

/// Rebinds the global hints toggle, or unbinds it with `None` so the key
/// reaches the host's own `handle_key` instead. Call it before `run`.
///
/// # Examples
///
/// ```
/// use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
/// use ratada::shortcut_hints::{set_toggle_key, toggle_key};
///
/// set_toggle_key(None);
/// assert!(toggle_key().is_none());
///
/// let chord = KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE);
/// set_toggle_key(Some(chord));
/// assert_eq!(toggle_key(), Some(chord));
/// ```
pub fn set_toggle_key(key: Option<KeyEvent>) {
    TOGGLE.with(|chord| chord.set(key));
}

/// The chords the toolkit itself intercepts, as `(key, description)` tokens:
/// the hints toggle (omitted while unbound) and the hard quit.
///
/// A host appends its own conventional chords (`?`, `q`) from its keymap: only
/// it knows them, and only it notices when the user rebinds them. With the
/// hints hidden the toggle appears nowhere else on screen, so a host that
/// builds a help overlay should list these.
///
/// # Examples
///
/// ```
/// use ratada::shortcut_hints::global_bindings;
///
/// let bindings = global_bindings();
/// assert!(bindings.iter().any(|(key, _)| key == "f1"));
/// assert!(bindings.iter().any(|(key, _)| key == "ctrl+q"));
/// ```
pub fn global_bindings() -> Vec<(String, String)> {
    let mut bindings = Vec::new();
    if let Some(chord) = toggle_key() {
        bindings.push((chord_label(chord), TOGGLE_LABEL.to_string()));
    }
    bindings.push((HARD_QUIT.0.to_string(), HARD_QUIT.1.to_string()));
    bindings
}

/// The rows a hint footer of `rows` lines occupies: `rows` while hints are
/// shown, `0` once they are hidden. For a layout that reserves a fixed footer.
pub fn footer_height(rows: u16) -> u16 {
    if visible() { rows } else { 0 }
}

/// Consumes `key` when it is the bound hints toggle, flipping the visibility.
///
/// Called by `driver::run` and `overlay::popup` before a key reaches the host
/// or a modal's handler, so every surface inherits the chord. Only `code` and
/// `modifiers` are compared: `kind` and `state` vary by terminal.
pub(crate) fn consume_toggle(key: KeyEvent) -> bool {
    let Some(bound) = toggle_key() else {
        return false;
    };
    if key.code != bound.code || key.modifiers != bound.modifiers {
        return false;
    }
    toggle();
    true
}

/// A chord as a footer token: `"f1"`, `"ctrl+h"`, `"shift+enter"`.
fn chord_label(key: KeyEvent) -> String {
    let mut label = String::new();
    for (modifier, name) in [
        (KeyModifiers::CONTROL, "ctrl+"),
        (KeyModifiers::ALT, "alt+"),
        (KeyModifiers::SHIFT, "shift+"),
    ] {
        if key.modifiers.contains(modifier) {
            label.push_str(name);
        }
    }
    label.push_str(&key_label(key.code));
    label
}

/// A key code's own name, without modifiers.
fn key_label(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(ch) => ch.to_lowercase().to_string(),
        KeyCode::F(number) => format!("f{number}"),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::BackTab => "backtab".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Insert => "insert".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// A titled group of key hints. An empty `label` renders without a label
/// column, so the group's hints flow like an ungrouped list.
pub struct HintGroup<'a, S: AsRef<str>> {
    /// The group label (the `"Name:"` cell); an empty label renders flat.
    pub label: &'a str,
    /// The `(key, description)` hints in this group.
    pub hints: &'a [(S, S)],
}

/// Rendering options for [`render`]: a style for each hint part (group label,
/// key and description), the number of blank rows above the hints, and an
/// optional background fill behind them.
pub struct HintStyle {
    /// Style of a group's label (the `"Name:"` cell).
    pub label: Style,
    /// Style of each shortcut key.
    pub key: Style,
    /// Style of each shortcut description.
    pub description: Style,
    /// Blank rows reserved above the hints.
    pub top_margin: u16,
    /// Optional background fill behind the hint area.
    pub background: Option<Color>,
}

impl Default for HintStyle {
    /// Dim labels and descriptions, bold (uncolored) keys, a one-row top margin
    /// and no background.
    fn default() -> Self {
        Self {
            label: style::dim(),
            key: Style::default().add_modifier(Modifier::BOLD),
            description: style::dim(),
            top_margin: 1,
            background: None,
        }
    }
}

/// Wraps `(key, description)` hints into lines at `width` without splitting a
/// token across lines. `key_color` styles the keys (e.g. a dimmed accent).
///
/// Yields nothing while the hints are hidden (see [`visible`]), which is what
/// lets every footer in the toolkit vanish from this one place.
pub fn lines<S: AsRef<str>>(
    items: &[(S, S)],
    key_color: Color,
    width: usize,
) -> Vec<Line<'static>> {
    if !visible() {
        return Vec::new();
    }
    let key_style = style::fg(key_color).add_modifier(Modifier::BOLD);
    wrap(items, key_style, style::dim(), width)
        .into_iter()
        .map(Line::from)
        .collect()
}

/// Lays out `groups` one per row with their labels aligned into a left column,
/// wrapping a group that overflows onto continuation rows indented under the
/// column. Groups without a label (or all groups, if none has one) flow flat.
///
/// Yields nothing while the hints are hidden (see [`visible`]).
pub fn group_lines<S: AsRef<str>>(
    groups: &[HintGroup<'_, S>],
    opts: &HintStyle,
    width: usize,
) -> Vec<Line<'static>> {
    if !visible() {
        return Vec::new();
    }
    let label_col = label_column_width(groups);
    let hint_width = width.saturating_sub(label_col).max(1);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for group in groups {
        let rows = wrap(group.hints, opts.key, opts.description, hint_width);
        for (row_index, mut hint_spans) in rows.into_iter().enumerate() {
            let mut spans: Vec<Span<'static>> = Vec::new();
            if label_col > 0 {
                let is_label_row = row_index == 0 && !group.label.is_empty();
                let cell = if is_label_row {
                    pad(&format!("{}:", group.label), label_col)
                } else {
                    " ".repeat(label_col)
                };
                let cell_style = if is_label_row {
                    opts.label
                } else {
                    Style::default()
                };
                spans.push(Span::styled(cell, cell_style));
            }
            spans.append(&mut hint_spans);
            lines.push(Line::from(spans));
        }
    }
    lines
}

/// The number of rows the grouped hints occupy at `width`, including the
/// `top_margin`. At least one row, or `0` once the hints are hidden, so a
/// caller reclaims the margin along with the hints.
pub fn height<S: AsRef<str>>(
    groups: &[HintGroup<'_, S>],
    width: usize,
    top_margin: u16,
) -> u16 {
    if !visible() {
        return 0;
    }
    // The styles do not affect the line count, only the text does.
    let count = group_lines(groups, &HintStyle::default(), width).len() as u16;
    (count + top_margin).max(1)
}

/// Renders the grouped hints into `area`: `opts.top_margin` blank rows, then
/// the aligned hint lines over `opts.background` (if any). Draws nothing at all
/// while the hints are hidden, margin included.
pub fn render<S: AsRef<str>>(
    frame: &mut Frame,
    area: Rect,
    groups: &[HintGroup<'_, S>],
    opts: &HintStyle,
) {
    if !visible() {
        return;
    }
    let width = area.width as usize;
    let lines = group_lines(groups, opts, width);
    let margin = opts.top_margin.min(area.height);
    let hint_area = Rect {
        x: area.x,
        y: area.y + margin,
        width: area.width,
        height: area.height.saturating_sub(margin),
    };
    let mut paragraph = Paragraph::new(lines);
    if let Some(bg) = opts.background {
        paragraph = paragraph.style(style::bg(bg));
    }
    frame.render_widget(paragraph, hint_area);
}

/// Wraps `(key, description)` tokens into rows no wider than `width`, never
/// splitting a token. Each returned row holds the spans for one line.
fn wrap<S: AsRef<str>>(
    items: &[(S, S)],
    key_style: Style,
    desc_style: Style,
    width: usize,
) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<Span<'static>>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;

    for (key, description) in items {
        let (key, description) = (key.as_ref(), description.as_ref());
        let token_width = format!("{key} {description}").width();
        let separator_width = if spans.is_empty() {
            0
        } else {
            SEPARATOR.width()
        };
        if !spans.is_empty() && used + separator_width + token_width > width {
            rows.push(std::mem::take(&mut spans));
            used = 0;
        }
        if !spans.is_empty() {
            spans.push(Span::styled(SEPARATOR, desc_style));
            used += SEPARATOR.width();
        }
        spans.push(Span::styled(format!("{key} "), key_style));
        spans.push(Span::styled(description.to_string(), desc_style));
        used += token_width;
    }
    if !spans.is_empty() {
        rows.push(spans);
    }
    rows
}

/// The width of the aligned label column (widest `"label:"` plus [`LABEL_GAP`]),
/// or `0` when no group has a label.
fn label_column_width<S: AsRef<str>>(groups: &[HintGroup<'_, S>]) -> usize {
    let widest = groups
        .iter()
        .filter(|group| !group.label.is_empty())
        .map(|group| group.label.width() + 1) // + the ':'
        .max()
        .unwrap_or(0);
    if widest == 0 { 0 } else { widest + LABEL_GAP }
}

/// Right-pads `text` with spaces to `width` (no-op if already at least `width`).
fn pad(text: &str, width: usize) -> String {
    let current = text.width();
    if current >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - current))
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    const ITEMS: &[(&str, &str)] = &[("a", "add"), ("q", "quit")];

    #[test]
    fn fits_on_one_line_when_wide() {
        let result = lines(ITEMS, Color::Default, 80);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn wraps_to_multiple_lines_when_narrow() {
        // "a add" (5) fits; the separator plus "q quit" overflows width 6.
        let result = lines(ITEMS, Color::Default, 6);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn group_labels_align_into_one_column() {
        let nav = [("a", "up")];
        let commands = [("b", "help")];
        let groups = [
            HintGroup {
                label: "Nav",
                hints: &nav,
            },
            HintGroup {
                label: "Commands",
                hints: &commands,
            },
        ];
        let lines = group_lines(&groups, &HintStyle::default(), 80);
        // "Commands" (8) + ':' + gap(2) = 11 columns for both label cells.
        assert_eq!(lines[0].spans[0].content.len(), 11);
        assert_eq!(lines[1].spans[0].content.len(), 11);
        assert!(lines[0].spans[0].content.starts_with("Nav:"));
        assert!(lines[1].spans[0].content.starts_with("Commands:"));
    }

    #[test]
    fn overflowing_group_wraps_indented_under_the_label() {
        let hints = [("aaa", "bbb"), ("ccc", "ddd")];
        let groups = [HintGroup {
            label: "G",
            hints: &hints,
        }];
        // label_col = "G:".len() + gap = 4; hint budget 8 - 4 = 4 forces a wrap.
        let lines = group_lines(&groups, &HintStyle::default(), 8);
        assert_eq!(lines.len(), 2);
        // The continuation row starts with the empty, label-width indent.
        let indent = &lines[1].spans[0].content;
        assert_eq!(indent.len(), 4);
        assert!(indent.trim().is_empty());
    }

    #[test]
    fn labelless_groups_render_flat() {
        let hints = [("a", "add")];
        let groups = [HintGroup {
            label: "",
            hints: &hints,
        }];
        let lines = group_lines(&groups, &HintStyle::default(), 80);
        // No label column: the first span is the key token itself.
        assert_eq!(lines[0].spans[0].content, "a ");
    }

    #[test]
    fn height_counts_lines_plus_margin() {
        let a = [("a", "x")];
        let b = [("b", "y")];
        let groups = [
            HintGroup {
                label: "A",
                hints: &a,
            },
            HintGroup {
                label: "B",
                hints: &b,
            },
        ];
        assert_eq!(height(&groups, 80, 1), 3);
        assert_eq!(height(&groups, 80, 0), 2);
    }

    // --- The global toggle ---
    //
    // `VISIBLE` is thread-local, so these tests cannot race the ones above (or
    // `tests/render.rs`), which run on other threads. Several tests do share a
    // thread, though, so each one restores the flag before returning.

    /// Runs `body` with the hints hidden, restoring the flag afterwards.
    fn while_hidden(body: impl FnOnce()) {
        let before = visible();
        set_visible(false);
        body();
        set_visible(before);
    }

    #[test]
    fn hints_start_out_visible() {
        assert!(visible());
    }

    #[test]
    fn toggle_flips_the_visibility_back_and_forth() {
        let before = visible();
        toggle();
        assert_eq!(visible(), !before);
        toggle();
        assert_eq!(visible(), before);
    }

    #[test]
    fn hidden_hints_yield_no_lines() {
        while_hidden(|| {
            assert!(lines(ITEMS, Color::Default, 80).is_empty());
            let groups = [HintGroup {
                label: "A",
                hints: ITEMS,
            }];
            assert!(group_lines(&groups, &HintStyle::default(), 80).is_empty());
        });
    }

    #[test]
    fn hidden_hints_reclaim_their_rows_and_the_top_margin() {
        while_hidden(|| {
            let groups = [HintGroup {
                label: "A",
                hints: ITEMS,
            }];
            assert_eq!(height(&groups, 80, 1), 0);
            assert_eq!(footer_height(1), 0);
            assert_eq!(footer_height(2), 0);
        });
    }

    #[test]
    fn footer_height_passes_the_rows_through_while_visible() {
        assert!(visible());
        assert_eq!(footer_height(1), 1);
        assert_eq!(footer_height(2), 2);
    }

    /// Runs `body` with `key` bound as the toggle, restoring the binding after.
    fn with_toggle_key(key: Option<KeyEvent>, body: impl FnOnce()) {
        let before = toggle_key();
        set_toggle_key(key);
        body();
        set_toggle_key(before);
    }

    #[test]
    fn the_toggle_key_is_consumed_and_flips_the_visibility() {
        let before = visible();
        let key = default_toggle_key();
        assert!(consume_toggle(key));
        assert_eq!(visible(), !before);
        // Restore, which also exercises the round trip.
        assert!(consume_toggle(key));
        assert_eq!(visible(), before);
    }

    #[test]
    fn any_other_key_passes_through_untouched() {
        let before = visible();
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(!consume_toggle(key));
        assert_eq!(visible(), before);
    }

    #[test]
    fn the_toggle_chord_matches_its_modifiers_exactly() {
        let before = visible();
        // `Shift+F1` is a different chord than the bound `F1`.
        let shifted = KeyEvent::new(KeyCode::F(1), KeyModifiers::SHIFT);
        assert!(!consume_toggle(shifted));
        assert_eq!(visible(), before);
    }

    #[test]
    fn an_unbound_toggle_lets_the_key_reach_the_host() {
        with_toggle_key(None, || {
            let before = visible();
            assert!(!consume_toggle(default_toggle_key()));
            assert_eq!(visible(), before);
        });
    }

    #[test]
    fn a_rebound_toggle_replaces_the_default_chord() {
        let rebound = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL);
        with_toggle_key(Some(rebound), || {
            let before = visible();
            assert!(!consume_toggle(default_toggle_key()));
            assert_eq!(visible(), before);
            assert!(consume_toggle(rebound));
            assert_eq!(visible(), !before);
            toggle();
        });
    }

    #[test]
    fn global_bindings_document_the_toggle_and_the_quit_chord() {
        let bindings = global_bindings();
        let keys: Vec<&str> =
            bindings.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["f1", "ctrl+q"]);
    }

    #[test]
    fn global_bindings_follow_the_toggle_binding() {
        let rebound = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL);
        with_toggle_key(Some(rebound), || {
            let bindings = global_bindings();
            assert_eq!(bindings[0].0, "ctrl+h");
        });
        with_toggle_key(None, || {
            let bindings = global_bindings();
            // Only the hard quit is left; the toggle has no key to name.
            assert_eq!(bindings.len(), 1);
            assert_eq!(bindings[0].0, "ctrl+q");
        });
    }

    #[test]
    fn chord_label_names_modifiers_and_keys() {
        let label =
            |code, modifiers| chord_label(KeyEvent::new(code, modifiers));
        assert_eq!(label(KeyCode::F(1), KeyModifiers::NONE), "f1");
        assert_eq!(label(KeyCode::Char('h'), KeyModifiers::CONTROL), "ctrl+h");
        assert_eq!(label(KeyCode::Enter, KeyModifiers::SHIFT), "shift+enter");
        assert_eq!(label(KeyCode::Esc, KeyModifiers::NONE), "esc");
        assert_eq!(label(KeyCode::Char(' '), KeyModifiers::NONE), "space");
    }
}
