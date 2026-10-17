//! Toggle line comment: comments the lines the cursors are on, or uncomments them
//! when every one of them is commented already.

use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::{BufferError, Edit, Position, Selection, Selections};
use crate::syntax::{Comment, Language};

use super::Editor;
use super::indent::selected_lines;

/// What a toggle did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Toggled {
    Done,
    /// The file has no known language, so there is no marker to write.
    NoLanguage,
    /// Only blank lines were touched.
    NothingToToggle,
}

/// A change to one line, in columns of the line as it was: at `column`, `removed`
/// characters go and `inserted` come.
#[derive(Debug, Clone, Copy)]
struct Change {
    column: usize,
    removed: usize,
    inserted: usize,
    /// Whether a bare cursor exactly at `column` goes along with the text after
    /// the change (at the start of a line's code it does; at the end it stays).
    along: bool,
}

impl Editor {
    /// `ctrl+/`: the lines every cursor is on or a selection touches get the comment
    /// marker of the file's language, one space after it, at the smallest indentation
    /// among them; when each of them already has it, it is taken away (with the
    /// space). Blank lines are left out and do not decide which way it goes. One undo
    /// step.
    pub(crate) fn toggle_comment(&mut self) -> Result<Toggled, BufferError> {
        let Some(comment) = self
            .buffer
            .path()
            .and_then(Language::from_path)
            .map(Language::comment)
        else {
            return Ok(Toggled::NoLanguage);
        };

        let mut lines: Vec<usize> = self
            .view
            .selections
            .iter()
            .flat_map(selected_lines)
            .collect();
        lines.sort_unstable();
        lines.dedup();
        let mut rows = Vec::new();
        for line in lines {
            let content = self.buffer.line_content(line)?.into_owned();
            if !content.trim().is_empty() {
                rows.push((line, content));
            }
        }
        if rows.is_empty() {
            return Ok(Toggled::NothingToToggle);
        }

        let uncomment = rows
            .iter()
            .all(|(_, content)| is_commented(content, comment));
        let indent = rows
            .iter()
            .map(|(_, content)| leading_space(content))
            .min()
            .unwrap_or(0);

        let mut edits = Vec::new();
        let mut changes: Vec<(usize, Vec<Change>)> = Vec::new();
        for (line, content) in &rows {
            let start = self.buffer.position_to_byte(Position::new(*line, 0))?;
            let (parts, line_changes) = match uncomment {
                true => take_away(content, comment),
                false => put_on(content, comment, indent),
            };
            edits.extend(
                parts
                    .into_iter()
                    .map(|(bytes, text)| Edit::new(start + bytes.start..start + bytes.end, text)),
            );
            changes.push((*line, line_changes));
        }

        let items: Vec<Selection> = self
            .view
            .selections
            .iter()
            .map(|selection| {
                let bare = selection.is_empty();
                let moved = |position: Position| {
                    let on_line = changes
                        .iter()
                        .find(|(line, _)| *line == position.line)
                        .map_or(&[][..], |(_, changes)| changes.as_slice());
                    Position::new(position.line, moved_column(position.column, on_line, bare))
                };
                Selection::new(moved(selection.anchor()), moved(selection.head()))
                    .with_desired_column(None)
            })
            .collect();
        let after = Selections::from_items(items, self.view.selections.primary_index());
        self.edit_lines(edits, after)?;
        Ok(Toggled::Done)
    }
}

/// Characters of white space the line starts with.
fn leading_space(content: &str) -> usize {
    content.len() - content.trim_start_matches([' ', '\t']).len()
}

fn is_commented(content: &str, comment: Comment) -> bool {
    let rest = content.trim_start_matches([' ', '\t']);
    match comment {
        Comment::Line(marker) => rest.starts_with(marker),
        Comment::Around(open, close) => {
            let trimmed = rest.trim_end();
            trimmed.len() >= open.len() + close.len()
                && trimmed.starts_with(open)
                && trimmed.ends_with(close)
        }
    }
}

/// A byte range to replace and what to put there.
type Part = (std::ops::Range<usize>, String);

/// The edits (bytes of the line) and the changes (columns) that comment `content`
/// at `indent`.
fn put_on(content: &str, comment: Comment, indent: usize) -> (Vec<Part>, Vec<Change>) {
    match comment {
        Comment::Line(marker) => {
            let text = format!("{marker} ");
            let inserted = text.len();
            (
                vec![(indent..indent, text)],
                vec![Change {
                    column: indent,
                    removed: 0,
                    inserted,
                    along: true,
                }],
            )
        }
        Comment::Around(open, close) => {
            let open_text = format!("{open} ");
            let close_text = format!(" {close}");
            let end = content.trim_end().len();
            let end_column = content[..end].graphemes(true).count();
            let changes = vec![
                Change {
                    column: indent,
                    removed: 0,
                    inserted: open_text.len(),
                    along: true,
                },
                Change {
                    column: end_column,
                    removed: 0,
                    inserted: close_text.len(),
                    along: false,
                },
            ];
            (
                vec![(indent..indent, open_text), (end..end, close_text)],
                changes,
            )
        }
    }
}

/// The edits and changes that uncomment `content`, which is commented.
fn take_away(content: &str, comment: Comment) -> (Vec<Part>, Vec<Change>) {
    let start = leading_space(content);
    let rest = &content[start..];
    match comment {
        Comment::Line(marker) => {
            let removed = marker.len() + usize::from(rest[marker.len()..].starts_with(' '));
            (
                vec![(start..start + removed, String::new())],
                vec![Change {
                    column: start,
                    removed,
                    inserted: 0,
                    along: true,
                }],
            )
        }
        Comment::Around(open, close) => {
            let open_removed = open.len() + usize::from(rest[open.len()..].starts_with(' '));
            let end = content.trim_end().len();
            let mut close_start = end - close.len();
            // The space before the closing marker goes too, unless the opening took it.
            if close_start > start + open_removed && content[..close_start].ends_with(' ') {
                close_start -= 1;
            }
            let close_removed = end - close_start;
            let close_column = content[..close_start].graphemes(true).count();
            (
                vec![
                    (start..start + open_removed, String::new()),
                    (close_start..end, String::new()),
                ],
                vec![
                    Change {
                        column: start,
                        removed: open_removed,
                        inserted: 0,
                        along: true,
                    },
                    Change {
                        column: close_column,
                        removed: close_removed,
                        inserted: 0,
                        along: false,
                    },
                ],
            )
        }
    }
}

/// Where the column `original` of a line is after `changes` (in order).
fn moved_column(original: usize, changes: &[Change], bare: bool) -> usize {
    let mut delta: isize = 0;
    for change in changes {
        let end = change.column + change.removed;
        let shifts = if change.removed == 0 {
            original > change.column || (original == change.column && change.along && bare)
        } else {
            original >= end
        };
        if shifts {
            delta += change.inserted as isize - change.removed as isize;
        } else if original > change.column && change.removed > 0 {
            // Inside what was taken away: at the place it was taken from.
            return (change.column as isize + delta + change.inserted as isize).max(0) as usize;
        }
    }
    (original as isize + delta).max(0) as usize
}
