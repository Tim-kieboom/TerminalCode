//! Indentation: detecting a file's style, Tab/Shift+Tab, and Enter's
//! auto-indent.

use std::ops::Range;

use crate::buffer::{Buffer, BufferError, Edit, Position, Selection, Selections};

use super::smart_indent::{Dialect, is_closer};
use super::{EditKind, Editor, Motion, TabWidth};

/// Lines looked at when detecting a file's indentation.
const SAMPLE_LINES: usize = 2000;
/// Indent width used when a file does not show one.
const FALLBACK_WIDTH: usize = 4;
/// Widths recognized as an indent step: 1 is alignment noise, and anything
/// wider than 4 is more likely several levels than one (such files fall back
/// to the default width).
const STEP_WIDTHS: std::ops::RangeInclusive<usize> = 2..=4;

/// What one level of indentation is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndentStyle {
    Tabs,
    Spaces(usize),
}

impl Default for IndentStyle {
    fn default() -> Self {
        Self::Spaces(FALLBACK_WIDTH)
    }
}

impl IndentStyle {
    /// The style the file already uses, or `None` if it has no indentation to
    /// learn from. Lines starting with tabs against lines starting with
    /// spaces decide between the two; for spaces, the most common increase
    /// from one indented line to the next is the width.
    pub(crate) fn detect(buffer: &Buffer) -> Option<Self> {
        let mut tab_lines = 0usize;
        let mut space_lines = 0usize;
        let mut steps = [0usize; 9];
        let mut previous = 0usize;
        let mut first_indent = None;

        for line in 0..buffer.len_lines().min(SAMPLE_LINES) {
            let Ok(content) = buffer.line_content(line) else {
                break;
            };
            if content.trim().is_empty() {
                continue;
            }
            if content.starts_with('\t') {
                tab_lines += 1;
                continue;
            }

            let width = content.chars().take_while(|c| *c == ' ').count();
            if width > 0 {
                space_lines += 1;
                first_indent.get_or_insert(width);
                if width > previous && STEP_WIDTHS.contains(&(width - previous)) {
                    steps[width - previous] += 1;
                }
            }
            previous = width;
        }

        if tab_lines > space_lines {
            return Some(Self::Tabs);
        }
        if space_lines == 0 {
            return None;
        }

        let most_common = STEP_WIDTHS.max_by_key(|width| (steps[*width], usize::MAX - *width));
        match most_common {
            Some(width) if steps[width] > 0 => Some(Self::Spaces(width)),
            _ => first_indent
                .filter(|width| STEP_WIDTHS.contains(width))
                .map(Self::Spaces),
        }
    }

    /// One level of indentation as text.
    fn unit(self) -> String {
        match self {
            Self::Tabs => "\t".to_owned(),
            Self::Spaces(width) => " ".repeat(width),
        }
    }

    /// Columns (graphemes) one level adds.
    fn columns(self) -> usize {
        match self {
            Self::Tabs => 1,
            Self::Spaces(width) => width,
        }
    }

    /// Most spaces Shift+Tab removes from a space-indented line.
    fn outdent_spaces(self) -> usize {
        match self {
            Self::Tabs => FALLBACK_WIDTH,
            Self::Spaces(width) => width,
        }
    }
}

impl Editor {
    /// Tab. With a selection over several lines it indents each of them;
    /// otherwise it inserts one level at the cursor (to the next tab stop for
    /// spaces), replacing the selection.
    pub(crate) fn indent(&mut self) -> Result<(), BufferError> {
        let selection = *self.view.selections.primary();
        if selection.start().line != selection.end().line {
            return self.indent_lines(&selection);
        }

        let text = match self.view.indent {
            IndentStyle::Tabs => "\t".to_owned(),
            IndentStyle::Spaces(width) => " ".repeat(width - selection.start().column % width),
        };
        let kind = selection.is_empty().then_some(EditKind::Typing);
        self.replace(self.selection_bytes()?, &text, kind)
    }

    /// Shift+Tab. Removes one level of indentation from the current line, or
    /// from every selected line.
    pub(crate) fn outdent(&mut self) -> Result<(), BufferError> {
        let selection = *self.view.selections.primary();
        let mut edits = Vec::new();
        let mut removed_per_line = Vec::new();

        for line in selected_lines(&selection) {
            let content = self.buffer.line_content(line)?;
            let removed = if content.starts_with('\t') {
                1
            } else {
                let spaces = content.chars().take_while(|c| *c == ' ').count();
                spaces.min(self.view.indent.outdent_spaces())
            };
            if removed == 0 {
                continue;
            }
            let start = self.buffer.position_to_byte(Position::new(line, 0))?;
            edits.push(Edit::new(start..start + removed, ""));
            removed_per_line.push((line, removed));
        }
        if edits.is_empty() {
            return Ok(());
        }

        let shift = |position: Position| {
            let removed = removed_per_line
                .iter()
                .find(|(line, _)| *line == position.line)
                .map_or(0, |(_, removed)| *removed);
            Position::new(position.line, position.column.saturating_sub(removed))
        };
        let after = remap(&selection, shift);
        self.edit_lines(edits, after)
    }

