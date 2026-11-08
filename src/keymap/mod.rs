//! User-remappable key bindings: parse chords from config, resolve them
//! against an app's actions, and render them back for the hints footer.
//!
//! An app supplies its own action type (what the keys *do*) by implementing
//! [`Action`]; this module owns everything else - the chord grammar, the
//! defaults-vs-overrides merge, conflict detection, and the display form. That
//! split is the point: the action table is the app's identity, while
//! [`KeyChord::matches`] is the function in which "the modifier comparison gets
//! forgotten", so it should exist exactly once.
//!
//! # Modifier semantics
//!
//! `ctrl` and `alt` are matched **exactly**, never with `contains`. That is
//! what keeps `AltGr` - reported as `Control + Alt`, and a real character on
//! e.g. a German layout - from triggering a `ctrl+…` binding.
//!
//! `shift` depends on the key: for a character it is carried by the character's
//! *case* (`G` is the chord, not `shift+g`) and not compared, because terminals
//! report the shifted character itself. For every other key it is significant,
//! so `left` and `shift+left` are distinct chords and an app can bind both.
//!
//! # Examples
//!
//! ```
//! use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
//! use ratada::keymap::KeyChord;
//!
//! let chord = KeyChord::parse("ctrl+s").expect("a valid chord");
//! assert!(chord.matches(&KeyEvent::new(
//!     KeyCode::Char('s'),
//!     KeyModifiers::CONTROL,
//! )));
//!
//! // `AltGr` is Control+Alt and types a character; it is not a Ctrl chord.
//! assert!(!chord.matches(&KeyEvent::new(
//!     KeyCode::Char('s'),
//!     KeyModifiers::CONTROL | KeyModifiers::ALT,
//! )));
//!
//! // The display form parses back into the same chord.
//! assert_eq!(chord.display(), "ctrl+s");
//! ```

mod chord;
mod config;

pub use chord::KeyChord;
use config::override_keys;
pub use config::{KeyBinding, warn_unknown};

use std::collections::BTreeMap;

use crossterm::event::KeyEvent;

/// An app's action type: what its keys do, and how they are named in config.
///
/// The app owns the variants and their defaults; this module owns the chords.
/// Implement it on a small `Copy` enum. `'static` because the descriptions and
/// default keys are `&'static str`, which a fieldless enum satisfies for free.
pub trait Action: Copy + Eq + 'static {
    /// Every action, in the order they claim keys: on a conflict the earlier
    /// one keeps the chord.
    ///
    /// An iterator rather than a slice: an app's actions are typically one
    /// column of a catalog table (`ACTIONS.iter().map(|spec| spec.action)`),
    /// and demanding a `&'static [Self]` would force a second, redundant list
    /// that could drift from the table.
    fn all() -> impl Iterator<Item = Self> + Clone;

    /// The action's key in the config's `[keys]` table.
    fn config_name(&self) -> &'static str;

    /// A one-line description for the help screen.
    fn description(&self) -> &'static str;

    /// The chords bound to it when config says nothing.
    fn default_keys(&self) -> &'static [&'static str];

    /// Whether two actions compete for one chord.
    ///
    /// The default is "always", i.e. one flat map in which a chord belongs to a
    /// single action. An app whose actions are scoped (per view, per focus)
    /// overrides this so two actions in disjoint scopes may share a chord.
    fn overlaps(&self, other: &Self) -> bool {
        let _ = other;
        true
    }

    /// The action a config name refers to, if any.
    fn from_config_name(name: &str) -> Option<Self> {
        Self::all().find(|action| action.config_name() == name)
    }
}

/// A configured key dropped because an earlier action already claimed it.
///
/// Surfaced rather than swallowed, so a silently shadowed binding is visible.
#[derive(Debug, Clone)]
pub struct Conflict<A> {
    /// The contested key, in [`KeyChord::display`] form.
    pub key: String,
    /// The action whose binding was dropped.
    pub action: A,
    /// The action that already owns the key.
    pub claimed_by: A,
}

/// The resolved bindings for an action type.
#[derive(Debug, Clone)]
pub struct Keymap<A> {
    entries: Vec<(KeyChord, A)>,
    conflicts: Vec<Conflict<A>>,
}

impl<A: Action> Default for Keymap<A> {
    /// The compiled-in defaults, i.e. the map an empty config yields.
    fn default() -> Self {
        Self::from_overrides(&BTreeMap::new())
    }
}

