//! Reading user key overrides out of a config table.
//!
//! The deserializable [`KeyBinding`] shape plus the lookup that turns a table
//! of action names onto chords into the bindings a [`super::Keymap`] uses,
//! warning about names no action answers to.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Action;

/// A key binding as written in config: one key, or a list of them.
///
/// `#[serde(untagged)]`, so `key = "ctrl+s"` and `key = ["ctrl+s", "f2"]` both
/// deserialize.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeyBinding {
    /// A single chord.
    One(String),
    /// Several chords, all triggering the action.
    Many(Vec<String>),
}

impl KeyBinding {
    /// The chords as a list, whichever form was written.
    #[must_use]
    pub fn into_keys(self) -> Vec<String> {
        match self {
            KeyBinding::One(key) => vec![key],
            KeyBinding::Many(keys) => keys,
        }
    }
}

/// Logs every `[keys]` entry that names no action, so a typo in config is not
/// silently ignored.
pub fn warn_unknown<A: Action>(overrides: &BTreeMap<String, Vec<String>>) {
    for name in overrides.keys() {
        if A::from_config_name(name).is_none() {
            log::warn!("unknown key action '{name}' in config, ignoring");
        }
    }
}

/// The keys to use for `action`: the override if config names it, else its
/// defaults.
pub(super) fn override_keys<A: Action>(
    overrides: &BTreeMap<String, Vec<String>>,
    action: A,
) -> Vec<String> {
    overrides
        .get(action.config_name())
        .cloned()
        .unwrap_or_else(|| {
            action
                .default_keys()
                .iter()
                .map(|key| (*key).to_string())
                .collect()
        })
}