    /// Enter: a line break followed by the leading whitespace of the current
    /// line, as far as it lies left of the cursor. After an opening bracket
    /// (in a language that has them, see [`Dialect`]) the new line is one level
    /// deeper, and a closing bracket right after the cursor goes to a line of
    /// its own below.
    pub(crate) fn insert_newline_indented(&mut self) -> Result<(), BufferError> {
        let selection = *self.view.selections.primary();
        let start = selection.start();
        let content = self.buffer.line_content(start.line)?;
        let indent: String = content
            .chars()
            .take_while(|c| matches!(c, ' ' | '\t'))
            .take(start.column)
            .collect();
        let line_break = self.buffer.line_ending().as_str();

        let cursor = self.buffer.position_to_byte(start)?;
        let line_start = self.buffer.position_to_byte(Position::new(start.line, 0))?;
        let (before, after) = content.split_at(cursor - line_start);
        let closer =
            Dialect::of_buffer(&self.buffer).and_then(|dialect| dialect.closer_to_add(before));
        let Some(closer) = closer else {
            let text = format!("{line_break}{indent}");
            return self.insert_text(&text);
        };

        let deeper = format!("{indent}{}", self.view.indent.unit());
        if selection.is_empty() && after.starts_with(closer) {
            let text = format!("{line_break}{deeper}{line_break}{indent}");
            self.insert_text(&text)?;
            // The cursor goes to the empty line in the middle.
            self.move_cursor(Motion::Up)?;
            return self.move_cursor(Motion::LineEnd);
        }
        self.insert_text(&format!("{line_break}{deeper}"))
    }

    /// Typing a closing bracket on a line that holds only indentation takes one
    /// level of it away first, so the bracket lines up with what opened it.
    /// `None` when `text` is something else, or there is nothing to take away.
    pub(super) fn closer_dedent(&self, text: &str) -> Result<Option<Range<usize>>, BufferError> {
        let selection = self.view.selections.primary();
        if !selection.is_empty() || !is_closer(text) || Dialect::of_buffer(&self.buffer).is_none() {
            return Ok(None);
        }
        let at = selection.head();
        let content = self.buffer.line_content(at.line)?;
        let cursor = self.buffer.position_to_byte(at)?;
        let line_start = self.buffer.position_to_byte(Position::new(at.line, 0))?;
        let before = &content[..cursor - line_start];
        if before.is_empty() || !before.chars().all(|c| matches!(c, ' ' | '\t')) {
            return Ok(None);
        }

        let take = match self.view.indent {
            IndentStyle::Tabs => before.ends_with('\t').then_some(1),
            IndentStyle::Spaces(width) => {
                let spaces = before.len() - before.trim_end_matches(' ').len();
                let remainder = spaces % width;
                let take = if remainder == 0 { width } else { remainder };
                (spaces >= take).then_some(take)
            }
        };
        Ok(take.map(|take| cursor - take..cursor))
    }

    fn indent_lines(&mut self, selection: &Selection) -> Result<(), BufferError> {
        let unit = self.view.indent.unit();
        let mut edits = Vec::new();
        let mut indented = Vec::new();

        for line in selected_lines(selection) {
            if self.buffer.line_len(line)? == 0 {
                continue;
            }
            let start = self.buffer.position_to_byte(Position::new(line, 0))?;
            edits.push(Edit::new(start..start, unit.as_str()));
            indented.push(line);
        }
        if edits.is_empty() {
            return Ok(());
        }

        // A position at column 0 stays there, so the selection also covers
        // the indentation just added.
        let columns = self.view.indent.columns();
        let shift = |position: Position| {
            if position.column == 0 || !indented.contains(&position.line) {
                return position;
            }
            Position::new(position.line, position.column + columns)
        };
        let after = remap(selection, shift);
        self.edit_lines(edits, after)
    }

    /// Applies edits to separate lines as one undo step. `edits` must be in
    /// text order; they are applied last first so earlier offsets stay valid.
    fn edit_lines(&mut self, edits: Vec<Edit>, after: Selections) -> Result<(), BufferError> {
        self.view.last_edit = None;
        let mut transaction = self.buffer.begin_transaction(self.view.selections.clone());
        for edit in edits.iter().rev() {
            transaction.apply(edit)?;
        }
        transaction.commit(after.clone());
        self.view.selections = after;
        Ok(())
    }
}

/// Lines a block operation covers: those the selection touches, except the
/// last when the selection ends at its very start.
fn selected_lines(selection: &Selection) -> std::ops::RangeInclusive<usize> {
    let (start, end) = (selection.start(), selection.end());
    let last = if end.line > start.line && end.column == 0 {
        end.line - 1
    } else {
        end.line
    };
    start.line..=last
}

/// The selection with both ends moved by `shift`.
fn remap(selection: &Selection, shift: impl Fn(Position) -> Position) -> Selections {
    let moved = Selection::new(shift(selection.anchor()), shift(selection.head()))
        .with_desired_column(None);
    Selections::single(moved)
}

/// What one level of indentation is made of when a file shows no style of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum IndentKind {
    Spaces,
    Tabs,
}

/// What the settings say about indentation: the style to use when a file shows
/// none, how many columns a level (and a tab character) is, and whether the
/// style a file already has wins over all that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IndentPrefs {
    pub(crate) kind: IndentKind,
    pub(crate) size: TabWidth,
    pub(crate) detect: bool,
}

impl Default for IndentPrefs {
    fn default() -> Self {
        Self {
            kind: IndentKind::Spaces,
            size: TabWidth::default(),
            detect: true,
        }
    }
}

impl IndentPrefs {
    /// The indentation for a file: its own style if detection is on and it has
    /// one, otherwise the one the settings name.
    pub(crate) fn style_for(&self, buffer: &Buffer) -> IndentStyle {
        let detected = self.detect.then(|| IndentStyle::detect(buffer)).flatten();
        detected.unwrap_or(match self.kind {
            IndentKind::Spaces => IndentStyle::Spaces(self.size.get()),
            IndentKind::Tabs => IndentStyle::Tabs,
        })
    }
}
