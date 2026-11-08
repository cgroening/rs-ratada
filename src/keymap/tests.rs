use crossterm::event::{KeyCode, KeyModifiers};

use super::*;

/// A miniature app action set, standing in for a real one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    Up,
    Down,
    Quit,
    Extend,
}

impl Action for Act {
    fn all() -> impl Iterator<Item = Self> + Clone {
        [Act::Up, Act::Down, Act::Quit, Act::Extend].into_iter()
    }

    fn config_name(&self) -> &'static str {
        match self {
            Act::Up => "up",
            Act::Down => "down",
            Act::Quit => "quit",
            Act::Extend => "extend",
        }
    }

    fn description(&self) -> &'static str {
        "test action"
    }

    fn default_keys(&self) -> &'static [&'static str] {
        match self {
            Act::Up => &["up", "k"],
            Act::Down => &["down", "j"],
            Act::Quit => &["ctrl+q", "q"],
            Act::Extend => &["shift+up"],
        }
    }
}

fn event(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

fn chord(text: &str) -> KeyChord {
    KeyChord::parse(text).expect("a valid literal chord")
}

#[test]
fn parses_modifiers_keys_and_function_keys() {
    assert_eq!(chord("a").display(), "a");
    assert_eq!(chord("G").display(), "G");
    assert_eq!(chord("ctrl+q").display(), "ctrl+q");
    assert_eq!(chord("control+q").display(), "ctrl+q");
    assert_eq!(chord("alt+up").display(), "alt+up");
    assert_eq!(chord("option+up").display(), "alt+up");
    assert_eq!(chord("shift+left").display(), "shift+left");
    assert_eq!(chord("f2").display(), "f2");
    assert_eq!(chord("pgup").display(), "pgup");
    assert_eq!(chord("pageup").display(), "pgup");
    assert_eq!(chord("space").display(), "space");
    assert_eq!(chord("ctrl+alt+del").display(), "ctrl+alt+del");
}

#[test]
fn a_label_reads_like_the_key_cap() {
    assert_eq!(chord("a").label(), "a");
    assert_eq!(chord("G").label(), "G");
    assert_eq!(chord("ctrl+q").label(), "Ctrl+Q");
    assert_eq!(chord("ctrl+shift+q").label(), "Ctrl+Shift+Q");
    assert_eq!(chord("Ctrl+Q"), chord("ctrl+q"), "a label reads as typed");
    assert_eq!(chord("alt+up").label(), "Alt+\u{2191}");
    assert_eq!(chord("shift+left").label(), "Shift+\u{2190}");
    assert_eq!(chord("backtab").label(), "Shift+Tab");
    assert_eq!(chord("f2").label(), "F2");
    assert_eq!(chord("pgup").label(), "PgUp");
    assert_eq!(chord("space").label(), "Space");
    assert_eq!(chord("enter").label(), "Enter");
    assert_eq!(chord("ctrl+alt+del").label(), "Ctrl+Alt+Del");
}

#[test]
fn rejects_an_unknown_modifier_or_a_word_key() {
    assert!(KeyChord::parse("hyper+a").is_none());
    assert!(KeyChord::parse("cmd+a").is_none());
    assert!(KeyChord::parse("arrows").is_none());
    assert!(KeyChord::parse("f13").is_none());
    assert!(KeyChord::parse("").is_none());
}

/// The contract the hints footer and the config file share: whatever is
/// rendered can be typed back into config and means the same chord.
#[test]
fn chords_round_trip_through_parse_and_display() {
    for text in [
        "a",
        "G",
        "ctrl+q",
        "alt+up",
        "shift+left",
        "f2",
        "pgup",
        "pgdn",
        "space",
        "enter",
        "esc",
        "tab",
        "del",
        "home",
        "end",
        "backspace",
        "ctrl+alt+left",
    ] {
        let parsed = chord(text);
        assert_eq!(parsed.display(), text, "{text} must round-trip");
        assert_eq!(chord(&parsed.display()), parsed);
    }
}

/// The reason `matches` compares ctrl/alt exactly: `AltGr` is reported as
/// Control+Alt and produces a real character, so it must never stand in for
/// a Ctrl chord.
#[test]
fn altgr_does_not_trigger_a_ctrl_chord() {
    let ctrl_q = chord("ctrl+q");
    assert!(ctrl_q.matches(&event(KeyCode::Char('q'), KeyModifiers::CONTROL)));
    assert!(!ctrl_q.matches(&event(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    )));
}

#[test]
fn a_plain_chord_does_not_match_a_modified_key() {
    let plain = chord("q");
    assert!(plain.matches(&event(KeyCode::Char('q'), KeyModifiers::NONE)));
    assert!(!plain.matches(&event(KeyCode::Char('q'), KeyModifiers::CONTROL)));
    assert!(!plain.matches(&event(KeyCode::Char('q'), KeyModifiers::ALT)));
}

/// Shift is significant for a non-character key, so an app can bind `left`
/// and `shift+left` to different actions.
#[test]
fn shift_distinguishes_non_character_chords() {
    let plain = chord("left");
    let shifted = chord("shift+left");
    assert!(plain.matches(&event(KeyCode::Left, KeyModifiers::NONE)));
    assert!(!plain.matches(&event(KeyCode::Left, KeyModifiers::SHIFT)));
    assert!(shifted.matches(&event(KeyCode::Left, KeyModifiers::SHIFT)));
    assert!(!shifted.matches(&event(KeyCode::Left, KeyModifiers::NONE)));
}

/// For a character, shift lives in the case: the terminal reports `G`, not
/// `shift+g`, so the modifier must not be compared or `G` would never fire.
#[test]
fn shift_is_ignored_for_a_character_chord() {
    let upper = chord("G");
    assert!(upper.matches(&event(KeyCode::Char('G'), KeyModifiers::SHIFT)));
    assert!(upper.matches(&event(KeyCode::Char('G'), KeyModifiers::NONE)));
    // A different case is a different chord.
    assert!(!upper.matches(&event(KeyCode::Char('g'), KeyModifiers::NONE)));
}

#[test]
fn defaults_resolve_every_action() {
    let map: Keymap<Act> = Keymap::from_overrides(&BTreeMap::new());
    assert!(map.conflicts().is_empty());
    assert_eq!(
        map.action_for(&event(KeyCode::Char('k'), KeyModifiers::NONE)),
        Some(Act::Up)
    );
    assert_eq!(
        map.action_for(&event(KeyCode::Char('q'), KeyModifiers::CONTROL)),
        Some(Act::Quit)
    );
    assert_eq!(map.keys_for(Act::Up), vec!["up", "k"]);
    assert_eq!(
        map.action_for(&event(KeyCode::Char('z'), KeyModifiers::NONE)),
        None
    );
}

/// The shift semantics reaching all the way through the map: `up` and
/// `shift+up` resolve to different actions.
#[test]
fn a_shifted_arrow_resolves_to_its_own_action() {
    let map: Keymap<Act> = Keymap::from_overrides(&BTreeMap::new());
    assert_eq!(
        map.action_for(&event(KeyCode::Up, KeyModifiers::NONE)),
        Some(Act::Up)
    );
    assert_eq!(
        map.action_for(&event(KeyCode::Up, KeyModifiers::SHIFT)),
        Some(Act::Extend)
    );
}

#[test]
fn an_override_replaces_the_defaults() {
    let mut overrides = BTreeMap::new();
    overrides.insert("up".to_string(), vec!["ctrl+p".to_string()]);
    let map: Keymap<Act> = Keymap::from_overrides(&overrides);
    assert_eq!(
        map.action_for(&event(KeyCode::Char('p'), KeyModifiers::CONTROL)),
        Some(Act::Up)
    );
    // The default is gone, not merged.
    assert_eq!(
        map.action_for(&event(KeyCode::Char('k'), KeyModifiers::NONE)),
        None
    );
}

#[test]
fn a_contested_key_keeps_the_earlier_action_and_is_reported() {
    let mut overrides = BTreeMap::new();
    overrides.insert("down".to_string(), vec!["k".to_string()]);
    let map: Keymap<Act> = Keymap::from_overrides(&overrides);
    // `Up` claims `k` first (it comes first in `all`).
    assert_eq!(
        map.action_for(&event(KeyCode::Char('k'), KeyModifiers::NONE)),
        Some(Act::Up)
    );
    let conflicts = map.conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].key, "k");
    assert_eq!(conflicts[0].action, Act::Down);
    assert_eq!(conflicts[0].claimed_by, Act::Up);
}

