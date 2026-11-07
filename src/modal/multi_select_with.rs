//! The multi-select modal with two switchable, complementary modes:
//! [`multi_select_with`].
//!
//! A mode pair such as `hide`/`show` reads the same checks two ways. `Tab`
//! switches the mode and inverts every check, so what the checks select stays
//! the same; the user then edits the set in whichever reading is shorter.

use std::collections::HashSet;
use std::io;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, style::Style};

use super::ModalSignal;
use super::SelectItem;
use super::multi_state::{CONFIRM_HINTS, MultiSelect};
use super::render::{
    CHECK_PREFIX_WIDTH, PickerChrome, labels_width, picker_area,
    render_styled_picker,
};
use crate::{
    input,
    overlay::{PopupFlow, popup},
    terminal::Tui,
    theme::Skin,
};

/// Opt-in behaviour of [`multi_select_with`]; the default is the plain
/// [`super::multi_select_styled`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MultiSelectOptions {
    /// Two complementary modes, e.g. `["hide", "show"]`. `Tab` switches
    /// between them and inverts every check. `None` binds no `Tab`.
    pub modes: Option<[String; 2]>,
    /// The mode the modal opens in (`0` or `1`; anything else reads as `1`).
    pub initial_mode: usize,
    /// `Space` ticks the row and moves to the next one, so several rows are
    /// ticked one after another; on the last row the cursor stays. `false`
    /// keeps the cursor where it is.
    pub advance_on_tick: bool,
    /// The row the cursor opens on (clamped to the last row).
    pub initial_cursor: usize,
}

/// What [`multi_select_with`] returns: the checked indices (ascending) and the
/// mode they were confirmed in (`0` without modes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiChoice {
    /// The checked item indices, in ascending order.
    pub checked: Vec<usize>,
    /// The index of the active mode in [`MultiSelectOptions::modes`].
    pub mode: usize,
}

/// Lets the user check several of `items`, each drawn in its own style.
/// `Space` toggles, `Enter` confirms, `Esc` cancels; with nothing checked,
/// `Enter` takes the cursor row as in [`super::multi_select`].
///
/// With [`MultiSelectOptions::modes`], `Tab` switches to the other mode and
/// inverts every check, and the frame title reads `title: mode (Tab: other)`.
/// The inversion counts as an edit, so an empty set it leaves is confirmed as
/// such. With [`MultiSelectOptions::advance_on_tick`], `Space` also moves to
/// the next row. The items' keys ([`SelectItem::hotkey`]) are not used here.
///
/// # Errors
///
/// Returns an I/O error when drawing or reading input fails.
pub fn multi_select_with(
    tui: &mut Tui,
    skin: &Skin,
    title: &str,
    items: &[SelectItem],
    checked: &[usize],
    options: &MultiSelectOptions,
    render_bg: impl Fn(&mut Frame),
) -> io::Result<ModalSignal<MultiChoice>> {
    if items.is_empty() {
        return Ok(ModalSignal::Cancelled);
    }
    let rows: Vec<(String, Style)> = items
        .iter()
        .map(|item| (item.label.clone(), item.style))
        .collect();
    let list = MultiSelect::new(checked)
        .starting_at(options.initial_cursor, items.len());
    let list = if options.advance_on_tick {
        list.advancing_on_tick()
    } else {
        list
    };
    let mut state = Moded {
        list,
        mode: options.initial_mode.min(1),
    };
    let modes = options.modes.as_ref();
    popup(
        tui,
        &mut state,
        |area, _| {
            let labels = items.iter().map(|item| item.label.as_str());
            let width = labels_width(labels) + CHECK_PREFIX_WIDTH;
            picker_area(area, items.len(), width, &CONFIRM_HINTS)
        },
        |frame, _| render_bg(frame),
        |frame, rect, state: &Moded| {
            let shown = moded_title(title, modes, state.mode);
            let hints = state.list.hints();
            let chrome = PickerChrome {
                title: &shown,
                cursor: state.list.cursor,
                checked: Some((&state.list.checked, skin.glyphs.mark)),
                hints: &hints,
            };
            let viewport =
                render_styled_picker(frame, skin, &rows, &chrome, rect);
            state.list.viewport.set(viewport);
        },
        |state, key| state.handle_key(key, items.len(), modes.is_some()),
    )
}

/// The multi-select state plus the active mode.
struct Moded {
    list: MultiSelect,
    mode: usize,
}

impl Moded {
    /// Switches the mode on `Tab` (when there are modes) and hands every other
    /// key to the plain multi-select.
    fn handle_key(
        &mut self,
        key: KeyEvent,
        len: usize,
        has_modes: bool,
    ) -> PopupFlow<MultiChoice> {
        if has_modes && key.code == KeyCode::Tab && !input::is_command(key) {
            self.mode = 1 - self.mode;
            self.list.replace_checks(inverted(&self.list.checked, len));
            return PopupFlow::Continue;
        }
        match self.list.handle_key(key, len) {
            PopupFlow::Done(checked) => PopupFlow::Done(MultiChoice {
                checked,
                mode: self.mode,
            }),
            PopupFlow::Cancelled => PopupFlow::Cancelled,
            PopupFlow::Continue => PopupFlow::Continue,
        }
    }
}

/// The indices below `len` that `checked` does not hold.
pub(super) fn inverted(checked: &HashSet<usize>, len: usize) -> HashSet<usize> {
    (0..len).filter(|index| !checked.contains(index)).collect()
}

