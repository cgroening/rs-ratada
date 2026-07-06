//! A swatch picker modal: pick one color from a curated, named palette.
//!
//! The colors are a fixed, theme-independent set ([`NAMED_COLORS`]) shown as a
//! scrollable list of swatch + name + hex. `Enter` returns the highlighted
//! color, `Esc` cancels.

use std::{cell::Cell, io};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    text::{Line, Span},
};

use super::{
    layout::centered_rect,
    list,
    modal::ModalSignal,
    nav,
    overlay::{self, PopupFlow, popup},
    style,
    terminal::Tui,
};
use crate::theme::{Color, Skin};

const BOX_WIDTH: u16 = 30;
/// Visible list rows before the list scrolls.
const VISIBLE_ROWS: u16 = 12;
/// Width of the color swatch shown at the start of each row.
const SWATCH_WIDTH: usize = 4;
/// Longest name column, for aligning the hex readout.
const NAME_WIDTH: usize = 12;

/// A curated set of named colors, independent of the active theme. CSS-derived
/// so the names are familiar.
pub const NAMED_COLORS: &[(&str, Color)] = &[
    ("Black", Color::hex("#000000")),
    ("Gray", Color::hex("#808080")),
    ("Silver", Color::hex("#c0c0c0")),
    ("White", Color::hex("#ffffff")),
    ("Slate", Color::hex("#708090")),
    ("Red", Color::hex("#e6194b")),
    ("Crimson", Color::hex("#dc143c")),
    ("Coral", Color::hex("#ff7f50")),
    ("Orange", Color::hex("#ffa500")),
    ("Gold", Color::hex("#ffd700")),
    ("Yellow", Color::hex("#ffe119")),
    ("Olive", Color::hex("#808000")),
    ("Lime", Color::hex("#bfef45")),
    ("Green", Color::hex("#3cb44b")),
    ("Mint", Color::hex("#aaffc3")),
    ("Teal", Color::hex("#469990")),
    ("Cyan", Color::hex("#22d3d3")),
    ("Sky", Color::hex("#87ceeb")),
    ("Blue", Color::hex("#4363d8")),
    ("Navy", Color::hex("#000075")),
    ("Indigo", Color::hex("#4b0082")),
    ("Violet", Color::hex("#911eb4")),
    ("Magenta", Color::hex("#f032e6")),
    ("Pink", Color::hex("#fabed4")),
    ("Rose", Color::hex("#e6007e")),
    ("Brown", Color::hex("#9a6324")),
    ("Tan", Color::hex("#d2b48c")),
    ("Beige", Color::hex("#fffac8")),
];

/// The picker state: the highlighted row and the scroll offset.
struct State {
    selected: usize,
    offset: Cell<usize>,
}

/// Lets the user pick one color from [`NAMED_COLORS`]. `↑`/`↓` (or `k`/`j`) move,
/// `PageUp`/`PageDown` jump a page, `Home`/`End` go to the ends, `Enter` returns
/// the highlighted color and `Esc` cancels. `initial` highlights its matching row.
pub fn swatch_picker(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    initial: Option<Color>,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<Color>> {
    let selected = initial
        .and_then(|color| NAMED_COLORS.iter().position(|(_, c)| *c == color))
        .unwrap_or(0);
    let mut state = State {
        selected,
        offset: Cell::new(0),
    };
    popup(
        tui,
        &mut state,
        |area, _| centered_rect(BOX_WIDTH, VISIBLE_ROWS + 2, area),
        |frame, _| render_bg(frame),
        |frame, rect, state: &State| {
            let inner = overlay::framed(frame, rect, skin, title);
            list::render(
                frame,
                inner,
                skin,
                rows(skin),
                state.selected,
                &state.offset,
            );
        },
        handle,
    )
}

/// Builds the swatch + name + hex row for every named color.
fn rows(skin: &Skin) -> Vec<Line<'static>> {
    NAMED_COLORS
        .iter()
        .map(|(name, color)| {
            Line::from(vec![
                Span::styled(" ".repeat(SWATCH_WIDTH), style::bg(*color)),
                Span::raw(" "),
                Span::styled(
                    format!("{name:<NAME_WIDTH$}"),
                    style::fg(skin.palette.foreground),
                ),
                Span::styled(color.to_hex(), style::secondary(&skin.palette)),
            ])
        })
        .collect()
}

/// Handles navigation, selection and cancellation.
fn handle(state: &mut State, key: KeyEvent) -> PopupFlow<Color> {
    let count = NAMED_COLORS.len();
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            state.selected = nav::cycle(state.selected, count, -1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.selected = nav::cycle(state.selected, count, 1);
        }
        KeyCode::PageUp => {
            state.selected = nav::step_clamped(
                state.selected,
                count,
                -i32::from(VISIBLE_ROWS) as isize,
            );
        }
        KeyCode::PageDown => {
            state.selected = nav::step_clamped(
                state.selected,
                count,
                i32::from(VISIBLE_ROWS) as isize,
            );
        }
        KeyCode::Home => state.selected = 0,
        KeyCode::End => state.selected = count.saturating_sub(1),
        KeyCode::Enter => {
            return PopupFlow::Done(NAMED_COLORS[state.selected].1);
        }
        KeyCode::Esc => return PopupFlow::Cancelled,
        _ => {}
    }
    PopupFlow::Continue
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crossterm::event::KeyModifiers;

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn state() -> State {
        State {
            selected: 0,
            offset: Cell::new(0),
        }
    }

    #[test]
    fn named_colors_are_present_and_uniquely_named() {
        assert!(NAMED_COLORS.len() >= 24);
        let names: HashSet<&str> =
            NAMED_COLORS.iter().map(|(name, _)| *name).collect();
        assert_eq!(names.len(), NAMED_COLORS.len(), "duplicate name");
    }

    #[test]
    fn navigation_wraps_and_clamps() {
        let mut state = state();
        handle(&mut state, key(KeyCode::Up));
        assert_eq!(state.selected, NAMED_COLORS.len() - 1);
        handle(&mut state, key(KeyCode::Down));
        assert_eq!(state.selected, 0);
        handle(&mut state, key(KeyCode::End));
        assert_eq!(state.selected, NAMED_COLORS.len() - 1);
        handle(&mut state, key(KeyCode::Home));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn enter_returns_the_selected_color() {
        let mut state = state();
        state.selected = 3;
        match handle(&mut state, key(KeyCode::Enter)) {
            PopupFlow::Done(color) => assert_eq!(color, NAMED_COLORS[3].1),
            _ => panic!("expected the selected color"),
        }
    }
}
