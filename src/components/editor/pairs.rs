//! Typing brackets and quotes in pairs: an opener writes its closer too, the closer typed
//! next steps over it, an opener typed over a selection wraps it, and Backspace between an
//! empty pair takes both (that part is in `delete_range`).
//!
//! Stateless: the editor does not remember which closers it wrote. What it does depends only
//! on the characters around each cursor and on the language's pairs (`Language::pairs`).

use std::ops::Range;

use crate::buffer::{BufferError, Edit, Position, Selection, Selections};
use crate::syntax::{Language, PLAIN_PAIRS};

use super::multi::{after_change, join_overlapping};
use super::{EditKind, Editor, Motion};

/// The brackets that end a level, and so are fine to stand before a new opener.
const CLOSERS: [char; 3] = [')', ']', '}'];

/// The setting `auto_close_pairs`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoClose {
    #[default]
    On,
    Off,
}

impl AutoClose {
    pub(crate) fn from_setting(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
    }
}

/// What typing one character does at one cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Typed {
    /// The character, as it is.
    Plain,
    /// The opener and its closer, with the cursor between them.
    Pair(char),
    /// The selection between the opener and the closer, still selected.
    Wrap(char),
    /// The closer that is already there, passed over.
    StepOver,
}

/// What typing `typed` does where `before` and `after` are the characters either side of
/// the cursor (`None` at the edge of the line), or over a selection (`selected`).
pub(super) fn decide(
    pairs: &[(char, char)],
    typed: char,
    before: Option<char>,
    after: Option<char>,
    selected: bool,
) -> Typed {
    let Some(&(open, close)) = pairs
        .iter()
        .find(|&&(open, close)| open == typed || close == typed)
    else {
        return Typed::Plain;
    };
    let quote = open == close;
    if selected {
        return if open == typed {
            Typed::Wrap(close)
        } else {
            Typed::Plain
        };
    }
    if quote {
        let at_the_end = before.is_some_and(|c| is_word(c) || CLOSERS.contains(&c) || c == typed);
        if after == Some(typed) && at_the_end {
            return Typed::StepOver;
        }
        let at_the_start = !before.is_some_and(|c| is_word(c) || c == typed);
        return if at_the_start && may_precede_a_closer(after) {
            Typed::Pair(close)
        } else {
            Typed::Plain
        };
    }
    if close == typed {
        return if after == Some(typed) {
            Typed::StepOver
        } else {
            Typed::Plain
        };
    }
    if may_precede_a_closer(after) {
        Typed::Pair(close)
    } else {
        Typed::Plain
    }
}

