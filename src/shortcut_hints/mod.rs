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
//! The `F1` toggle governs **only the host's main-app footer**, drawn through
//! the grouped API: while [`visible`] is false, [`group_lines`] yields nothing,
//! [`render`] draws nothing (not even its top margin) and [`height`] reports
//! zero rows. The flat popup API ([`lines`], [`footer_height`]) ignores the
//! toggle and always renders, because a modal's key prompt (a confirm's `y/n`,
//! a picker's `enter/esc`) is essential and must show regardless. A popup that
//! *does* want its footer to follow the toggle guards its hints with
//! [`visible`] itself. `driver::run` and `overlay::popup` consume the toggle
//! chord ([`default_toggle_key`], `F1`), so every screen and every modal
//! inherits it without the host wiring anything up. Hints start out shown.
//!
//! A host that needs the key for itself rebinds or unbinds the chord with
//! [`set_toggle_key`]; unbound, the key reaches its `handle_key` as usual. An
//! app that drives its own event loop instead of `driver::run` calls
//! [`consume_toggle`] at the top of its key dispatch to inherit the chord.

mod render;
mod state;

use ratatui::style::{Modifier, Style};
pub use render::{footer_height, group_lines, height, lines, render};
pub use state::{
    consume_toggle, default_toggle_key, global_bindings, set_toggle_key,
    set_visible, toggle, toggle_key, visible,
};

use super::{keymap, style};
use crate::theme::Color;

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

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::state::chord_label;

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
    fn hiding_the_hints_affects_only_the_grouped_footer() {
        // The flat popup API always renders; only the grouped main-footer API
        // follows the global toggle.
        let flat = lines(ITEMS, Color::Default, 80);
        while_hidden(|| {
            assert_eq!(lines(ITEMS, Color::Default, 80), flat);
            let groups = [HintGroup {
                label: "A",
                hints: ITEMS,
            }];
            assert!(group_lines(&groups, &HintStyle::default(), 80).is_empty());
        });
    }

    #[test]
    fn hidden_hints_reclaim_only_the_grouped_footer_rows() {
        while_hidden(|| {
            let groups = [HintGroup {
                label: "A",
                hints: ITEMS,
            }];
            // The grouped main-app footer collapses with the toggle.
            assert_eq!(height(&groups, 80, 1), 0);
            // A popup footer keeps its reserved rows.
            assert_eq!(footer_height(1), 1);
            assert_eq!(footer_height(2), 2);
        });
    }

    #[test]
    fn footer_height_passes_the_rows_through_regardless_of_visibility() {
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

    /// A hint must keep an upper-case key upper-case: an app binding both `g`
    /// and `G` would otherwise show the same footer token for two actions.
    #[test]
    fn chord_label_keeps_a_characters_case() {
        let label =
            |code, modifiers| chord_label(KeyEvent::new(code, modifiers));
        assert_eq!(label(KeyCode::Char('G'), KeyModifiers::SHIFT), "G");
        assert_eq!(label(KeyCode::Char('g'), KeyModifiers::NONE), "g");
    }

    /// The label and the config grammar are one thing now: every rendered hint
    /// must parse back into the chord it came from, or a user could not type
    /// what the footer shows into their `[keys]` table.
    #[test]
    fn a_rendered_chord_parses_back_into_itself() {
        for (code, modifiers) in [
            (KeyCode::F(1), KeyModifiers::NONE),
            (KeyCode::Char('h'), KeyModifiers::CONTROL),
            (KeyCode::Char('G'), KeyModifiers::SHIFT),
            (KeyCode::Enter, KeyModifiers::SHIFT),
            (KeyCode::Left, KeyModifiers::ALT),
            (KeyCode::Char(' '), KeyModifiers::NONE),
            (KeyCode::Delete, KeyModifiers::NONE),
            (KeyCode::PageUp, KeyModifiers::NONE),
        ] {
            let key = KeyEvent::new(code, modifiers);
            let label = chord_label(key);
            let parsed = keymap::KeyChord::parse(&label)
                .unwrap_or_else(|| panic!("'{label}' must parse back"));
            assert!(parsed.matches(&key), "'{label}' must match {key:?}");
        }
    }
}