#[test]
fn an_unparseable_override_is_skipped_not_fatal() {
    let mut overrides = BTreeMap::new();
    overrides.insert(
        "quit".to_string(),
        vec!["nonsense+x".to_string(), "f4".to_string()],
    );
    let map: Keymap<Act> = Keymap::from_overrides(&overrides);
    // The bad key is dropped; the good one still binds.
    assert_eq!(
        map.action_for(&event(KeyCode::F(4), KeyModifiers::NONE)),
        Some(Act::Quit)
    );
}

/// A scoped app keeps every binding in one map and narrows the lookup, so
/// the same chord can mean two things in two contexts.
#[test]
fn action_for_where_narrows_the_lookup() {
    let mut overrides = BTreeMap::new();
    // Both want `x`; `Up` claims it, `Down`'s is a conflict under the
    // default `overlaps`, so only `Up` is reachable.
    overrides.insert("up".to_string(), vec!["x".to_string()]);
    let map: Keymap<Act> = Keymap::from_overrides(&overrides);
    let key = event(KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(map.action_for(&key), Some(Act::Up));
    // Excluding the owner leaves nothing rather than falling through.
    assert_eq!(map.action_for_where(&key, |a| *a != Act::Up), None);
}

#[test]
fn for_actions_restricts_the_map_to_a_subset() {
    let map: Keymap<Act> = Keymap::for_actions([Act::Quit], &BTreeMap::new());
    assert_eq!(
        map.action_for(&event(KeyCode::Char('q'), KeyModifiers::NONE)),
        Some(Act::Quit)
    );
    assert_eq!(
        map.action_for(&event(KeyCode::Char('k'), KeyModifiers::NONE)),
        None
    );
}

/// A scoped action set, standing in for an app whose views each own their
/// keys: `Submit` and `Rename` both want `enter`, but never at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scoped {
    Submit,
    Rename,
    Quit,
}