/// Whether a pair may be opened before `after`: at the end of the line, before whitespace or
/// before a bracket that closes a level.
fn may_precede_a_closer(after: Option<char>) -> bool {
    after.is_none_or(|c| c.is_whitespace() || CLOSERS.contains(&c))
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// What one selection changes when a character is typed with pairs in play.
struct Planned {
    range: Range<usize>,
    text: String,
    after: After,
}

/// Where the cursor or selection goes once the change is made.
enum After {
    /// A cursor, with this many bytes of the change's text after it.
    Cursor(usize),
    /// A selection of `selected` bytes that ends `closer` bytes before the end of the change.
    Inside {
        selected: usize,
        closer: usize,
        backwards: bool,
    },
}

impl Editor {
    /// Types one character as the keyboard does: with the language's pairs closed, passed over
    /// and wrapped (see [`Typed`]), unless the setting is off. Other text goes in as it is.
    pub(crate) fn type_text(&mut self, text: &str) -> Result<(), BufferError> {
        let mut chars = text.chars();
        let (Some(typed), None) = (chars.next(), chars.next()) else {
            return self.insert_text(text);
        };
        if self.view.auto_close == AutoClose::Off {
            return self.insert_text(text);
        }
        let pairs = self.pairs();
        if !pairs
            .iter()
            .any(|&(open, close)| open == typed || close == typed)
        {
            return self.insert_text(text);
        }
        let mut decisions = Vec::new();
        for selection in self.view.selections.iter() {
            decisions.push(self.decide_at(pairs, typed, selection)?);
        }
        if decisions.iter().all(|decision| *decision == Typed::Plain) {
            return self.insert_text(text);
        }
        if decisions
            .iter()
            .all(|decision| *decision == Typed::StepOver)
        {
            return self.move_cursor(Motion::Right);
        }
        self.edit_typed(typed, &decisions)
    }

    /// The pairs of the file being edited.
    fn pairs(&self) -> &'static [(char, char)] {
        self.buffer
            .path()
            .and_then(Language::from_path)
            .map_or(PLAIN_PAIRS, Language::pairs)
    }

    fn decide_at(
        &self,
        pairs: &[(char, char)],
        typed: char,
        selection: &Selection,
    ) -> Result<Typed, BufferError> {
        let (before, after) = self.chars_around(selection.head())?;
        Ok(decide(pairs, typed, before, after, !selection.is_empty()))
    }

    /// The characters just before and after `at` on its line.
    fn chars_around(&self, at: Position) -> Result<(Option<char>, Option<char>), BufferError> {
        let content = self.buffer.line_content(at.line)?;
        let cursor = self.buffer.position_to_byte(at)?;
        let line_start = self.buffer.position_to_byte(Position::new(at.line, 0))?;
        let (before, after) = content.split_at(cursor - line_start);
        Ok((before.chars().next_back(), after.chars().next()))
    }

    /// The bytes of the closer right after `at` when the character before it is the opener of
    /// the same pair (`(|)`), else 0. 0 as well when the setting is off.
    pub(super) fn empty_pair_closer(&self, at: Position) -> Result<usize, BufferError> {
        if self.view.auto_close == AutoClose::Off {
            return Ok(0);
        }
        let (Some(before), Some(after)) = self.chars_around(at)? else {
            return Ok(0);
        };
        let empty = self
            .pairs()
            .iter()
            .any(|&(open, close)| open == before && close == after);
        Ok(if empty { after.len_utf8() } else { 0 })
    }

    /// Types `typed` at every cursor, each as it decided, as one undo step.
    fn edit_typed(&mut self, typed: char, decisions: &[Typed]) -> Result<(), BufferError> {
        let selections: Vec<Selection> = self.view.selections.iter().copied().collect();
        let mut plans = Vec::with_capacity(selections.len());
        for (selection, decision) in selections.iter().zip(decisions) {
            plans.push(self.plan_typed(selection, typed, *decision)?);
        }
        let changes = join_overlapping(
            plans
                .iter()
                .map(|plan| (plan.range.clone(), plan.text.clone()))
                .collect(),
        );

        let previous = self.view.last_edit.take();
        let mut transaction = self.buffer.begin_transaction(self.view.selections.clone());
        // Last first, so the offsets of the changes before it stay right.
        for (range, text) in changes.iter().rev() {
            transaction.apply(&Edit::new(range.clone(), text.as_str()))?;
        }
        let mut items = Vec::with_capacity(plans.len());
        for plan in &plans {
            let end = after_change(&changes, &plan.range);
            let item = match plan.after {
                After::Cursor(leaving) => {
                    Selection::cursor(transaction.buffer().byte_to_position(end - leaving)?)
                }
                After::Inside {
                    selected,
                    closer,
                    backwards,
                } => {
                    let to = end - closer;
                    let from = to - selected;
                    let (from, to) = (
                        transaction.buffer().byte_to_position(from)?,
                        transaction.buffer().byte_to_position(to)?,
                    );
                    if backwards {
                        Selection::new(to, from)
                    } else {
                        Selection::new(from, to)
                    }
                }
            };
            items.push(item);
        }
        let after = Selections::from_items(items, self.view.selections.primary_index());
        transaction.commit(after.clone());
        self.view.selections = after;

        if previous == Some(EditKind::Typing) {
            self.buffer.merge_last_two_steps();
        }
        self.view.last_edit = Some(EditKind::Typing);
        Ok(())
    }

    fn plan_typed(
        &self,
        selection: &Selection,
        typed: char,
        decision: Typed,
    ) -> Result<Planned, BufferError> {
        let range = self.bytes_of(selection)?;
        Ok(match decision {
            Typed::Plain => {
                let text = typed.to_string();
                let range = match self.closer_dedent_for(selection, &text)? {
                    Some(dedent) => dedent.start..range.end,
                    None => range,
                };
                Planned {
                    range,
                    text,
                    after: After::Cursor(0),
                }
            }
            Typed::Pair(close) => Planned {
                range,
                text: format!("{typed}{close}"),
                after: After::Cursor(close.len_utf8()),
            },
            Typed::Wrap(close) => {
                let selected = self.buffer.text_in(range.clone())?;
                Planned {
                    range,
                    after: After::Inside {
                        selected: selected.len(),
                        closer: close.len_utf8(),
                        backwards: selection.head() < selection.anchor(),
                    },
                    text: format!("{typed}{selected}{close}"),
                }
            }
            Typed::StepOver => {
                // Another cursor changes text, so this one is a change too: the closer
                // replaced by itself, which puts the cursor after it.
                let (_, after) = self.chars_around(selection.head())?;
                let closer = after.unwrap_or(typed);
                Planned {
                    range: range.end..range.end + closer.len_utf8(),
                    text: closer.to_string(),
                    after: After::Cursor(0),
                }
            }
        })
    }
}
