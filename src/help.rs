//! Scrollable, fuzzy-searchable help overlay listing key bindings in sections.
//!
//! A thin wrapper over [`overlay::popup`]: the dimmed backdrop, box and loop
//! come from there; this module owns the search state and the sectioned body.
//! Bindings are grouped into [`HelpSection`]s; `Tab`/`BackTab` jump between
//! sections, the arrows (plus `PageUp`/`PageDown` and `Home`/`End`) move within
//! the flat list, and typing filters fuzzily while keeping the section headers
//! of any section that still has a match.

use std::io;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use super::{
    chrome,
    filter_list::FilterList,
    fuzzy, input,
    layout::centered_fraction,
    list,
    modal::ModalSignal,
    overlay::{self, PopupFlow, popup_with_paste},
    shortcut_hints, style,
    terminal::Tui,
};
use crate::theme::Skin;

/// The prefix of the query line; its width is taken off the caret line's.
const SEARCH_LABEL: &str = "search ";

/// A titled group of key bindings shown under one header in the overlay.
pub struct HelpSection<'a, B: AsRef<str>> {
    /// The section header.
    pub title: &'a str,
    /// The `(key, description)` bindings listed under the header.
    pub bindings: &'a [(B, B)],
}

/// One rendered row: a section header or a selectable binding.
enum Row<'a> {
    Header(&'a str),
    Item { key: &'a str, description: &'a str },
}

/// The rows to render plus the navigation index maps for the current query.
struct RowLayout<'a> {
    rows: Vec<Row<'a>>,
    /// Row index of each selectable item, in order.
    selectable: Vec<usize>,
    /// Position within `selectable` of each visible section's first item.
    section_starts: Vec<usize>,
}

/// The help overlay's state: the typed query, the cursor and the scroll offset.
///
/// [`show`] owns one for the length of its blocking loop. A host with its own
/// non-blocking event loop keeps one itself and drives it through
/// [`handle_key`], [`HelpState::paste`] and [`render`], so its background work
/// keeps running while the help is open.
pub struct HelpState {
    list: FilterList,
}

impl HelpState {
    /// An empty query with the cursor on the first binding.
    #[must_use]
    pub fn new() -> Self {
        Self {
            list: FilterList::new(),
        }
    }

    /// The filter text typed so far.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.list.query
    }

    /// Appends pasted `text` to the query, dropping control characters, and
    /// moves the cursor back to the first match.
    pub fn paste(&mut self, text: &str) {
        self.list.paste(text);
    }
}

impl Default for HelpState {
    fn default() -> Self {
        Self::new()
    }
}

/// What a key did to the help overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpFlow {
    /// The overlay stays open.
    Continue,
    /// The user closed the overlay (`Esc` or `?`).
    Close,
}

