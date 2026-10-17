//! Toggle line comment: comments the lines the cursors are on, or uncomments them
//! when every one of them is commented already.

use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::{BufferError, Edit, Position, Selection, Selections};
use crate::syntax::{Comment, Language};

use super::Editor;
use super::indent::{Caret, touched_lines};

/// What a toggle did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Toggled {
    Done,
    /// The file has no known language, so there is no marker to write.
    NoLanguage,
    /// Only blank lines were touched.
    NothingToToggle,
}

/// Whether a bare cursor exactly at a change goes along with the text after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Affinity {
    /// At the start of a line's code it does.
    WithText,
    /// At the end of a line it stays where it is.
    Stays,
}

/// A change to one line, in columns of the line as it was: at `column`, `removed`
/// characters go and `inserted` come.
#[derive(Debug, Clone, Copy)]
struct Change {
    column: usize,
    removed: usize,
    inserted: usize,
    affinity: Affinity,
}

/// A line a toggle touches, with its text.
struct Row {
    line: usize,
    content: String,
    /// It is part of a block of several lines, not the line of a cursor.
    in_block: bool,
}

impl Row {
    fn is_blank(&self) -> bool {
        self.content.trim().is_empty()
    }
}

/// What a toggle does to one line: byte ranges of the line to replace, and the
/// changes in columns, for moving the cursors.
struct Planned {
    line: usize,
    parts: Vec<Part>,
    changes: Vec<Change>,
}

impl Editor {
    /// `ctrl+/`: the lines every cursor is on or a selection touches get the comment
    /// marker of the file's language, one space after it, at the smallest indentation
    /// among them; when each of them already has it, it is taken away (with the
    /// space). Blank lines inside a block are left out and do not decide which way it
    /// goes; the blank line of a cursor is commented with the rest, or alone. One undo
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
        let rows = self.rows_touched()?;
        let planned = plan(&rows, comment);
        if planned.is_empty() {
            return Ok(Toggled::NothingToToggle);
        }

        let mut edits = Vec::new();
        for line in &planned {
            let start = self.buffer.position_to_byte(Position::new(line.line, 0))?;
            edits.extend(line.parts.iter().map(|(bytes, text)| {
                Edit::new(start + bytes.start..start + bytes.end, text.as_str())
            }));
        }
        let after = self.selections_after(&planned);
        self.edit_lines(edits, after)?;
        Ok(Toggled::Done)
    }

    /// The lines the cursors and selections touch, with their text.
    fn rows_touched(&self) -> Result<Vec<Row>, BufferError> {
        touched_lines(&self.view.selections)
            .into_iter()
            .map(|touched| {
                Ok(Row {
                    line: touched.line,
                    content: self.buffer.line_content(touched.line)?.into_owned(),
                    in_block: touched.in_block,
                })
            })
            .collect()
    }

    /// The selections moved along with what `planned` changes.
    fn selections_after(&self, planned: &[Planned]) -> Selections {
        let items: Vec<Selection> = self
            .view
            .selections
            .iter()
            .map(|selection| {
                let caret = Caret::of(selection);
                let moved = |position: Position| {
                    let changes = planned
                        .binary_search_by_key(&position.line, |line| line.line)
                        .map_or(&[][..], |at| planned[at].changes.as_slice());
                    Position::new(position.line, moved_column(position.column, changes, caret))
                };
                Selection::new(moved(selection.anchor()), moved(selection.head()))
                    .with_desired_column(None)
            })
            .collect();
        Selections::from_items(items, self.view.selections.primary_index())
    }
}

/// What to do to each of `rows` (in line order): comment them, or uncomment them when
/// every line with text is commented. Lines it leaves alone are not in the answer.
fn plan(rows: &[Row], comment: Comment) -> Vec<Planned> {
    let with_text: Vec<&Row> = rows.iter().filter(|row| !row.is_blank()).collect();
    // The blank line of a cursor is commented with the rest; one in a block is not.
    let cursor_blanks = |row: &&Row| row.is_blank() && !row.in_block;
    let uncomment = !with_text.is_empty()
        && with_text
            .iter()
            .all(|row| is_commented(&row.content, comment));
    let indent = with_text
        .iter()
        .map(|row| leading_space(&row.content))
        .min();

    rows.iter()
        .filter(|row| match uncomment {
            true => !row.is_blank(),
            false => !row.is_blank() || cursor_blanks(row),
        })
        .map(|row| {
            let (parts, changes) = match uncomment {
                true => take_away(&row.content, comment),
                false => {
                    // On a blank line the marker goes after what white space it has.
                    let at = indent.map_or_else(
                        || leading_space(&row.content),
                        |indent| indent.min(row.content.len()),
                    );
                    put_on(&row.content, comment, at)
                }
            };
            Planned {
                line: row.line,
                parts,
                changes,
            }
        })
        .collect()
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
                    affinity: Affinity::WithText,
                }],
            )
        }
        Comment::Around(open, close) => {
            let open_text = format!("{open} ");
            let close_text = format!(" {close}");
            let end = content.trim_end().len().max(indent);
            let end_column = content[..end].graphemes(true).count();
            let changes = vec![
                Change {
                    column: indent,
                    removed: 0,
                    inserted: open_text.len(),
                    affinity: Affinity::WithText,
                },
                Change {
                    column: end_column,
                    removed: 0,
                    inserted: close_text.len(),
                    affinity: Affinity::Stays,
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
                    affinity: Affinity::WithText,
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
                        affinity: Affinity::WithText,
                    },
                    Change {
                        column: close_column,
                        removed: close_removed,
                        inserted: 0,
                        affinity: Affinity::Stays,
                    },
                ],
            )
        }
    }
}

/// Where the column `original` of a line is after `changes` (in order).
fn moved_column(original: usize, changes: &[Change], caret: Caret) -> usize {
    let mut delta: isize = 0;
    for change in changes {
        let end = change.column + change.removed;
        let shifts = if change.removed == 0 {
            original > change.column
                || (original == change.column
                    && change.affinity == Affinity::WithText
                    && caret == Caret::Bare)
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