impl<A: Action> Keymap<A> {
    /// Builds the map for every action, applying `overrides`.
    ///
    /// An action named in `overrides` uses those keys instead of its defaults.
    /// An unparseable key is logged and skipped; a chord an earlier action
    /// already owns (per [`Action::all`] order, narrowed by
    /// [`Action::overlaps`]) keeps that binding, and the later one is recorded
    /// in [`Keymap::conflicts`].
    #[must_use]
    pub fn from_overrides(overrides: &BTreeMap<String, Vec<String>>) -> Self {
        Self::for_actions(A::all(), overrides)
    }

    /// Like [`Keymap::from_overrides`], but only for `actions`.
    ///
    /// For an app that resolves keys per view or per focus and wants one map
    /// per subset rather than one map plus a scope check. Takes anything
    /// iterable, so a catalog iterator, a `Vec` of a view's actions and a bare
    /// array all fit.
    #[must_use]
    pub fn for_actions(
        actions: impl IntoIterator<Item = A>,
        overrides: &BTreeMap<String, Vec<String>>,
    ) -> Self {
        let mut map = Keymap {
            entries: Vec::new(),
            conflicts: Vec::new(),
        };
        for action in actions {
            for key in override_keys(overrides, action) {
                map.bind(action, &key);
            }
        }
        map
    }

    /// The action bound to `key`, if any.
    #[must_use]
    pub fn action_for(&self, key: &KeyEvent) -> Option<A> {
        self.action_for_where(key, |_| true)
    }

    /// The first action bound to `key` that `allow` accepts.
    ///
    /// For an app whose actions are only live in some contexts (a view, a
    /// focused pane): one map holds every binding, and the caller decides which
    /// are reachable right now. Pair it with an [`Action::overlaps`] that draws
    /// the same line, or two actions sharing a chord across contexts would be
    /// reported as a conflict.
    #[must_use]
    pub fn action_for_where(
        &self,
        key: &KeyEvent,
        allow: impl Fn(&A) -> bool,
    ) -> Option<A> {
        self.entries
            .iter()
            .find(|(chord, action)| allow(action) && chord.matches(key))
            .map(|(_, action)| *action)
    }

    /// The keys bound to `action` in config syntax ([`KeyChord::display`]:
    /// `ctrl+s`), in binding order.
    #[must_use]
    pub fn keys_for(&self, action: A) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(_, bound)| *bound == action)
            .map(|(chord, _)| chord.display())
            .collect()
    }

    /// The bindings dropped because an earlier action owned the key.
    #[must_use]
    pub fn conflicts(&self) -> &[Conflict<A>] {
        &self.conflicts
    }

    /// The keys bound to `action` as a reader sees them ([`KeyChord::label`]:
    /// `Ctrl+S`, `Enter`, `G`), in binding order: what a footer, a help
    /// screen and a README show.
    #[must_use]
    pub fn labels_for(&self, action: A) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(_, bound)| *bound == action)
            .map(|(chord, _)| chord.label())
            .collect()
    }

    /// `(keys, description)` hint pairs for `actions`, in the given order,
    /// skipping any action with no bound key.
    ///
    /// The one source for a footer and a help screen: several keys for one
    /// action join with `/` (`"\u{2191}/k"`), each written as a reader sees it
    /// ([`KeyChord::label`]: `Ctrl+S`, `Enter`). Feed the result to
    /// `shortcut_hints`.
    #[must_use]
    pub fn hints(&self, actions: &[A]) -> Vec<(String, String)> {
        actions
            .iter()
            .filter_map(|&action| {
                let keys = self.labels_for(action).join("/");
                if keys.is_empty() {
                    return None;
                }
                Some((keys, action.description().to_string()))
            })
            .collect()
    }

    /// Binds `key` to `action`, or records why it was dropped.
    fn bind(&mut self, action: A, key: &str) {
        let Some(chord) = KeyChord::parse(key) else {
            log::warn!("invalid key '{key}' for '{}'", action.config_name());
            return;
        };
        let owner = self.entries.iter().find(|(existing, owner)| {
            *existing == chord && owner.overlaps(&action)
        });
        if let Some((_, owner)) = owner {
            log::warn!(
                "key '{}' for '{}' is already bound to '{}', ignoring",
                chord.display(),
                action.config_name(),
                owner.config_name(),
            );
            self.conflicts.push(Conflict {
                key: chord.display(),
                action,
                claimed_by: *owner,
            });
            return;
        }
        self.entries.push((chord, action));
    }
}

#[cfg(test)]
mod tests;
