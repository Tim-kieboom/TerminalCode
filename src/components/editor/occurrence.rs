//! `ctrl+d`: adds the next occurrence of the selected text to the selections, so the next edit
//! reaches all of them. With a bare cursor it first selects the word the cursor is in.
//! `ctrl+shift+l` adds all of them, `ctrl+k ctrl+d` moves the newest one on to the next
//! occurrence (skips one) and `ctrl+u` takes the newest one back.

use std::ops::Range;

use crate::buffer::{BufferError, Selection, Selections};

use super::Editor;

/// What `ctrl+d` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Occurrence {
    /// The cursor was in a word: the word is selected now.
    Word,
    /// The next occurrence was added, and is the primary selection.
    Added,
    /// Every occurrence is selected already.
    NoMore,
    /// A bare cursor that is not in or next to a word: nothing to take.
    NoWord,
    /// The primary selection moved on to the next occurrence, leaving the old one unselected.
    Skipped,
    /// This many occurrences were added (every one that was not selected yet).
    All(usize),
}

impl Editor {
    /// `ctrl+d`. With the primary selection bare, selects the word it is in (or just after).
    /// Otherwise looks for the text of the primary selection after it, going round to the
    /// start of the text, and adds the first match that does not run into a selection there
    /// is; it becomes the primary one. Text that is a word on its own (letters, digits and
    /// `_`) only matches whole words, other text matches wherever it is.
    pub(crate) fn select_next_occurrence(&mut self) -> Result<Occurrence, BufferError> {
        self.view.last_edit = None;
        let primary = *self.view.selections.primary();
        if primary.is_empty() {
            return self.select_word_at(&primary);
        }

        let text = self.buffer.text();
        let wanted = self.bytes_of(&primary)?;
        let needle = &text[wanted.clone()];
        let taken = self.taken_ranges()?;
        let Some(found) = next_match(&text, needle, wanted.end, &taken) else {
            return Ok(Occurrence::NoMore);
        };
        let added = Selection::new(
            self.buffer.byte_to_position(found.start)?,
            self.buffer.byte_to_position(found.end)?,
        );
        self.view.selections = self.view.selections.with_added(added);
        Ok(Occurrence::Added)
    }

    /// `ctrl+shift+l`. Selects every occurrence of the primary selection's text (the word at a
    /// bare cursor) that is not selected yet, keeping the selections there are; the primary one
    /// stays the primary one. Matches as `ctrl+d` does.
    pub(crate) fn select_all_occurrences(&mut self) -> Result<Occurrence, BufferError> {
        self.view.last_edit = None;
        let primary = *self.view.selections.primary();
        if primary.is_empty() {
            let word = self.select_word_at(&primary)?;
            if word != Occurrence::Word {
                return Ok(word);
            }
        }
        let text = self.buffer.text();
        let wanted = self.bytes_of(self.view.selections.primary())?;
        let needle = &text[wanted];
        let taken = self.taken_ranges()?;
        let found = all_matches(&text, needle, &taken);
        if found.is_empty() {
            return Ok(Occurrence::NoMore);
        }
        let mut items: Vec<Selection> = self.view.selections.iter().copied().collect();
        for range in &found {
            items.push(Selection::new(
                self.buffer.byte_to_position(range.start)?,
                self.buffer.byte_to_position(range.end)?,
            ));
        }
        self.view.selections = Selections::from_items(items, self.view.selections.primary_index());
        Ok(Occurrence::All(found.len()))
    }

    /// `ctrl+k ctrl+d`. Moves the primary selection to the next occurrence of its text, as
    /// `ctrl+d` finds it, and leaves the old place unselected. At a bare cursor it selects the
    /// word, as `ctrl+d` does.
    pub(crate) fn skip_occurrence(&mut self) -> Result<Occurrence, BufferError> {
        self.view.last_edit = None;
        let primary = *self.view.selections.primary();
        if primary.is_empty() {
            return self.select_word_at(&primary);
        }
        let text = self.buffer.text();
        let wanted = self.bytes_of(&primary)?;
        let needle = &text[wanted.clone()];
        let taken = self.taken_ranges()?;
        let Some(found) = next_match(&text, needle, wanted.end, &taken) else {
            return Ok(Occurrence::NoMore);
        };
        let moved = Selection::new(
            self.buffer.byte_to_position(found.start)?,
            self.buffer.byte_to_position(found.end)?,
        );
        let primary_index = self.view.selections.primary_index();
        let items: Vec<Selection> = self
            .view
            .selections
            .iter()
            .enumerate()
            .map(|(index, selection)| match index == primary_index {
                true => moved,
                false => *selection,
            })
            .collect();
        self.view.selections = Selections::from_items(items, primary_index);
        Ok(Occurrence::Skipped)
    }

