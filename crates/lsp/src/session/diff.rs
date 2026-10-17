//! What changed between the text a server has and the text now, as the single
//! change that turns one into the other: the common start and the common end
//! stay, and what lies between them is replaced.
//!
//! This does not depend on how the text got from one state to the other (typing,
//! undo, replace all, a reload from disk), so it cannot disagree with the text.

use ropey::Rope;

use crate::position::{LspPosition, to_lsp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The part of the old text that is replaced.
    pub start: LspPosition,
    pub end: LspPosition,
    /// What it is replaced by.
    pub text: String,
    /// The same change in bytes, for moving what points into the text.
    pub bytes: ByteChange,
}

/// A change as byte offsets: the bytes `start..old_end` of the old text became the
/// bytes `start..new_end` of the new text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteChange {
    pub start: usize,
    pub old_end: usize,
    pub new_end: usize,
}

/// The change from `old` to `new`, or `None` when they are the same text.
pub fn change_between(old: &Rope, new: &Rope) -> Option<Change> {
    let (old_len, new_len) = (old.len_bytes(), new.len_bytes());
    let mut prefix = common_prefix(old, new);
    if prefix == old_len && prefix == new_len {
        return None;
    }
    let mut suffix = common_suffix(old, new, prefix);

    // Never split a character, nor a `\r\n`: positions inside either mean nothing.
    while prefix > 0 && !splits_nothing(old, new, prefix, prefix) {
        prefix -= 1;
    }
    suffix = suffix.min(old_len - prefix).min(new_len - prefix);
    while suffix > 0 && !splits_nothing(old, new, old_len - suffix, new_len - suffix) {
        suffix -= 1;
    }

    let (old_end, new_end) = (old_len - suffix, new_len - suffix);
    Some(Change {
        start: to_lsp(old, prefix),
        end: to_lsp(old, old_end),
        text: new.byte_slice(prefix..new_end).to_string(),
        bytes: ByteChange {
            start: prefix,
            old_end,
            new_end,
        },
    })
}

/// Whether cutting `old` at byte `at_old` and `new` at byte `at_new` leaves
/// whole characters on both sides, and no `\r\n` in two.
fn splits_nothing(old: &Rope, new: &Rope, at_old: usize, at_new: usize) -> bool {
    let whole = |rope: &Rope, at: usize| rope.try_byte_to_char(at).is_ok() && boundary(rope, at);
    whole(old, at_old)
        && whole(new, at_new)
        && !between_cr_and_lf(old, at_old)
        && !between_cr_and_lf(new, at_new)
}

/// Whether `at` is the start of a character (or the end of the text).
fn boundary(rope: &Rope, at: usize) -> bool {
    if at >= rope.len_bytes() {
        return true;
    }
    // The first byte of a character is never a continuation byte.
    rope.byte(at) & 0b1100_0000 != 0b1000_0000
}

fn between_cr_and_lf(rope: &Rope, at: usize) -> bool {
    at > 0 && at < rope.len_bytes() && rope.byte(at - 1) == b'\r' && rope.byte(at) == b'\n'
}

/// How many bytes the two texts start with in common.
fn common_prefix(a: &Rope, b: &Rope) -> usize {
    let (mut chunks_a, mut chunks_b) = (a.chunks(), b.chunks());
    let (mut rest_a, mut rest_b): (&[u8], &[u8]) = (&[], &[]);
    let mut total = 0;
    loop {
        while rest_a.is_empty() {
            match chunks_a.next() {
                Some(chunk) => rest_a = chunk.as_bytes(),
                None => return total,
            }
        }
        while rest_b.is_empty() {
            match chunks_b.next() {
                Some(chunk) => rest_b = chunk.as_bytes(),
                None => return total,
            }
        }
        let same = rest_a
            .iter()
            .zip(rest_b)
            .take_while(|(left, right)| left == right)
            .count();
        total += same;
        if same < rest_a.len().min(rest_b.len()) {
            return total;
        }
        rest_a = &rest_a[same..];
        rest_b = &rest_b[same..];
    }
}

/// How many bytes the two texts end with in common, not counting into the
/// first `skip` bytes of either (the start they already share).
fn common_suffix(a: &Rope, b: &Rope, skip: usize) -> usize {
    let limit = a.len_bytes().min(b.len_bytes()) - skip.min(a.len_bytes().min(b.len_bytes()));
    let (mut chunks_a, mut chunks_b) = (end_of(a), end_of(b));
    let (mut rest_a, mut rest_b): (&[u8], &[u8]) = (&[], &[]);
    let mut total = 0;
    while total < limit {
        while rest_a.is_empty() {
            match chunks_a.prev() {
                Some(chunk) => rest_a = chunk.as_bytes(),
                None => return total,
            }
        }
        while rest_b.is_empty() {
            match chunks_b.prev() {
                Some(chunk) => rest_b = chunk.as_bytes(),
                None => return total,
            }
        }
        let same = rest_a
            .iter()
            .rev()
            .zip(rest_b.iter().rev())
            .take(limit - total)
            .take_while(|(left, right)| left == right)
            .count();
        total += same;
        if same < rest_a.len().min(rest_b.len()).min(limit - total + same) {
            return total;
        }
        rest_a = &rest_a[..rest_a.len() - same];
        rest_b = &rest_b[..rest_b.len() - same];
    }
    total
}

/// Chunks positioned after the last one, so that `prev` gives the last.
fn end_of(rope: &Rope) -> ropey::iter::Chunks<'_> {
    rope.chunks_at_byte(rope.len_bytes()).0
}