impl Scoped {
    /// Which screen the action belongs to; `None` means everywhere.
    fn screen(self) -> Option<u8> {
        match self {
            Scoped::Submit => Some(1),
            Scoped::Rename => Some(2),
            Scoped::Quit => None,
        }
    }
}

impl Action for Scoped {
    fn all() -> impl Iterator<Item = Self> + Clone {
        [Scoped::Submit, Scoped::Rename, Scoped::Quit].into_iter()
    }

    fn config_name(&self) -> &'static str {
        match self {
            Scoped::Submit => "submit",
            Scoped::Rename => "rename",
            Scoped::Quit => "quit",
        }
    }

    fn description(&self) -> &'static str {
        "scoped action"
    }

    fn default_keys(&self) -> &'static [&'static str] {
        match self {
            Scoped::Submit | Scoped::Rename => &["enter"],
            Scoped::Quit => &["q"],
        }
    }

    fn overlaps(&self, other: &Self) -> bool {
        match (self.screen(), other.screen()) {
            // A global action competes with everything.
            (None, _) | (_, None) => true,
            (Some(a), Some(b)) => a == b,
        }
    }
}

/// The point of `overlaps`: two actions on different screens may share a
/// chord, so neither is dropped and neither is a conflict. With the default
/// `overlaps` (always true) the second binding would lose `enter`.
#[test]
fn a_chord_shared_across_disjoint_scopes_is_not_a_conflict() {
    let map: Keymap<Scoped> = Keymap::from_overrides(&BTreeMap::new());
    assert!(
        map.conflicts().is_empty(),
        "disjoint scopes must not collide: {:?}",
        map.conflicts()
    );
    assert_eq!(map.keys_for(Scoped::Submit), vec!["enter"]);
    assert_eq!(map.keys_for(Scoped::Rename), vec!["enter"]);

    // The caller decides which of the two is live right now.
    let enter = event(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        map.action_for_where(&enter, |a| a.screen() != Some(2)),
        Some(Scoped::Submit)
    );
    assert_eq!(
        map.action_for_where(&enter, |a| a.screen() != Some(1)),
        Some(Scoped::Rename)
    );
}

/// The other half: a global action overlaps every scope, so sharing a chord
/// with it *is* a conflict. `overlaps` narrows the check, it does not
/// disable it - and the earlier action still keeps the key.
#[test]
fn a_chord_shared_with_a_global_action_is_still_a_conflict() {
    let mut overrides = BTreeMap::new();
    // `Rename` claims `q` first (it precedes `Quit` in `all`), so the
    // global `Quit` loses its own default to it.
    overrides.insert("rename".to_string(), vec!["q".to_string()]);
    let map: Keymap<Scoped> = Keymap::from_overrides(&overrides);
    let conflicts = map.conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].action, Scoped::Quit);
    assert_eq!(conflicts[0].claimed_by, Scoped::Rename);
}

/// `to_key` and `from_key` are inverses, and what `to_key` builds is always
/// a press the chord accepts - that is what makes it safe to replay.
#[test]
fn to_key_round_trips_and_always_matches() {
    for text in ["a", "G", "ctrl+q", "alt+up", "shift+left", "f2", "space"] {
        let parsed = chord(text);
        let key = parsed.to_key();
        assert!(parsed.matches(&key), "{text} must match its own key");
        assert_eq!(KeyChord::from_key(key), parsed, "{text} round-trips");
    }
}

#[test]
fn hints_join_several_keys_and_skip_unbound_actions() {
    let mut overrides = BTreeMap::new();
    // Binding an action to nothing leaves it out of the hints.
    overrides.insert("quit".to_string(), Vec::new());
    let map: Keymap<Act> = Keymap::from_overrides(&overrides);
    assert_eq!(
        map.hints(&[Act::Up, Act::Quit]),
        vec![("\u{2191}/k".to_string(), "test action".to_string())]
    );
}

#[test]
fn from_config_name_resolves_every_action() {
    for action in Act::all() {
        assert_eq!(Act::from_config_name(action.config_name()), Some(action));
    }
    assert_eq!(Act::from_config_name("nope"), None);
}

#[test]
fn a_key_binding_reads_both_the_scalar_and_the_list_form() {
    assert_eq!(
        KeyBinding::One("ctrl+s".to_string()).into_keys(),
        vec!["ctrl+s".to_string()]
    );
    assert_eq!(
        KeyBinding::Many(vec!["a".to_string(), "b".to_string()]).into_keys(),
        vec!["a".to_string(), "b".to_string()]
    );
}