/// Shows the help overlay until the user closes it.
///
/// A query filters the bindings fuzzily (keeping the header of every section
/// that still matches); the arrows (plus `PageUp`/`PageDown` and `Home`/`End`)
/// move the selection, `Tab`/`BackTab` jump
/// to the next/previous section, and `Esc` or `?` close the overlay.
///
/// # Errors
///
/// Returns an I/O error when the terminal cannot be drawn or read.
pub fn show<B: AsRef<str>>(
    tui: &mut Tui,
    skin: &Skin,
    sections: &[HelpSection<'_, B>],
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<()>> {
    let mut state = HelpState::new();
    popup_with_paste(
        tui,
        &mut state,
        |area, _| box_area(area),
        |frame, _| render_bg(frame),
        |frame, rect, state: &HelpState| {
            render_box(frame, rect, skin, sections, state);
        },
        |state, key| match handle_key(state, key, sections) {
            HelpFlow::Continue => PopupFlow::Continue,
            HelpFlow::Close => PopupFlow::Done(()),
        },
        |state, text| {
            state.paste(&text);
            PopupFlow::Continue
        },
    )
}

/// Draws the help overlay centred in `area`, for a host that runs its own
/// event loop (see [`HelpState`]).
///
/// The box is cleared before it is drawn; dimming the view behind it is the
/// host's job, as it is for every other overlay it draws.
pub fn render<B: AsRef<str>>(
    frame: &mut Frame,
    area: Rect,
    skin: &Skin,
    sections: &[HelpSection<'_, B>],
    state: &HelpState,
) {
    let rect = box_area(area);
    frame.render_widget(Clear, rect);
    render_box(frame, rect, skin, sections, state);
}

/// Applies one key to the help overlay `state`.
///
/// `Esc` and `?` close it; everything else is list navigation or filter text,
/// handled by the shared [`FilterList`] core the filtered overlays share. A
/// command chord is ignored, so the host keeps it.
pub fn handle_key<B: AsRef<str>>(
    state: &mut HelpState,
    key: KeyEvent,
    sections: &[HelpSection<'_, B>],
) -> HelpFlow {
    if input::is_command(key) {
        return HelpFlow::Continue;
    }
    if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
        return HelpFlow::Close;
    }
    let layout = layout_rows(sections, &state.list.query);
    state
        .list
        .handle_key(key, layout.selectable.len(), &layout.section_starts);
    HelpFlow::Continue
}

/// The overlay's box within the screen `area`.
fn box_area(area: Rect) -> Rect {
    centered_fraction(area, 2, 3, 40, 8)
}

/// Draws the frame, the body and the position badge into `rect`.
fn render_box<B: AsRef<str>>(
    frame: &mut Frame,
    rect: Rect,
    skin: &Skin,
    sections: &[HelpSection<'_, B>],
    state: &HelpState,
) {
    let inner = overlay::framed(frame, rect, skin, "Help");
    render_body(frame, inner, skin, sections, &state.list);
    // Section headers are rows but not positions: the badge counts the
    // selectable bindings.
    let count = layout_rows(sections, &state.list.query).selectable.len();
    let cursor = state.list.cursor.min(count.saturating_sub(1));
    let badge = chrome::position_badge(cursor, count);
    chrome::render_badge(frame, rect, skin, &badge);
}

/// Builds the rows and navigation index maps for `sections` filtered by `query`.
/// A section is included only if it keeps at least one matching binding; the
/// bindings stay in their original order (no score re-sorting).
fn layout_rows<'a, B: AsRef<str>>(
    sections: &'a [HelpSection<'a, B>],
    query: &str,
) -> RowLayout<'a> {
    let query = query.trim();
    let mut rows: Vec<Row<'a>> = Vec::new();
    let mut selectable: Vec<usize> = Vec::new();
    let mut section_starts: Vec<usize> = Vec::new();

    for section in sections {
        let matches: Vec<&(B, B)> = section
            .bindings
            .iter()
            .filter(|(key, description)| {
                is_match(key.as_ref(), description.as_ref(), query)
            })
            .collect();
        if matches.is_empty() {
            continue;
        }
        section_starts.push(selectable.len());
        rows.push(Row::Header(section.title));
        for (key, description) in matches {
            selectable.push(rows.len());
            rows.push(Row::Item {
                key: key.as_ref(),
                description: description.as_ref(),
            });
        }
    }
    RowLayout {
        rows,
        selectable,
        section_starts,
    }
}

/// Whether a binding matches `query` (everything matches an empty query).
fn is_match(key: &str, description: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    fuzzy::score(&format!("{key} {description}"), query).is_some()
}

fn render_body<B: AsRef<str>>(
    frame: &mut Frame,
    inner: Rect,
    skin: &Skin,
    sections: &[HelpSection<'_, B>],
    state: &FilterList,
) {
    let palette = &skin.palette;
    let layout = layout_rows(sections, &state.query);

    // The popup footer always reserves its row (popup hints ignore the global
    // F1 toggle, which governs only the main-app footer).
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(shortcut_hints::footer_height(1)),
        ])
        .split(inner);

    let mut search =
        vec![Span::styled(SEARCH_LABEL, style::secondary(palette))];
    search.extend(input::query_spans(
        &state.query,
        palette,
        (rows[0].width as usize).saturating_sub(SEARCH_LABEL.len()),
    ));
    frame.render_widget(Paragraph::new(Line::from(search)), rows[0]);

    let header_style =
        style::fg(palette.accent_dim).add_modifier(Modifier::BOLD);
    let entries: Vec<Line<'static>> = layout
        .rows
        .iter()
        .map(|row| match row {
            Row::Header(title) => {
                Line::from(Span::styled(title.to_uppercase(), header_style))
            }
            Row::Item { key, description } => {
                let mut spans = vec![Span::styled(
                    format!("  {key:<12}"),
                    style::fg(palette.accent).add_modifier(Modifier::BOLD),
                )];
                spans.extend(fuzzy::highlight(
                    description,
                    &state.query,
                    style::secondary(palette),
                    palette,
                ));
                Line::from(spans)
            }
        })
        .collect();

    // Delegate to the shared list widget so the cursor highlight, scroll and
    // scrollbar-on-overflow are consistent with every other list. The selected
    // row is the current item's flat row index; headers are never selected.
    let selected = layout
        .selectable
        .get(state.cursor.min(layout.selectable.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0);
    let viewport = list::render(
        frame,
        rows[1],
        skin,
        list::ListView {
            rows: entries,
            selected,
            offset: &state.offset,
        },
    );
    state.viewport.set(viewport);

    let hint = footer_hint(skin, rows[2].width as usize);
    frame.render_widget(Paragraph::new(hint), rows[2]);
}

/// The footer hint line for the overlay.
fn footer_hint(skin: &Skin, width: usize) -> Line<'static> {
    shortcut_hints::lines(
        &[
            ("\u{2191}\u{2193}", "move"),
            ("tab", "section"),
            ("esc", "close"),
        ],
        skin.palette.accent_dim,
        width,
    )
    .into_iter()
    .next()
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    fn state() -> HelpState {
        HelpState::new()
    }

    /// Drives a section jump the way `handle_key` does, through the shared
    /// core, so the test exercises the real path rather than a helper.
    fn jump_section<B: AsRef<str>>(
        state: &mut HelpState,
        sections: &[HelpSection<'_, B>],
        direction: isize,
    ) {
        let code = if direction > 0 {
            KeyCode::Tab
        } else {
            KeyCode::BackTab
        };
        handle_key(state, KeyEvent::new(code, KeyModifiers::NONE), sections);
    }

    fn sections() -> Vec<HelpSection<'static, &'static str>> {
        vec![
            HelpSection {
                title: "Navigation",
                bindings: &[("k", "up"), ("j", "down")],
            },
            HelpSection {
                title: "Tasks",
                bindings: &[("a", "add task"), ("d", "delete")],
            },
        ]
    }

    #[test]
    fn empty_query_keeps_every_section_and_item() {
        let secs = sections();
        let layout = layout_rows(&secs, "");
        // 2 headers + 4 items = 6 rows; 4 selectable; 2 section starts.
        assert_eq!(layout.rows.len(), 6);
        assert_eq!(layout.selectable.len(), 4);
        assert_eq!(layout.section_starts, vec![0, 2]);
    }

    #[test]
    fn query_filters_items_and_drops_empty_sections() {
        let secs = sections();
        let layout = layout_rows(&secs, "add");
        // Only the Tasks section keeps a match ("add task").
        assert_eq!(layout.selectable.len(), 1);
        assert_eq!(layout.section_starts, vec![0]);
        assert!(matches!(layout.rows[0], Row::Header("Tasks")));
    }

    #[test]
    fn section_jump_lands_on_the_first_item_of_the_target() {
        let secs = sections();
        let mut state = state();
        // From the first section, Tab moves to the Tasks section start (index 2
        // in the selectable list).
        jump_section(&mut state, &secs, 1);
        assert_eq!(state.list.cursor, 2);
        // Wraps back to the first section.
        jump_section(&mut state, &secs, 1);
        assert_eq!(state.list.cursor, 0);
        // BackTab from the first section wraps to the last.
        jump_section(&mut state, &secs, -1);
        assert_eq!(state.list.cursor, 2);
    }

    /// A Ctrl chord belongs to the host: without the guard `Ctrl+U` would type
    /// a `u` into the search, `Ctrl+J` walk the list and `Ctrl+?` close the
    /// overlay.
    #[test]
    fn ctrl_chords_do_not_navigate_or_type() {
        let secs = sections();
        for code in [
            KeyCode::Down,
            KeyCode::Up,
            KeyCode::End,
            KeyCode::PageDown,
            KeyCode::Tab,
            KeyCode::Char('u'),
            KeyCode::Char('?'),
        ] {
            let mut state = state();
            let key = KeyEvent::new(code, KeyModifiers::CONTROL);
            assert_eq!(handle_key(&mut state, key, &secs), HelpFlow::Continue);
            assert_eq!(state.list.cursor, 0, "Ctrl+{code:?} moved the cursor");
            assert!(
                state.list.query.is_empty(),
                "Ctrl+{code:?} typed a character"
            );
        }
    }

    #[test]
    fn bare_keys_still_navigate_and_type() {
        let secs = sections();
        let mut state = state();
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
        handle_key(&mut state, press(KeyCode::Down), &secs);
        assert_eq!(state.list.cursor, 1);
        handle_key(&mut state, press(KeyCode::End), &secs);
        assert_eq!(state.list.cursor, 3);
        handle_key(&mut state, press(KeyCode::Char('u')), &secs);
        assert_eq!(state.list.query, "u");
        assert_eq!(state.list.cursor, 0, "typing restarts the selection");
        assert_eq!(
            handle_key(&mut state, press(KeyCode::Esc), &secs),
            HelpFlow::Close
        );
    }

    /// The other half of the rule: `AltGr` is reported as `Ctrl+Alt` yet types
    /// a real character, so the search must accept it. Guarding the `Char` arm
    /// with `is_bare_character` instead of `!is_command` would make `@`, `\`
    /// and `[` untypeable on a German keyboard.
    #[test]
    fn altgr_characters_still_reach_the_filter() {
        let secs = sections();
        let mut state = state();
        for ch in ['@', '\\', '['] {
            handle_key(
                &mut state,
                KeyEvent::new(
                    KeyCode::Char(ch),
                    KeyModifiers::CONTROL | KeyModifiers::ALT,
                ),
                &secs,
            );
        }
        assert_eq!(state.list.query, "@\\[");
    }

    #[test]
    fn paste_drops_control_characters_and_restarts_the_selection() {
        let secs = sections();
        let mut state = state();
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &secs,
        );
        state.paste("ad\nd\t");
        assert_eq!(state.query(), "add");
        assert_eq!(state.list.cursor, 0);
    }

    /// The non-blocking entry point draws at any size a host may hand it,
    /// including areas smaller than the box's minimum.
    #[test]
    fn render_draws_at_any_terminal_size() {
        use ratatui::{Terminal, backend::TestBackend};

        use crate::theme::{
            ColorOverrides, GlyphVariant, Glyphs, Palette, ThemeRegistry,
        };

        let base = ThemeRegistry::builtin().resolve("default");
        let skin = Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
        );
        let secs = sections();
        let mut state = state();
        state.paste("add");
        for (width, height) in [(1, 1), (10, 3), (40, 8), (80, 24), (300, 90)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))
                .expect("the test backend never fails");
            terminal
                .draw(|frame| {
                    render(frame, frame.area(), &skin, &secs, &state);
                })
                .unwrap_or_else(|error| {
                    panic!("{width}x{height} failed: {error}")
                });
        }
    }
}
