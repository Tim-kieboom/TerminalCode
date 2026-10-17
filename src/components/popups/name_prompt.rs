//! A one-line prompt for the name of a new file or folder.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::files::entries::EntryKind;
use crate::ui::theme::Theme;

const WIDTH: u16 = 60;
const PROMPT: &str = "> ";

/// What a key press asks the prompt to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Insert(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    WordLeft,
    WordRight,
    ClearName,
    Submit,
    Cancel,
}

impl Command {
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        if key.kind == KeyEventKind::Release || key.modifiers.contains(KeyModifiers::ALT) {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        Some(match (key.code, ctrl) {
            (KeyCode::Esc, _) => Self::Cancel,
            (KeyCode::Enter, _) => Self::Submit,
            (KeyCode::Backspace, _) => Self::Backspace,
            (KeyCode::Delete, _) => Self::Delete,
            (KeyCode::Left, true) => Self::WordLeft,
            (KeyCode::Right, true) => Self::WordRight,
            (KeyCode::Left, false) => Self::Left,
            (KeyCode::Right, false) => Self::Right,
            (KeyCode::Home, _) => Self::Home,
            (KeyCode::End, _) => Self::End,
            (KeyCode::Char('u'), true) => Self::ClearName,
            (KeyCode::Char(c), false) => Self::Insert(c),
            _ => return None,
        })
    }
}

/// What the name is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Purpose {
    Create(EntryKind),
    /// Renames this entry.
    Rename(PathBuf),
    /// Moves this entry to a path from the project root.
    Move(PathBuf),
    /// A line, and perhaps a column, of the open file, which has this many lines.
    GoToLine {
        lines: usize,
    },
    /// A path of a file to open as a tab.
    OpenFile,
    /// A path of a folder to open as the project, by starting again in it.
    OpenFolder,
    /// The name of a new theme that starts as a copy of the template with this name.
    NewTheme {
        template: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamePrompt {
    purpose: Purpose,
    /// The folder the new entry is made in.
    dir: PathBuf,
    name: String,
    /// Where typing goes: a byte offset in `name`, always between two graphemes.
    cursor: usize,
    /// The first byte of `name` that is shown, when the name is longer than the box.
    scroll: usize,
    /// Where the box is on the screen, set by [`NamePrompt::place`] before each draw.
    layout: Layout,
}

/// Where the box and its text are on the screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Layout {
    area: Rect,
    /// The cells the text can use, on one row.
    text_x: u16,
    text_y: u16,
    text_width: u16,
}

impl NamePrompt {
    fn with(purpose: Purpose, dir: PathBuf, name: String) -> Self {
        Self {
            purpose,
            dir,
            cursor: name.len(),
            name,
            scroll: 0,
            layout: Layout::default(),
        }
    }

    /// Asks for the name of a new entry in `dir`.
    pub(crate) fn new(kind: EntryKind, dir: PathBuf) -> Self {
        Self::with(Purpose::Create(kind), dir, String::new())
    }

    /// Asks for the new name of `from`, starting from its current one.
    pub(crate) fn rename(from: PathBuf) -> Self {
        let dir = from.parent().map(Path::to_path_buf).unwrap_or_default();
        let name = from
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        Self::with(Purpose::Rename(from), dir, name)
    }

    /// Asks for the new place of `from`, as a path from `root`, starting from
    /// the current one.
    pub(crate) fn move_from(from: PathBuf, root: PathBuf) -> Self {
        let name = from
            .strip_prefix(&root)
            .unwrap_or(&from)
            .to_string_lossy()
            .into_owned();
        Self::with(Purpose::Move(from), root, name)
    }

    /// Asks for the name of a new theme, a copy of `template`.
    pub(crate) fn new_theme(template: String) -> Self {
        Self::with(
            Purpose::NewTheme { template },
            PathBuf::new(),
            String::new(),
        )
    }

    /// Asks for the path of a file to open; a relative one is from `root`.
    pub(crate) fn open_file(root: PathBuf) -> Self {
        Self::with(Purpose::OpenFile, root, String::new())
    }

