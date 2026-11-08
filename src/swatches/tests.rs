use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{
    interaction::{edit, move_horizontal, move_vertical, pick},
    *,
};
use crate::overlay::PopupFlow;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

fn state() -> State {
    let mut state = State {
        mode: Mode::Names,
        cursor: 0,
        offset: Cell::new(0),
        cells: Vec::new(),
        cols: 1,
        palette: vec![("accent", Color::hex("#8bd3cd"))],
        grid_light: GRID_LIGHT_DEFAULT,
        filter: String::new(),
        filtering: false,
        clearing: Clearing::Off,
    };
    state.rebuild();
    state
}

/// A state whose color can be removed.
fn clearable() -> State {
    let mut state = state();
    state.clearing = Clearing::On;
    state.rebuild();
    state
}

#[test]
fn names_mode_matches_named_colors() {
    let state = state();
    assert_eq!(state.cells.len(), NAMED_COLORS.len());
    assert_eq!(state.cells[0].color, NAMED_COLORS[0].1);
}

#[test]
fn m_cycles_all_modes() {
    let mut state = state();
    let seen: Vec<Mode> = (0..4)
        .map(|_| {
            let mode = state.mode;
            handle(&mut state, key(KeyCode::Char('m')));
            mode
        })
        .collect();
    assert_eq!(seen, Mode::ALL);
    assert_eq!(state.mode, Mode::Names);
}

#[test]
fn switching_mode_carries_the_color_close() {
    let mut state = state();
    state.cursor = 6; // Crimson
    let before = state.focus_color();
    handle(&mut state, key(KeyCode::Char('m'))); // -> Grid
    assert_eq!(state.mode, Mode::Grid);
    assert!(state.focus_color().distance(before) < 0.2);
}

#[test]
fn grid_left_wraps_hue_but_up_clamps() {
    let mut state = state();
    state.mode = Mode::Grid;
    state.rebuild();
    state.cursor = 0; // top-left
    move_horizontal(&mut state, -1);
    assert_eq!(state.cursor, GRID_COLS - 1); // wrapped to row end
    state.cursor = 0;
    move_vertical(&mut state, -1);
    assert_eq!(state.cursor, 0); // clamped at the top
}

#[test]
fn list_up_wraps_to_the_end() {
    let mut state = state();
    handle(&mut state, key(KeyCode::Up));
    assert_eq!(state.cursor, NAMED_COLORS.len() - 1);
}

#[test]
fn brackets_change_the_grid_lightness() {
    let mut state = state();
    state.mode = Mode::Grid;
    state.rebuild();
    let before = state.grid_light;
    let color_before = state.focus_color();
    handle(&mut state, key(KeyCode::Char(']')));
    assert!(state.grid_light > before);
    assert_ne!(state.focus_color(), color_before);
}

#[test]
fn filter_narrows_the_named_list() {
    let mut state = state();
    handle(&mut state, key(KeyCode::Char('/')));
    assert!(state.filtering);
    for ch in "crim".chars() {
        handle(&mut state, key(KeyCode::Char(ch)));
    }
    assert!(state.cells.len() < NAMED_COLORS.len());
    assert_eq!(state.cursor, 0);
    assert!(
        state
            .cells
            .iter()
            .any(|cell| cell.name.as_deref() == Some("Crimson"))
    );
}

#[test]
fn enter_picks_and_e_edits_the_focused_color() {
    let mut state = state();
    state.cursor = 3;
    match handle(&mut state, key(KeyCode::Enter)) {
        PopupFlow::Done(Choice::Pick(color)) => {
            assert_eq!(color, NAMED_COLORS[3].1);
        }
        _ => panic!("expected a direct pick"),
    }
    match handle(&mut state, key(KeyCode::Char('e'))) {
        PopupFlow::Done(Choice::Edit(color)) => {
            assert_eq!(color, NAMED_COLORS[3].1);
        }
        _ => panic!("expected an edit hand-off"),
    }
    assert!(matches!(pick(&state), PopupFlow::Done(Choice::Pick(_))));
    assert!(matches!(edit(&state), PopupFlow::Done(Choice::Edit(_))));
}

#[test]
fn space_does_nothing() {
    let mut state = state();
    assert!(matches!(
        handle(&mut state, key(KeyCode::Char(' '))),
        PopupFlow::Continue
    ));
}

#[test]
fn enter_while_filtering_picks_the_first_match() {
    let mut state = state();
    handle(&mut state, key(KeyCode::Char('/')));
    for ch in "crim".chars() {
        handle(&mut state, key(KeyCode::Char(ch)));
    }
    match handle(&mut state, key(KeyCode::Enter)) {
        PopupFlow::Done(Choice::Pick(color)) => {
            assert_eq!(color, Color::hex("#dc143c"));
        }
        _ => panic!("expected a direct pick"),
    }
}

