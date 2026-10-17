//! Positions as the protocol counts them: a line, and a character that is a
//! count of UTF-16 code units from the start of the line. The text works in
//! bytes, so this converts, in logarithmic time (the rope keeps the counts).
//!
//! Lines end with `\n` or `\r\n`, which is what the editor and `rust-analyzer`
//! take as line ends (the protocol also names a lone `\r`).

use ropey::Rope;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LspPosition {
    pub(crate) line: u32,
    /// UTF-16 code units from the start of the line.
    pub(crate) character: u32,
}

/// The position of byte `byte` of `rope`. A byte past the end is the end, and
/// one in the middle of a character is the start of that character.
pub(crate) fn to_lsp(rope: &Rope, byte: usize) -> LspPosition {
    let char_index = rope.byte_to_char(byte.min(rope.len_bytes()));
    let line = rope.char_to_line(char_index);
    let line_start = rope.line_to_char(line);
    let units = rope.char_to_utf16_cu(char_index) - rope.char_to_utf16_cu(line_start);
    LspPosition {
        line: clamp(line),
        character: clamp(units),
    }
}

/// The byte of `rope` that `position` names. As the protocol says: a line past
/// the end is the end of the text, a character past the end of its line is the
/// end of the line (before its line break), and one in the middle of a
/// character outside the basic plane is the start of that character.
pub(crate) fn from_lsp(rope: &Rope, position: LspPosition) -> usize {
    let line = position.line as usize;
    if line >= rope.len_lines() {
        return rope.len_bytes();
    }
    let start = rope.line_to_char(line);
    let end = start + content_chars(rope, line);
    let wanted = rope.char_to_utf16_cu(start) + position.character as usize;
    let target = wanted.min(rope.char_to_utf16_cu(end));
    let char_index = rope.utf16_cu_to_char(target).clamp(start, end);
    rope.char_to_byte(char_index)
}

/// How many characters `line` has, without its line break.
fn content_chars(rope: &Rope, line: usize) -> usize {
    let slice = rope.line(line);
    let mut chars = slice.len_chars();
    if chars > 0 && slice.char(chars - 1) == '\n' {
        chars -= 1;
        if chars > 0 && slice.char(chars - 1) == '\r' {
            chars -= 1;
        }
    }
    chars
}

fn clamp(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
