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

use super::completion::Kind;
use super::diagnostics::map_range;
use super::diff::ByteChange;
use super::position::{LspPosition, from_lsp};
use super::snippet::expand;
use super::{CompletionItem, CompletionList};

/// A stretch of text to replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ByteEdit {
    pub(crate) range: Range<usize>,
    pub(crate) text: String,
}

/// What picking an item writes: edits in text order that do not overlap, to be
/// applied together as one undo step, and what is selected after them (bytes of the
/// text as it is after them): `anchor..cursor`, which is empty for a plain cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Accepted {
    pub(crate) edits: Vec<ByteEdit>,
    pub(crate) anchor: usize,
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
    /// What is selected in `text` after it is written: `anchor..cursor`, empty for
    /// a cursor, at the end unless it is a snippet.
    anchor: usize,
    cursor: usize,
    /// The bytes the server said to replace; `None` when it did not say, or
    /// once an edit took them (then the word is replaced).
    replaces: Option<Range<usize>>,
    /// Edits elsewhere that go with it: the `use` line of an import.
    extra: Vec<ByteEdit>,
    /// What the typed text is compared with, in lower case.
    filter: String,
}

/// An item told apart from the others: two can share a label (a module and a type
/// both called `Json`), so the kind and the detail count too, and when even those are
/// equal, which of them it is.
#[derive(Debug)]
struct Highlight {
    label: String,
    kind: Kind,
    detail: Option<String>,
    nth: usize,
}

fn same_item(a: &CompletionItem, b: &CompletionItem) -> bool {
    a.label == b.label && a.kind == b.kind && a.detail == b.detail
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

    fn load(&mut self, list: CompletionList, rope: &Rope) {
        self.incomplete = list.incomplete;
        let mut items = list.items;
        // The server's order; the label breaks a tie, so the order is the same every time.
        items.sort_by(|a, b| (&a.sort_text, &a.label).cmp(&(&b.sort_text, &b.label)));
        self.entries = items
            .into_iter()
            .map(|item| entry(item, rope, self.word_start))
            .collect();
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
        // The entry itself, not its label: two entries can have the same one.
        let kept = self.shown.get(self.selected).copied();
        self.prefix = word;
        self.filter();
        if let Some(position) = kept.and_then(|kept| self.shown.iter().position(|&i| i == kept)) {
            self.selected = position;
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

    /// What tells the highlighted item apart in another list for the same word.
    fn highlight(&self) -> Option<Highlight> {
        let item = &self.entries[*self.shown.get(self.selected)?].item;
        let nth = self.shown[..self.selected]
            .iter()
            .filter(|&&index| same_item(&self.entries[index].item, item))
            .count();
        Some(Highlight {
            label: item.label.clone(),
            kind: item.kind,
            detail: item.detail.clone(),
            nth,
        })
    }

    fn select_like(&mut self, wanted: &Highlight) {
        let position = self
            .shown
            .iter()
            .enumerate()
            .filter(|(_, index)| {
                let item = &self.entries[**index].item;
                item.label == wanted.label
                    && item.kind == wanted.kind
                    && item.detail == wanted.detail
            })
            .nth(wanted.nth)
            .map(|(position, _)| position);
        if let Some(position) = position {
            self.selected = position;
        }
    }

    /// Where the word being typed starts.
    pub(crate) fn word_start(&self) -> usize {
        self.word_start
    }

    /// What is typed of the word.
    pub(crate) fn typed(&self) -> &str {
        &self.prefix
    }

    /// The server said its list leaves items out that a longer word would have.
    pub(crate) fn is_incomplete(&self) -> bool {
        self.incomplete
    }

    /// Highlights the item that `previous` had highlighted, if this has it too.
    pub(crate) fn keep_highlight_of(&mut self, previous: &Completion) {
        if let Some(highlight) = previous.highlight() {
            self.select_like(&highlight);
        }
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
        self.accept_with(cursor, Vec::new())
    }

    /// The same, with the edits the server named when asked to resolve the item
    /// (in bytes of the text as it is now); they are used in place of the ones the
    /// item came with, if there are any.
    pub(crate) fn accept_with(&self, cursor: usize, resolved: Vec<ByteEdit>) -> Option<Accepted> {
        let entry = &self.entries[*self.shown.get(self.selected)?];
        let extra = match resolved.is_empty() {
            true => entry.extra.clone(),
            false => resolved,
        };
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
            extra
                .into_iter()
                .filter(|edit| !overlaps(&edit.range, &main)),
        );
        edits.sort_by_key(|edit| (edit.range.start, edit.range.end));

        // The cursor goes after the word, past whatever was added before it.
        let shift: isize = edits
            .iter()
            .filter(|edit| edit.range != main && edit.range.end <= main.start)
            .map(|edit| to_isize(edit.text.len()) - to_isize(edit.range.len()))
            .sum();
        let base = main.start.saturating_add_signed(shift);
        Some(Accepted {
            edits,
            anchor: base + entry.anchor,
            cursor: base + entry.cursor,
        })
    }
}

/// `edits` of the server, in bytes of `rope`, the text they were made for.
pub(crate) fn byte_edits(edits: &[super::TextEdit], rope: &Rope) -> Vec<ByteEdit> {
    edits
        .iter()
        .map(|edit| (from_lsp(rope, edit.start)..from_lsp(rope, edit.end), edit))
        .filter(|(range, _)| range.start <= range.end)
        .map(|(range, edit)| ByteEdit {
            range,
            text: edit.new_text.clone(),
        })
        .collect()
}

fn entry(item: CompletionItem, rope: &Rope, word_start: usize) -> Entry {
    let at = |position: LspPosition| from_lsp(rope, position);
    let written = match (&item.text_edit, &item.insert_text) {
        (Some(edit), _) => edit.new_text.clone(),
        (None, Some(text)) => text.clone(),
        (None, None) => item.label.clone(),
    };
    let replaces = item
        .text_edit
        .as_ref()
        .map(|edit| at(edit.start)..at(edit.end))
        .filter(|range| range.start <= range.end);
    let extra = item
        .additional_edits
        .iter()
        .map(|edit| (at(edit.start)..at(edit.end), edit.new_text.clone()))
        .filter(|(range, _)| range.start <= range.end)
        .map(|(range, text)| ByteEdit { range, text })
        .collect();
    // A snippet is written with the defaults of its placeholders, the first stop
    // selected, and its lines indented like the line it goes into.
    let (text, anchor, cursor) = if item.snippet {
        let from = replaces.as_ref().map_or(word_start, |range| range.start);
        let expanded = expand(&written, &indent_of_line_at(rope, from));
        (expanded.text, expanded.anchor, expanded.cursor)
    } else {
        let end = written.len();
        (written, end, end)
    };
    Entry {
        filter: item.filter_text.to_lowercase(),
        text,
        anchor,
        cursor,
        replaces,
        extra,
        item,
    }
}

/// The white space the line that holds `byte` starts with.
fn indent_of_line_at(rope: &Rope, byte: usize) -> String {
    let line = rope.byte_to_line(byte.min(rope.len_bytes()));
    rope.line(line)
        .chars()
        .take_while(|c| matches!(c, ' ' | '\t'))
        .collect()
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
