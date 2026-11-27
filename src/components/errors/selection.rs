//! Which problem of the Errors view is selected. The list is made again from what the language
//! servers say at every draw, and a server says it all again on almost every keystroke, so a
//! row number would point at another problem after a while. The selection is the problem
//! itself: an id made from what it says, and the line it was on.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use super::ErrorRow;

/// What tells a problem from the others, kept while the server says it again: a hash of its
/// file, its message and its code, and for equal ones of a file, their number among them. Its
/// place is not in it, so typing above a problem does not lose it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct ProblemId(u64);

/// Gives each row its id. `rows` is in the order of the list, so the equal problems of one file
/// are numbered by where they are.
pub(crate) fn identify(rows: &mut [ErrorRow]) {
    let mut seen: HashMap<u64, u64> = HashMap::new();
    for row in rows {
        let mut hasher = DefaultHasher::new();
        (&row.file, &row.message, &row.code).hash(&mut hasher);
        let base = hasher.finish();
        let number = seen.entry(base).or_default();
        let mut hasher = DefaultHasher::new();
        (base, *number).hash(&mut hasher);
        *number += 1;
        row.id = ProblemId(hasher.finish());
    }
}

/// The problem that is selected, as it was when last seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pin {
    id: ProblemId,
    line: usize,
    /// The text version of the file the line is of; `None` for a file that is not open.
    version: Option<u64>,
}

impl Pin {
    pub(super) fn of(row: &ErrorRow) -> Self {
        Self {
            id: row.id,
            line: row.position.line,
            version: row.version,
        }
    }

    pub(super) fn id(&self) -> ProblemId {
        self.id
    }

    /// Whether `row`, which has this pin's id, is not the problem that was selected: the text
    /// is the same as when the pin was last seen, yet the problem is on another line, so it was
    /// replaced (the first of two equal problems was fixed and the second took its number),
    /// not moved by an edit above it. A file that is not open has no version: the id decides.
    pub(super) fn replaced_by(&self, row: &ErrorRow) -> bool {
        row.version.is_some() && row.version == self.version && row.position.line != self.line
    }
}

/// What is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Selection {
    /// Nothing was chosen yet: the first row is, as soon as there is one.
    #[default]
    Start,
    Pinned(Pin),
    /// The selected problem is gone, and nothing is selected until the user chooses.
    Nothing,
}
