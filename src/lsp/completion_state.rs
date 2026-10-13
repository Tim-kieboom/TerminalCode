//! The box of suggestions, without the drawing: the list a server sent, narrowed
//! to what has been typed, in the server's order, with one highlighted; and
//! what picking one writes.
//!
//! Everything is in bytes of the text as it is now. The list is converted from
//! the server's positions when it arrives, and then moved through each change
//! made to the text ([`Completion::apply_change`]), so that picking an item
//! after more typing still replaces the whole word and still puts the import at
//! the top of the file.

use std::ops::Range;

use ropey::Rope;

use super::diagnostics::map_range;
use super::diff::ByteChange;
use super::position::{LspPosition, from_lsp};
use super::{CompletionItem, CompletionList};

/// A stretch of text to replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ByteEdit {
    pub(crate) range: Range<usize>,
    pub(crate) text: String,
}

/// What picking an item writes: edits in text order that do not overlap, to be
/// applied together as one undo step, and where the cursor goes (a byte of the
/// text as it is after them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Accepted {
    pub(crate) edits: Vec<ByteEdit>,
    pub(crate) cursor: usize,
}

/// Whether the box stays open after the text or the cursor moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refresh {
    Open,
    /// The cursor left the word, or the word stopped being one.
    Closed,
}

#[derive(Debug)]
struct Entry {
    item: CompletionItem,
    /// What picking it writes in place of the word.
    text: String,
    /// The bytes the server said to replace; `None` when it did not say, or
    /// once an edit took them (then the word is replaced).
    replaces: Option<Range<usize>>,
    /// Edits elsewhere that go with it: the `use` line of an import.
    extra: Vec<ByteEdit>,
    /// What the typed text is compared with, in lower case.
    filter: String,
}

#[derive(Debug)]
pub(crate) struct Completion {
    /// Every item of the list, in the server's order.
    entries: Vec<Entry>,
    /// The entries that start with what is typed, as indexes into `entries`.
    shown: Vec<usize>,
    /// Which of `shown` is highlighted.
    selected: usize,
    /// Where the word being typed starts.
    word_start: usize,
    /// What is typed of it.
    prefix: String,
    /// The server said its list leaves items out that a longer word would have.
    incomplete: bool,
    /// What was typed when the list was asked for.
    asked_for: String,
}

impl Completion {
    /// The box for `list`, which the server made for the word that starts at
    /// `word_start` and has been typed up to `cursor` of `rope`, the text the
    /// server saw.
    pub(crate) fn new(list: CompletionList, rope: &Rope, word_start: usize, cursor: usize) -> Self {
        let prefix = text_between(rope, word_start, cursor);
        let mut completion = Self {
            entries: Vec::new(),
            shown: Vec::new(),
            selected: 0,
            word_start,
            asked_for: prefix.clone(),
            prefix,
            incomplete: false,
        };
        completion.load(list, rope);
        completion
    }

    /// Takes a newer answer for the same word, which was made for the text `rope`
    /// (the text as it is now: an answer for older text is to be dropped by the
    /// caller). What was highlighted stays highlighted if it is still there.
    pub(crate) fn replace_list(&mut self, list: CompletionList, rope: &Rope, cursor: usize) {
        let kept = self.selected_item().map(|item| item.label.clone());
        self.asked_for = text_between(rope, self.word_start, cursor);
        self.load(list, rope);
        if let Some(label) = kept {
            self.select_label(&label);
        }
    }

    fn load(&mut self, list: CompletionList, rope: &Rope) {
        self.incomplete = list.incomplete;
        let mut items = list.items;
        // The server's order; the label breaks a tie, so the order is the same every time.
        items.sort_by(|a, b| (&a.sort_text, &a.label).cmp(&(&b.sort_text, &b.label)));
        self.entries = items.into_iter().map(|item| entry(item, rope)).collect();
        self.filter();
    }

    /// Whether the list has to be asked for again: the word is not an extension
    /// of the one the list was made for (it was shortened, or a new word began),
    /// or it grew and the list was incomplete.
    pub(crate) fn wants_new_list(&self) -> bool {
        match self.prefix.strip_prefix(&self.asked_for) {
            None => true,
            Some(more) => self.incomplete && !more.is_empty(),
        }
    }

    /// Moves the box through a change made to the text.
    pub(crate) fn apply_change(&mut self, change: &ByteChange) {
        self.word_start = map_start(self.word_start, change);
        for entry in &mut self.entries {
            entry.replaces = entry
                .replaces
                .take()
                .and_then(|range| map_range(range, change));
            entry
                .extra
                .retain_mut(|edit| match map_range(edit.range.clone(), change) {
                    Some(range) => {
                        edit.range = range;
                        true
                    }
                    None => false,
                });
        }
    }