/// The frame title: `title` alone without modes, else `title: mode (Tab:
/// other)`.
pub(super) fn moded_title(
    title: &str,
    modes: Option<&[String; 2]>,
    mode: usize,
) -> String {
    match modes {
        Some(names) => {
            let current = &names[mode.min(1)];
            let other = &names[1 - mode.min(1)];
            format!("{title}: {current} (Tab: {other})")
        }
        None => title.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::terminal::{clear_scripted_input, script_keys};
    use crate::theme::{
        ColorOverrides, GlyphVariant, Glyphs, Palette, ThemeRegistry,
    };

    fn skin() -> Skin {
        let base = ThemeRegistry::builtin().resolve("default");
        Skin::new(
            Palette::resolve(base, &ColorOverrides::default()),
            Glyphs::new(GlyphVariant::Unicode),
        )
    }

    fn statuses() -> Vec<SelectItem> {
        ["open", "in progress", "paused", "done"]
            .into_iter()
            .map(SelectItem::new)
            .collect()
    }

    fn hide_show() -> MultiSelectOptions {
        MultiSelectOptions {
            modes: Some(["hide".to_string(), "show".to_string()]),
            ..MultiSelectOptions::default()
        }
    }

    fn bare(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Runs the modal with `keys` scripted, `checked` preset, and returns the
    /// confirmed choice (`None` when cancelled).
    fn run(
        options: &MultiSelectOptions,
        checked: &[usize],
        keys: Vec<KeyEvent>,
    ) -> Option<MultiChoice> {
        let mut tui = Tui::for_test(60, 16).expect("a test terminal");
        script_keys(keys);
        let items = statuses();
        let answer = multi_select_with(
            &mut tui,
            &skin(),
            "Status",
            &items,
            checked,
            options,
            |_| {},
        )
        .expect("the scripted modal runs");
        clear_scripted_input();
        match answer {
            ModalSignal::Value(choice) => Some(choice),
            _ => None,
        }
    }

    #[test]
    fn inverting_checks_every_unchecked_index_and_nothing_else() {
        let checked: HashSet<usize> = [1, 3].into_iter().collect();
        let expected: HashSet<usize> = [0, 2].into_iter().collect();
        assert_eq!(inverted(&checked, 4), expected);
        assert!(inverted(&(0..4).collect(), 4).is_empty());
    }

    #[test]
    fn the_title_names_the_mode_and_the_tab_target() {
        let modes = ["hide".to_string(), "show".to_string()];
        assert_eq!(
            moded_title("Status", Some(&modes), 0),
            "Status: hide (Tab: show)",
        );
        assert_eq!(
            moded_title("Status", Some(&modes), 1),
            "Status: show (Tab: hide)",
        );
        assert_eq!(moded_title("Status", None, 0), "Status");
    }

    #[test]
    fn enter_returns_the_checks_and_the_mode() {
        let keys = vec![bare(KeyCode::Char(' ')), bare(KeyCode::Enter)];
        let choice = run(&hide_show(), &[3], keys).expect("confirmed");
        assert_eq!(choice.checked, [0, 3]);
        assert_eq!(choice.mode, 0);
    }

    /// Hiding "done" and showing everything else select the same projects, so
    /// `Tab` turns the one into the other.
    #[test]
    fn tab_switches_the_mode_and_inverts_the_checks() {
        let keys = vec![bare(KeyCode::Tab), bare(KeyCode::Enter)];
        let choice = run(&hide_show(), &[3], keys).expect("confirmed");
        assert_eq!(choice.checked, [0, 1, 2]);
        assert_eq!(choice.mode, 1);
    }

    #[test]
    fn without_modes_tab_does_nothing() {
        let keys = vec![bare(KeyCode::Tab), bare(KeyCode::Enter)];
        let options = MultiSelectOptions::default();
        let choice = run(&options, &[3], keys).expect("confirmed");
        assert_eq!(choice.checked, [3]);
        assert_eq!(choice.mode, 0);
    }

    #[test]
    fn enter_with_nothing_checked_takes_the_cursor_row() {
        let keys = vec![bare(KeyCode::Down), bare(KeyCode::Enter)];
        let options = MultiSelectOptions::default();
        let choice = run(&options, &[], keys).expect("confirmed");
        assert_eq!(choice.checked, [1]);
    }

    /// Showing every status leaves nothing checked; that is the user's choice,
    /// not an empty pick, so the cursor row is not taken.
    #[test]
    fn a_tab_that_unchecks_everything_confirms_nothing() {
        let keys = vec![bare(KeyCode::Tab), bare(KeyCode::Enter)];
        let choice = run(&hide_show(), &[0, 1, 2, 3], keys).expect("confirmed");
        assert!(choice.checked.is_empty());
        assert_eq!(choice.mode, 1);
    }

    #[test]
    fn the_cursor_opens_on_the_initial_row() {
        let options = MultiSelectOptions {
            initial_cursor: 2,
            ..MultiSelectOptions::default()
        };
        let choice = run(&options, &[], vec![bare(KeyCode::Enter)]);
        assert_eq!(choice.expect("confirmed").checked, [2]);
        // Past the last row it opens on the last row.
        let options = MultiSelectOptions {
            initial_cursor: 99,
            ..MultiSelectOptions::default()
        };
        let choice = run(&options, &[], vec![bare(KeyCode::Enter)]);
        assert_eq!(choice.expect("confirmed").checked, [3]);
    }

    #[test]
    fn esc_cancels() {
        assert!(run(&hide_show(), &[3], vec![bare(KeyCode::Esc)]).is_none());
    }
}