    /// `ctrl+u`. Drops the primary selection, and the one before it (in the text) becomes the
    /// primary one. Does nothing, and says `false`, when there is only one.
    pub(crate) fn take_back_selection(&mut self) -> bool {
        if self.view.selections.is_single() {
            return false;
        }
        self.view.last_edit = None;
        let at = self.view.selections.primary_index();
        let mut items: Vec<Selection> = self.view.selections.iter().copied().collect();
        items.remove(at);
        self.view.selections = Selections::from_items(items, at.saturating_sub(1));
        true
    }

    /// The byte ranges of every selection.
    fn taken_ranges(&self) -> Result<Vec<Range<usize>>, BufferError> {
        self.view
            .selections
            .iter()
            .map(|selection| self.bytes_of(selection))
            .collect()
    }

    /// Selects the word at the bare cursor `at`, leaving the other selections.
    fn select_word_at(&mut self, at: &Selection) -> Result<Occurrence, BufferError> {
        let head = at.head();
        let line_start = self
            .buffer
            .position_to_byte(crate::buffer::Position::new(head.line, 0))?;
        let cursor = self.buffer.position_to_byte(head)?;
        let content = self.buffer.line_content(head.line)?;
        let local = cursor - line_start;
        let start = content[..local]
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_word_char(*c))
            .last()
            .map_or(local, |(index, _)| index);
        let end = content[local..]
            .char_indices()
            .find(|(_, c)| !is_word_char(*c))
            .map_or(content.len(), |(index, _)| local + index);
        if start == end {
            return Ok(Occurrence::NoWord);
        }
        let word = Selection::new(
            self.buffer.byte_to_position(line_start + start)?,
            self.buffer.byte_to_position(line_start + end)?,
        );
        let items: Vec<Selection> = self
            .view
            .selections
            .iter()
            .enumerate()
            .map(
                |(index, selection)| match index == self.view.selections.primary_index() {
                    true => word,
                    false => *selection,
                },
            )
            .collect();
        self.view.selections = Selections::from_items(items, self.view.selections.primary_index());
        Ok(Occurrence::Word)
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `needle` at byte `at` of `text` is a place to select: it does not run into one of
/// `taken` (byte ranges) and, for a needle that is a word, has no word character next to it.
fn fits(text: &str, needle: &str, at: usize, taken: &[Range<usize>]) -> bool {
    let range = at..at + needle.len();
    let whole_word = needle.chars().all(is_word_char);
    let alone = !whole_word
        || (!text[..range.start]
            .chars()
            .next_back()
            .is_some_and(is_word_char)
            && !text[range.end..].chars().next().is_some_and(is_word_char));
    alone
        && !taken
            .iter()
            .any(|other| other.start < range.end && range.start < other.end)
}

/// The first place of `needle` in `text` from byte `from` on, going round to the start, that
/// `fits`.
fn next_match(
    text: &str,
    needle: &str,
    from: usize,
    taken: &[Range<usize>],
) -> Option<Range<usize>> {
    if needle.is_empty() {
        return None;
    }
    // From the end of the selection, then from the start of the text up to it.
    let from = from.min(text.len());
    let after = text[from..].match_indices(needle).map(|(at, _)| from + at);
    let before = text[..from].match_indices(needle).map(|(at, _)| at);
    after
        .chain(before)
        .find(|at| fits(text, needle, *at, taken))
        .map(|at| at..at + needle.len())
}

/// Every place of `needle` in `text` that `fits`, in text order.
fn all_matches(text: &str, needle: &str, taken: &[Range<usize>]) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    text.match_indices(needle)
        .map(|(at, _)| at)
        .filter(|at| fits(text, needle, *at, taken))
        .map(|at| at..at + needle.len())
        .collect()
}

#[cfg(test)]
mod tests;
