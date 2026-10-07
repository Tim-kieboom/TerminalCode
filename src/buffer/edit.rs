use std::ops::Range;

use super::position::Point;

/// Replaces a byte range with new text. Every change to a buffer is one of
/// these: insertion is an empty range, deletion is empty text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edit {
    range: Range<usize>,
    text: Box<str>,
}

impl Edit {
    pub(crate) fn new(range: Range<usize>, text: impl Into<Box<str>>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }

    pub(crate) fn insert(at: usize, text: impl Into<Box<str>>) -> Self {
        Self::new(at..at, text)
    }

    pub(crate) fn delete(range: Range<usize>) -> Self {
        Self::new(range, "")
    }

    pub(crate) fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

/// Positions of an applied edit, in the form incremental parsers consume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EditInfo {
    pub(crate) start_byte: usize,
    pub(crate) old_end_byte: usize,
    pub(crate) new_end_byte: usize,
    pub(crate) start_point: Point,
    pub(crate) old_end_point: Point,
    pub(crate) new_end_point: Point,
}

/// What [`Buffer::apply`](super::Buffer::apply) hands back: the edit that
/// undoes the change, and the positions for incremental consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppliedEdit {
    pub(crate) inverse: Edit,
    pub(crate) info: EditInfo,
}
