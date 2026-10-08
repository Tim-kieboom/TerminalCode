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

    #[cfg(test)]
    pub(crate) fn insert(at: usize, text: impl Into<Box<str>>) -> Self {
        Self::new(at..at, text)
    }

    #[cfg(test)]
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

impl EditInfo {
    /// Where a byte offset of the old text ends up after this edit. Offsets at or
    /// before the start stay put (so a cursor at an insertion point stays before
    /// the new text), offsets after the replaced range shift by the size change,
    /// and offsets inside it collapse to the start.
    pub(crate) fn remap_byte(&self, byte: usize) -> usize {
        if byte <= self.start_byte {
            return byte;
        }
        if byte >= self.old_end_byte {
            return byte - self.old_end_byte + self.new_end_byte;
        }
        self.start_byte
    }
}

/// What [`Buffer::apply`](super::Buffer::apply) hands back: the edit that
/// undoes the change, and the positions for incremental consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppliedEdit {
    pub(crate) inverse: Edit,
    pub(crate) info: EditInfo,
}