    /// Reads the word again after the text or the cursor moved (call it after
    /// [`Completion::apply_change`]); narrows the list to it.
    pub(crate) fn retarget(&mut self, rope: &Rope, cursor: usize) -> Refresh {
        if cursor < self.word_start || cursor > rope.len_bytes() {
            return Refresh::Closed;
        }
        let word = text_between(rope, self.word_start, cursor);
        if !word.chars().all(is_word_char) {
            return Refresh::Closed;
        }
        let kept = self.selected_item().map(|item| item.label.clone());
        self.prefix = word;
        self.filter();
        if let Some(label) = kept {
            self.select_label(&label);
        }
        Refresh::Open
    }

    fn filter(&mut self) {
        let typed = self.prefix.to_lowercase();
        self.shown = (0..self.entries.len())
            .filter(|&index| self.entries[index].filter.starts_with(&typed))
            .collect();
        self.selected = 0;
    }

    fn select_label(&mut self, label: &str) {
        if let Some(position) = self
            .shown
            .iter()
            .position(|&index| self.entries[index].item.label == label)
        {
            self.selected = position;
        }
    }

    /// Where the word being typed starts.
    pub(crate) fn word_start(&self) -> usize {
        self.word_start
    }

    /// Nothing matches what is typed: the box has nothing to show.
    pub(crate) fn is_empty(&self) -> bool {
        self.shown.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.shown.len()
    }

    /// The items that match, in the server's order.
    pub(crate) fn items(&self) -> impl Iterator<Item = &CompletionItem> {
        self.shown.iter().map(|&index| &self.entries[index].item)
    }

    pub(crate) fn selected_index(&self) -> usize {
        self.selected
    }

    pub(crate) fn selected_item(&self) -> Option<&CompletionItem> {
        self.shown
            .get(self.selected)
            .map(|&index| &self.entries[index].item)
    }

    /// The next item; after the last, the first.
    pub(crate) fn select_next(&mut self) {
        if !self.shown.is_empty() {
            self.selected = (self.selected + 1) % self.shown.len();
        }
    }

    /// The previous item; before the first, the last.
    pub(crate) fn select_previous(&mut self) {
        if !self.shown.is_empty() {
            self.selected = (self.selected + self.shown.len() - 1) % self.shown.len();
        }
    }

    /// What picking the highlighted item writes, with the cursor at `cursor`.
    /// `None` when nothing is shown.
    pub(crate) fn accept(&self, cursor: usize) -> Option<Accepted> {
        let entry = &self.entries[*self.shown.get(self.selected)?];
        // The word as it is now, which is longer than the one the server saw
        // when more was typed since; the server's range is only a start.
        let main = match &entry.replaces {
            Some(range) => range.start..range.end.max(cursor),
            None => self.word_start..cursor,
        };
        let mut edits = vec![ByteEdit {
            range: main.clone(),
            text: entry.text.clone(),
        }];
        // An edit that reaches into the word would not apply to anything sane.
        edits.extend(
            entry
                .extra
                .iter()
                .filter(|edit| !overlaps(&edit.range, &main))
                .cloned(),
        );
        edits.sort_by_key(|edit| (edit.range.start, edit.range.end));

        // The cursor goes after the word, past whatever was added before it.
        let shift: isize = edits
            .iter()
            .filter(|edit| edit.range != main && edit.range.end <= main.start)
            .map(|edit| to_isize(edit.text.len()) - to_isize(edit.range.len()))
            .sum();
        let cursor = main.start.saturating_add_signed(shift) + entry.text.len();
        Some(Accepted { edits, cursor })
    }
}

fn entry(item: CompletionItem, rope: &Rope) -> Entry {
    let at = |position: LspPosition| from_lsp(rope, position);
    // A snippet's `$1` and `${2:x}` would be written as they are, which is wrong,
    // and none is asked for; the plain name is the safe reading.
    let text = match (&item.text_edit, &item.insert_text, item.snippet) {
        (_, _, true) => item.label.clone(),
        (Some(edit), _, false) => edit.new_text.clone(),
        (None, Some(text), false) => text.clone(),
        (None, None, false) => item.label.clone(),
    };
    let replaces = item
        .text_edit
        .as_ref()
        .filter(|_| !item.snippet)
        .map(|edit| at(edit.start)..at(edit.end))
        .filter(|range| range.start <= range.end);
    let extra = item
        .additional_edits
        .iter()
        .map(|edit| (at(edit.start)..at(edit.end), edit.new_text.clone()))
        .filter(|(range, _)| range.start <= range.end)
        .map(|(range, text)| ByteEdit { range, text })
        .collect();
    Entry {
        filter: item.filter_text.to_lowercase(),
        text,
        replaces,
        extra,
        item,
    }
}

/// Two stretches overlap when they share text; one that only touches the other,
/// or an insertion at a point, does not.
fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

/// Where the start of a word ends up after `change`: text typed at it goes after it.
fn map_start(start: usize, change: &ByteChange) -> usize {
    if start <= change.start {
        start
    } else if start >= change.old_end {
        start - change.old_end + change.new_end
    } else {
        change.start
    }
}

fn text_between(rope: &Rope, start: usize, end: usize) -> String {
    let end = end.min(rope.len_bytes());
    let start = start.min(end);
    rope.slice(rope.byte_to_char(start)..rope.byte_to_char(end))
        .to_string()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn to_isize(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}
