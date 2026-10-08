use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use serde::Deserialize;
use thiserror::Error;

use crate::event::action::Action;
use crate::terminal::KeyboardSupport;

pub(crate) use key::KeyChord;
use key::KeyParseError;
pub(crate) use resolver::{Expiry, Outcome, Resolver};
use trie::Node;

mod key;
mod resolver;
#[cfg(test)]
mod tests;
mod trie;

/// Built-in bindings for every terminal.
const DEFAULT_KEYMAP_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/defaults/default_keymap.toml"
));

/// Extra bindings for terminals without the kitty keyboard protocol, where
/// some chords arrive as other keys.
const LEGACY_KEYMAP_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/defaults/default_keymap_legacy.toml"
));

#[derive(Debug, Error)]
pub enum KeymapError {
    #[error("keymap is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("binding `{keys}`: {source}")]
    InvalidKey {
        keys: Box<str>,
        source: KeyParseError,
    },
    #[error("`{0}` is bound more than once")]
    DuplicateBinding(Box<str>),
}

/// Where a binding applies. When several contexts are active, earlier ones
/// win: a binding in a more specific context hides one in a broader context.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Context {
    #[default]
    Global,
    Editor,
    Explorer,
}

#[derive(Debug, Deserialize)]
struct KeymapFile {
    #[serde(default)]
    binding: Vec<BindingSpec>,
}

/// One `[[binding]]` table. Without an `action`, the binding removes an
/// existing one (a user keymap uses this to free a default key).
#[derive(Debug, Deserialize)]
struct BindingSpec {
    keys: Box<str>,
    action: Option<Action>,
    #[serde(default)]
    context: Context,
}

#[derive(Debug)]
struct Binding {
    context: Context,
    keys: Vec<KeyChord>,
    action: Option<Action>,
}

/// What the keymap knows about a sequence of chords.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lookup<'a> {
    NotFound,
    Found {
        /// The action bound to exactly this sequence, if any.
        action: Option<&'a Action>,
        /// Whether longer sequences start with this one.
        has_more: bool,
    },
}

/// Key sequences bound to actions, per context.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Keymap {
    contexts: HashMap<Context, Node>,
}

impl Keymap {
    /// Built-in bindings, plus the fallbacks the terminal needs.
    pub(crate) fn defaults(keyboard: KeyboardSupport) -> Self {
        let mut keymap = Self::default();
        keymap
            .apply_toml(DEFAULT_KEYMAP_TOML)
            .expect("defaults/default_keymap.toml must be a valid keymap");
        if keyboard == KeyboardSupport::Legacy {
            keymap
                .apply_toml(LEGACY_KEYMAP_TOML)
                .expect("defaults/default_keymap_legacy.toml must be a valid keymap");
        }
        keymap
    }

    /// Parses a keymap described as TOML:
    ///
    /// ```toml
    /// [[binding]]
    /// keys = "ctrl+k ctrl+s"   # a sequence: chords separated by spaces
    /// action = "save"
    /// context = "editor"       # optional, defaults to "global"
    /// ```
    #[cfg(test)]
    pub(crate) fn from_toml(source: &str) -> Result<Self, KeymapError> {
        let mut keymap = Self::default();
        keymap.apply_toml(source)?;
        Ok(keymap)
    }

    /// Layers the bindings in `source` over this keymap: they replace bindings
    /// of the same sequence and context, and a binding without an action
    /// removes one. Nothing changes if `source` is invalid.
    pub(crate) fn apply_toml(&mut self, source: &str) -> Result<(), KeymapError> {
        for binding in parse_bindings(source)? {
            let root = self.contexts.entry(binding.context).or_default();
            match binding.action {
                Some(action) => root.bind(&binding.keys, action),
                None => root.unbind(&binding.keys),
            }
        }
        Ok(())
    }

    /// The shortest key sequence bound to `action` in any of `contexts`, as
    /// text such as `ctrl+k right`. Fewer chords win, then the shorter text.
    pub(crate) fn keys_for(&self, action: &Action, contexts: &[Context]) -> Option<String> {
        contexts
            .iter()
            .filter_map(|context| self.contexts.get(context))
            .flat_map(|root| root.sequences_for(action))
            .map(|keys| {
                let text: Vec<_> = keys.iter().map(ToString::to_string).collect();
                let text = text.join(" ");
                (keys.len(), text.len(), text)
            })
            .min()
            .map(|(_, _, text)| text)
    }

    /// Looks `keys` up in each of `contexts` in order; the first context that
    /// knows the sequence answers.
    pub(crate) fn find(&self, contexts: &[Context], keys: &[KeyChord]) -> Lookup<'_> {
        for context in contexts {
            let Some(node) = self.contexts.get(context).and_then(|root| root.get(keys)) else {
                continue;
            };
            return Lookup::Found {
                action: node.action(),
                has_more: node.has_children(),
            };
        }
        Lookup::NotFound
    }
}

fn parse_bindings(source: &str) -> Result<Vec<Binding>, KeymapError> {
    let file: KeymapFile = toml::from_str(source)?;
    let mut seen = HashSet::with_capacity(file.binding.len());
    let mut bindings = Vec::with_capacity(file.binding.len());

    for spec in file.binding {
        let keys = parse_sequence(&spec.keys)?;
        if !seen.insert((spec.context, keys.clone())) {
            return Err(KeymapError::DuplicateBinding(spec.keys));
        }
        bindings.push(Binding {
            context: spec.context,
            keys,
            action: spec.action,
        });
    }
    Ok(bindings)
}

/// `"ctrl+k ctrl+s"` -> two chords.
fn parse_sequence(keys: &str) -> Result<Vec<KeyChord>, KeymapError> {
    let invalid = |source| KeymapError::InvalidKey {
        keys: keys.into(),
        source,
    };
    let chords = keys
        .split_whitespace()
        .map(KeyChord::from_str)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if chords.is_empty() {
        return Err(invalid(KeyParseError::Empty));
    }
    Ok(chords)
}
