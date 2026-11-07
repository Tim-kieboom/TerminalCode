//! `ctrl+d`: adds the next occurrence of the selected text to the selections, so the next edit
//! reaches all of them. With a bare cursor it first selects the word the cursor is in.

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
        let taken: Vec<Range<usize>> = self
            .view
            .selections
            .iter()
            .map(|selection| self.bytes_of(selection))
            .collect::<Result<_, _>>()?;
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

/// The first place of `needle` in `text` from byte `from` on, going round to the start, that
/// does not run into one of `taken` (byte ranges) and, for a needle that is a word, has no
/// word character next to it.
fn next_match(
    text: &str,
    needle: &str,
    from: usize,
    taken: &[Range<usize>],
) -> Option<Range<usize>> {
    if needle.is_empty() {
        return None;
    }
    let whole_word = needle.chars().all(is_word_char);
    let fits = |at: usize| {
        let range = at..at + needle.len();
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
    };
    // From the end of the selection, then from the start of the text up to it.
    let from = from.min(text.len());
    let after = text[from..].match_indices(needle).map(|(at, _)| from + at);
    let before = text[..from].match_indices(needle).map(|(at, _)| at);
    after
        .chain(before)
        .find(|at| fits(*at))
        .map(|at| at..at + needle.len())
}

#[cfg(test)]
mod tests;