#[test]
fn ctrl_chords_do_not_pick_move_or_switch_mode() {
    let mut state = state();
    state.cursor = 3;
    // Ctrl+E would otherwise open the picker.
    assert!(matches!(
        handle(&mut state, ctrl(KeyCode::Char('e'))),
        PopupFlow::Continue,
    ));
    // Crossterm reports Ctrl+J/Ctrl+H as plain characters, so a bare match
    // on the code alone would move the cursor.
    handle(&mut state, ctrl(KeyCode::Char('j')));
    assert_eq!(state.cursor, 3);
    handle(&mut state, ctrl(KeyCode::Char('m')));
    assert_eq!(state.mode, Mode::Names);
}

#[test]
fn a_ctrl_chord_does_not_type_into_the_filter() {
    let mut state = state();
    handle(&mut state, key(KeyCode::Char('/')));
    handle(&mut state, ctrl(KeyCode::Char('u')));
    assert_eq!(state.filter, "", "Ctrl+U typed a character");
    // A bare character still reaches the filter.
    handle(&mut state, key(KeyCode::Char('u')));
    assert_eq!(state.filter, "u");
}

/// The other half of the rule: `AltGr` is reported as `Ctrl+Alt` yet types
/// a real character, so the filter must accept it. Guarding that arm with
/// `is_bare_character` instead of `!is_command` would make `@`, `\` and `[`
/// untypeable on a German keyboard.
#[test]
fn altgr_characters_still_reach_the_filter() {
    let mut state = state();
    handle(&mut state, key(KeyCode::Char('/')));
    for ch in ['@', '\\', '['] {
        handle(
            &mut state,
            KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
        );
    }
    assert_eq!(state.filter, "@\\[");
}

#[test]
fn the_none_entry_heads_the_lists_only_when_clearable() {
    assert!(!state().cells[0].is_none_entry());
    let mut state = clearable();
    assert!(state.cells[0].is_none_entry(), "Names");
    assert_eq!(state.cells.len(), NAMED_COLORS.len() + 1);
    state.mode = Mode::Palette;
    state.rebuild();
    assert!(state.cells[0].is_none_entry(), "Palette");
    state.mode = Mode::Grid;
    state.rebuild();
    assert!(!state.cells.iter().any(Swatch::is_none_entry), "Grid");
}

#[test]
fn a_filtered_list_has_no_none_entry() {
    let mut state = clearable();
    handle(&mut state, key(KeyCode::Char('/')));
    handle(&mut state, key(KeyCode::Char('r')));
    assert!(!state.cells.iter().any(Swatch::is_none_entry));
}

#[test]
fn enter_on_the_none_entry_removes_the_color_and_e_does_nothing() {
    let mut state = clearable();
    state.cursor = 0;
    assert!(matches!(
        handle(&mut state, key(KeyCode::Enter)),
        PopupFlow::Done(Choice::Clear)
    ));
    assert!(matches!(
        handle(&mut state, key(KeyCode::Char('e'))),
        PopupFlow::Continue
    ));
}

#[test]
fn backspace_and_delete_remove_the_color_only_when_clearable() {
    for code in [KeyCode::Backspace, KeyCode::Delete] {
        assert!(matches!(
            handle(&mut clearable(), key(code)),
            PopupFlow::Done(Choice::Clear)
        ));
        assert!(matches!(
            handle(&mut state(), key(code)),
            PopupFlow::Continue
        ));
    }
}

#[test]
fn backspace_still_edits_the_filter() {
    let mut state = clearable();
    handle(&mut state, key(KeyCode::Char('/')));
    handle(&mut state, key(KeyCode::Char('r')));
    handle(&mut state, key(KeyCode::Backspace));
    assert_eq!(state.filter, "");
    assert!(state.filtering, "still filtering, nothing removed");
}

#[test]
fn nearest_never_lands_on_the_none_entry() {
    let state = clearable();
    let index = nearest(&state.cells, Color::Default);
    assert!(!state.cells[index].is_none_entry());
}

#[test]
fn only_an_exact_named_or_palette_color_is_listed() {
    let palette = [("accent", Color::hex("#8bd3cd"))];
    assert!(is_listed(NAMED_COLORS[0].1, &palette));
    assert!(is_listed(Color::hex("#8bd3cd"), &palette));
    // One step off a named color is a custom color.
    assert!(!is_listed(Color::hex("#000001"), &palette));
}
