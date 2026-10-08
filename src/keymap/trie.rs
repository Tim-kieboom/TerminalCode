use std::collections::HashMap;

use super::KeyChord;
use crate::event::action::Action;

/// Key sequences as a tree: each step is one chord. A node can have an action
/// (the sequence so far is a binding) and children (it is also a prefix).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Node {
    action: Option<Action>,
    children: HashMap<KeyChord, Node>,
}

impl Node {
    pub(super) fn action(&self) -> Option<&Action> {
        self.action.as_ref()
    }

    pub(super) fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    pub(super) fn get(&self, keys: &[KeyChord]) -> Option<&Node> {
        let Some((first, rest)) = keys.split_first() else {
            return Some(self);
        };
        self.children.get(first)?.get(rest)
    }

    /// Binds `keys`, replacing any earlier action for exactly that sequence.
    pub(super) fn bind(&mut self, keys: &[KeyChord], action: Action) {
        let Some((first, rest)) = keys.split_first() else {
            self.action = Some(action);
            return;
        };
        self.children.entry(*first).or_default().bind(rest, action);
    }

    /// Removes the action of exactly `keys`. Longer sequences that start with
    /// it stay bound, and branches left empty are pruned.
    pub(super) fn unbind(&mut self, keys: &[KeyChord]) {
        let Some((first, rest)) = keys.split_first() else {
            self.action = None;
            return;
        };
        let Some(child) = self.children.get_mut(first) else {
            return;
        };
        child.unbind(rest);
        if child.is_empty() {
            self.children.remove(first);
        }
    }

    fn is_empty(&self) -> bool {
        self.action.is_none() && self.children.is_empty()
    }
}
