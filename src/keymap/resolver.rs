use super::{Context, KeyChord, Keymap, Lookup};
use crate::event::action::Action;

/// What a chord turned into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// A complete binding: run it.
    Action(Action),
    /// The start of a longer binding: wait for more keys (or a timeout).
    Pending,
    /// Not part of any binding.
    Unbound(KeyChord),
}

/// Result of feeding one chord to the [`Resolver`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Resolution {
    /// Chords that were waiting for a binding that this chord broke. The
    /// caller decides what to do with them (typically type the printable ones).
    pub(crate) discarded: Vec<KeyChord>,
    pub(crate) outcome: Outcome,
}

/// What a timed-out pending sequence turned into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expiry {
    Nothing,
    /// The pending chords were themselves a binding.
    Fire(Action),
    /// The pending chords matched no binding on their own.
    Discard(Vec<KeyChord>),
}

/// Turns a stream of chords into actions, keeping track of multi-chord
/// sequences in progress.
#[derive(Debug, Default)]
pub(crate) struct Resolver {
    pending: Vec<KeyChord>,
}

impl Resolver {
    #[cfg(test)]
    pub(crate) fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(crate) fn feed(
        &mut self,
        keymap: &Keymap,
        contexts: &[Context],
        chord: KeyChord,
    ) -> Resolution {
        self.pending.push(chord);
        if let Some(outcome) = self.advance(keymap, contexts) {
            return Resolution {
                discarded: Vec::new(),
                outcome,
            };
        }

        // The sequence so far matches nothing. Drop the old part and give the
        // new chord a fresh start, so a stray prefix never eats a command.
        let mut discarded = std::mem::take(&mut self.pending);
        discarded.pop();
        self.pending.push(chord);
        let outcome = self.advance(keymap, contexts).unwrap_or_else(|| {
            self.pending.clear();
            Outcome::Unbound(chord)
        });
        Resolution { discarded, outcome }
    }

    /// Resolves the pending chords after waiting too long for the next one.
    pub(crate) fn expire(&mut self, keymap: &Keymap, contexts: &[Context]) -> Expiry {
        let pending = std::mem::take(&mut self.pending);
        if pending.is_empty() {
            return Expiry::Nothing;
        }
        match keymap.find(contexts, &pending) {
            Lookup::Found {
                action: Some(action),
                ..
            } => Expiry::Fire(action.clone()),
            _ => Expiry::Discard(pending),
        }
    }

    /// Outcome for the current pending chords, or `None` if they match nothing.
    fn advance(&mut self, keymap: &Keymap, contexts: &[Context]) -> Option<Outcome> {
        let Lookup::Found { action, has_more } = keymap.find(contexts, &self.pending) else {
            return None;
        };
        if has_more {
            return Some(Outcome::Pending);
        }
        let action = action?.clone();
        self.pending.clear();
        Some(Outcome::Action(action))
    }
}
