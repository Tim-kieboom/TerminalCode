//! Where you have been: a list of places to go back to and forward to, like the
//! back and forward of a browser. A place is recorded when a jump leaves it; the
//! list moves its places through the edits made to their documents, so that
//! they stay on the same text; places whose file is gone are skipped.
//!
//! See "Go to definition and the jump list" in `docs/design.md`.

use std::path::PathBuf;

use trcode_ids::DocumentId;

/// The most places the list keeps; the oldest go first.
const LIMIT: usize = 100;

/// A place in a document: the document, the file it came from (so that it can
/// be opened again after its tab was closed), and a byte of its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub document: DocumentId,
    pub path: Option<PathBuf>,
    pub byte: usize,
}

/// An edit of a document: the bytes `start..old_end` became `start..new_end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub start: usize,
    pub old_end: usize,
    pub new_end: usize,
}

impl Place {
    /// This place after an edit of its document.
    pub fn moved(mut self, document: DocumentId, change: Change) -> Self {
        if self.document == document {
            self.byte = shifted(self.byte, change);
        }
        self
    }
}

/// A place the list went to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub place: Place,
    /// How many places were passed over and forgotten on the way, because they
    /// were not usable (their file is gone).
    pub skipped: usize,
}

#[derive(Debug, Default)]
pub struct JumpList {
    entries: Vec<Place>,
    /// The entry the list is at, or `entries.len()` when it is at the present
    /// (nothing has gone back since the last jump).
    at: usize,
}

impl JumpList {
    /// Notes the place a jump leaves. What was ahead after going back is
    /// dropped. The same place twice in a row is kept once.
    pub fn record(&mut self, left: Place) {
        self.entries.truncate(self.at);
        if self.entries.last() != Some(&left) {
            self.entries.push(left);
        }
        if self.entries.len() > LIMIT {
            self.entries.remove(0);
        }
        self.at = self.entries.len();
    }

    /// Goes back from `here`, to the nearest older place that is `usable`;
    /// the ones passed that are not are forgotten. `None` when there is none.
    /// `here` is remembered, so that going forward comes back to it.
    pub fn back(&mut self, here: Place, usable: impl Fn(&Place) -> bool) -> Option<Step> {
        if self.entries.is_empty() {
            return None;
        }
        if self.at >= self.entries.len() {
            if self.entries.last() != Some(&here) {
                self.entries.push(here);
                if self.entries.len() > LIMIT {
                    self.entries.remove(0);
                }
            }
            self.at = self.entries.len() - 1;
        }
        let mut skipped = 0;
        let mut index = self.at;
        while index > 0 {
            index -= 1;
            let place = &self.entries[index];
            if same_text(place, &self.entries[self.at]) {
                // Back to where it already is is no way back: dropped without a word.
                self.entries.remove(index);
                self.at -= 1;
            } else if usable(place) {
                self.at = index;
                return Some(Step {
                    place: self.entries[index].clone(),
                    skipped,
                });
            } else {
                self.entries.remove(index);
                self.at -= 1;
                skipped += 1;
            }
        }
        None
    }

    /// Goes forward, after going back, to the nearest newer place that is
    /// `usable`; the ones passed that are not are forgotten.
    pub fn forward(&mut self, usable: impl Fn(&Place) -> bool) -> Option<Step> {
        let mut skipped = 0;
        let index = self.at + 1;
        while index < self.entries.len() {
            if usable(&self.entries[index]) {
                self.at = index;
                return Some(Step {
                    place: self.entries[index].clone(),
                    skipped,
                });
            }
            self.entries.remove(index);
            skipped += 1;
        }
        None
    }

    /// Moves the places of `document` through an edit of it. A place inside
    /// what was replaced goes to the start of it.
    pub fn remap(&mut self, document: DocumentId, change: Change) {
        for place in self
            .entries
            .iter_mut()
            .filter(|place| place.document == document)
        {
            place.byte = shifted(place.byte, change);
        }
    }

    /// Gives a place its document back: the tab of a file was closed and the
    /// file is open again under a new document.
    pub fn reopened(&mut self, path: &PathBuf, document: DocumentId) {
        for place in self
            .entries
            .iter_mut()
            .filter(|place| place.path.as_ref() == Some(path))
        {
            place.document = document;
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Where a byte ends up after `change`: text typed at it goes after it.
fn shifted(byte: usize, change: Change) -> usize {
    if byte <= change.start {
        byte
    } else if byte >= change.old_end {
        byte - change.old_end + change.new_end
    } else {
        change.start
    }
}

fn same_text(a: &Place, b: &Place) -> bool {
    a.document == b.document && a.byte == b.byte
}

#[cfg(test)]
mod tests;
