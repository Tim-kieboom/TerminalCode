//! Copy, cut and paste on the editor.

use std::borrow::Cow;
use std::ops::Range;

use crate::buffer::{BufferError, LineEnding, Position};
use crate::clipboard::{Register, RegisterKind};

use super::Editor;

impl Editor {
    /// The selected text, or the current line (with its line break) when
    /// nothing is selected.
    pub(crate) fn copy(&self) -> Result<Register, BufferError> {
        let selection = self.view.selections.primary();
        if !selection.is_empty() {
            let text = self.buffer.text_in(self.selection_bytes()?)?;
            return Ok(Register::charwise(text));
        }

        let line = selection.head().line;
        let mut text = self.buffer.line_content(line)?.into_owned();
        text.push_str(self.buffer.line_ending().as_str());
        Ok(Register::linewise(text))
    }

    /// Copies, then deletes the selection, or the current line when nothing is
    /// selected.
    pub(crate) fn cut(&mut self) -> Result<Register, BufferError> {
        let register = self.copy()?;
        let range = if self.view.selections.primary().is_empty() {
            self.line_range(self.view.selections.primary().head().line)?
        } else {
            self.selection_bytes()?
        };
        self.replace(range, "", None)?;
        Ok(register)
    }

    /// Pastes as one undo step. Line breaks in the text are converted to the
    /// buffer's style. Charwise text goes at the cursor (replacing the
    /// selection); linewise text goes above the current line and leaves the
    /// cursor on the same character of the line it was on.
    pub(crate) fn paste(&mut self, register: &Register) -> Result<(), BufferError> {
        let ending = self.buffer.line_ending();
        let selection = *self.view.selections.primary();
        let text = normalize_line_endings(register.text(), ending);

        match register.kind() {
            RegisterKind::Charwise => self.replace(self.selection_bytes()?, &text, None),
            RegisterKind::Linewise if !selection.is_empty() => {
                let text = text.strip_suffix(ending.as_str()).unwrap_or(&text);
                self.replace(self.selection_bytes()?, text, None)
            }
            RegisterKind::Linewise => self.paste_lines_above(&text, ending),
        }
    }

    fn paste_lines_above(&mut self, text: &str, ending: LineEnding) -> Result<(), BufferError> {
        let head = self.view.selections.primary().head();
        let start = self.buffer.position_to_byte(Position::new(head.line, 0))?;
        let mut lines = text.to_owned();
        if !lines.ends_with(ending.as_str()) {
            lines.push_str(ending.as_str());
        }
        let added = lines.matches('\n').count();

        let cursor = Position::new(head.line + added, head.column);
        self.replace_with(start..start, &lines, None, |_, _| Ok(cursor))
    }

    /// Byte range of `line` including its line break. The last line has none,
    /// so the break before it goes instead (unless it is the only line).
    fn line_range(&self, line: usize) -> Result<Range<usize>, BufferError> {
        let start = self.buffer.position_to_byte(Position::new(line, 0))?;
        if line + 1 < self.buffer.len_lines() {
            let next = self.buffer.position_to_byte(Position::new(line + 1, 0))?;
            return Ok(start..next);
        }

        let end = self
            .buffer
            .position_to_byte(Position::new(line, self.buffer.line_len(line)?))?;
        let Some(previous) = line.checked_sub(1) else {
            return Ok(start..end);
        };
        let previous_end = self
            .buffer
            .position_to_byte(Position::new(previous, self.buffer.line_len(previous)?))?;
        Ok(previous_end..end)
    }
}

/// Converts every line break in `text` (`\r\n`, `\r` or `\n`) to `ending`.
fn normalize_line_endings(text: &str, ending: LineEnding) -> Cow<'_, str> {
    if !text.contains(['\r', '\n']) {
        return Cow::Borrowed(text);
    }
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    match ending {
        LineEnding::Lf => Cow::Owned(unified),
        LineEnding::Crlf => Cow::Owned(unified.replace('\n', "\r\n")),
    }
}