    /// Asks for the path of a folder to open as the project; a relative one is
    /// from `root`.
    pub(crate) fn open_folder(root: PathBuf) -> Self {
        Self::with(Purpose::OpenFolder, root, String::new())
    }

    /// Asks for a line (`42`) or a line and column (`42:7`) of a file with `lines`
    /// lines.
    pub(crate) fn go_to_line(lines: usize) -> Self {
        Self::with(Purpose::GoToLine { lines }, PathBuf::new(), String::new())
    }

    pub(crate) fn purpose(&self) -> &Purpose {
        &self.purpose
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Where the text cursor is: a byte offset in the name.
    #[cfg(test)]
    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    /// Types `c` at the cursor.
    pub(crate) fn insert(&mut self, c: char) {
        self.name.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Deletes the character (grapheme) before the cursor.
    pub(crate) fn backspace(&mut self) {
        let start = self.previous_boundary(self.cursor);
        self.name.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    /// Deletes the character (grapheme) at the cursor.
    pub(crate) fn delete(&mut self) {
        let end = self.next_boundary(self.cursor);
        self.name.replace_range(self.cursor..end, "");
    }

    pub(crate) fn clear(&mut self) {
        self.name.clear();
        self.cursor = 0;
    }

    pub(crate) fn move_left(&mut self) {
        self.cursor = self.previous_boundary(self.cursor);
    }

    pub(crate) fn move_right(&mut self) {
        self.cursor = self.next_boundary(self.cursor);
    }

    pub(crate) fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub(crate) fn move_end(&mut self) {
        self.cursor = self.name.len();
    }

    /// To the start of the word before the cursor: over what is not a word, then over
    /// the word.
    pub(crate) fn move_word_left(&mut self) {
        let mut at = self.cursor;
        while at > 0 && !self.is_word_before(at) {
            at = self.previous_boundary(at);
        }
        while at > 0 && self.is_word_before(at) {
            at = self.previous_boundary(at);
        }
        self.cursor = at;
    }

    /// To the end of the word after the cursor: over what is not a word, then over the
    /// word.
    pub(crate) fn move_word_right(&mut self) {
        let mut at = self.cursor;
        while at < self.name.len() && !self.is_word_at(at) {
            at = self.next_boundary(at);
        }
        while at < self.name.len() && self.is_word_at(at) {
            at = self.next_boundary(at);
        }
        self.cursor = at;
    }

    /// Puts the cursor at the character under the cell (`column`, `row`) when that is
    /// on the text. Returns whether it was.
    pub(crate) fn click(&mut self, column: u16, row: u16) -> bool {
        let layout = self.layout;
        if row != layout.text_y
            || column < layout.text_x
            || column >= layout.text_x.saturating_add(layout.text_width)
        {
            return false;
        }
        let wanted = usize::from(column - layout.text_x);
        let mut used = 0;
        let mut at = self.floor_to_boundary(self.scroll);
        for grapheme in self.name[at..].graphemes(true) {
            let width = grapheme.width().max(1);
            if wanted < used + width {
                // The nearer edge of the character: the left half of a wide one is before it.
                if wanted - used >= width.div_ceil(2) {
                    at += grapheme.len();
                }
                break;
            }
            used += width;
            at += grapheme.len();
        }
        self.cursor = at;
        true
    }

    fn previous_boundary(&self, from: usize) -> usize {
        self.name[..from]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next_boundary(&self, from: usize) -> usize {
        self.name[from..]
            .graphemes(true)
            .next()
            .map_or(from, |grapheme| from + grapheme.len())
    }

    fn is_word_before(&self, at: usize) -> bool {
        self.name[..at].chars().next_back().is_some_and(is_word)
    }

    fn is_word_at(&self, at: usize) -> bool {
        self.name[at..].chars().next().is_some_and(is_word)
    }

    /// The nearest grapheme boundary of the name at or before `at`. A byte offset kept
    /// from an earlier frame may fall inside a character once the text was edited.
    fn floor_to_boundary(&self, at: usize) -> usize {
        if at >= self.name.len() {
            return self.name.len();
        }
        self.name
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .take_while(|index| *index <= at)
            .last()
            .unwrap_or(0)
    }

    /// Finds where the box goes on `screen` and which part of a long name is shown, so
    /// that the cursor is in view. Call it before each draw.
    pub(crate) fn place(&mut self, screen: Rect) {
        let width = WIDTH.min(screen.width);
        let height = 3.min(screen.height);
        let area = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height / 8).min(screen.height - height),
            width,
            height,
        };
        let inner = Block::bordered().inner(area);
        let room = usize::from(inner.width).saturating_sub(PROMPT.width() + 1);
        self.scroll = self.first_shown(room);
        let text_x = inner.x + PROMPT.width().min(usize::from(inner.width)) as u16;
        self.layout = Layout {
            area,
            text_x,
            text_y: inner.y,
            text_width: inner.right().saturating_sub(text_x),
        };
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let area = self.layout.area;
        let folder = self.dir.file_name().map_or_else(
            || self.dir.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let title = match &self.purpose {
            Purpose::Create(EntryKind::File) => format!(" New file in {folder} "),
            Purpose::Create(EntryKind::Folder) => format!(" New folder in {folder} "),
            Purpose::Rename(_) => format!(" Rename in {folder} "),
            Purpose::Move(_) => format!(" Move within {folder} "),
            Purpose::GoToLine { lines } => format!(" Go to line (1-{lines}, or line:column) "),
            Purpose::NewTheme { template } => format!(" New theme from {template}: name "),
            Purpose::OpenFile => " Open file (a path, ~ is your home folder) ".to_owned(),
            Purpose::OpenFolder => {
                " Open folder (a path; starts the editor again there) ".to_owned()
            }
        };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(title, theme.style("pane.title")));
        let inner = block.inner(area);
        crate::ui::backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);
        if inner.height == 0 {
            return;
        }

        let room = usize::from(inner.width).saturating_sub(PROMPT.width() + 1);
        let from = self.floor_to_boundary(self.scroll).min(self.cursor);
        let shown = head_fitting(&self.name[from..], room);
        let line = format!("{PROMPT}{shown}");
        frame.render_widget(Paragraph::new(line.as_str()), Rect { height: 1, ..inner });
        let before_cursor = self.name[from..self.cursor].width();
        let cursor_x = self
            .layout
            .text_x
            .saturating_add(before_cursor.min(usize::from(u16::MAX)) as u16);
        frame.set_cursor_position(Position::new(cursor_x.min(inner.right()), inner.y));
    }
}

impl NamePrompt {
    /// The first byte of the name that is drawn: the start when the name fits, and
    /// otherwise as far along as the cursor needs to be inside `room` cells; it stays
    /// where it was while the cursor is in view.
    fn first_shown(&self, room: usize) -> usize {
        let mut from = self.floor_to_boundary(self.scroll.min(self.cursor));
        while from < self.cursor && self.name[from..self.cursor].width() > room {
            from = self.next_boundary(from);
        }
        // Nothing needs hiding when the whole name fits.
        if self.name.width() <= room { 0 } else { from }
    }
}

/// The start of `text` that fits in `width` cells.
fn head_fitting(text: &str, width: usize) -> &str {
    let mut used = 0;
    for (index, c) in text.char_indices() {
        used += c.width().unwrap_or(0);
        if used > width {
            return &text[..index];
        }
    }
    text
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A line and an optional column from what was typed: `42`, `42:7`, `:7` is
/// not one. Both count from 1; a 0 is taken as 1.
pub(crate) fn parse_place(text: &str) -> Option<(usize, Option<usize>)> {
    let number = |part: &str| -> Option<usize> {
        let part = part.trim();
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse::<usize>().ok().map(|n| n.max(1))
    };
    match text.split_once(':') {
        None => Some((number(text)?, None)),
        Some((line, column)) => Some((number(line)?, Some(number(column)?))),
    }
}

#[cfg(test)]
mod tests;
