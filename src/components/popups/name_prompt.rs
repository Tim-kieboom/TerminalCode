//! A one-line prompt for the name of a new file or folder.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::components::popups::key_with_ctrl;
use crate::components::text_field::{Caret, TextField};
use crate::files::entries::EntryKind;
use crate::ui::theme::Theme;

const WIDTH: u16 = 60;
const PROMPT: &str = "> ";

mod picker;

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
    /// `tab`: completes the name from the list.
    Complete,
    Up,
    Down,
    /// `ctrl+enter`: what is typed is what is meant, whatever the list has highlighted.
    SubmitTyped,
    Submit,
    Cancel,
}

impl Command {
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        let (code, ctrl) = key_with_ctrl(key)?;
        Some(match (code, ctrl) {
            (KeyCode::Esc, _) => Self::Cancel,
            (KeyCode::Enter, true) => Self::SubmitTyped,
            (KeyCode::Enter, false) => Self::Submit,
            (KeyCode::Tab, _) => Self::Complete,
            (KeyCode::Up, _) => Self::Up,
            (KeyCode::Down, _) => Self::Down,
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
    /// The new name of the symbol that is called `old`.
    RenameSymbol {
        old: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamePrompt {
    purpose: Purpose,
    /// The folder the new entry is made in.
    dir: PathBuf,
    field: TextField,
    /// Where the box is on the screen, set by [`NamePrompt::place`] before each draw.
    layout: Layout,
    /// The list of the folder under the text, for the prompts that ask for a path.
    picker: Option<picker::Picker>,
}

/// Where the box and its text are on the screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Layout {
    area: Rect,
    /// Where the text starts, on one row (the field itself knows how far it reaches).
    text_x: u16,
    text_y: u16,
    /// The rows of the list under the text.
    list: Rect,
}

impl NamePrompt {
    fn with(purpose: Purpose, dir: PathBuf, name: String) -> Self {
        Self {
            picker: Self::picker_for(&purpose),
            purpose,
            dir,
            field: TextField::with_text(name),
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
        let mut prompt = Self::with(Purpose::Move(from), root, name.clone());
        prompt.leave_unfiltered_while(&name);
        prompt
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

    /// Asks for the new name of the symbol called `old`, starting from `start`.
    pub(crate) fn rename_symbol(old: String, start: String) -> Self {
        Self::with(Purpose::RenameSymbol { old }, PathBuf::new(), start)
    }

    pub(crate) fn purpose(&self) -> &Purpose {
        &self.purpose
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn name(&self) -> &str {
        self.field.text()
    }

    /// Where the text cursor is: a byte offset in the name.
    #[cfg(test)]
    pub(crate) fn cursor(&self) -> usize {
        self.field.cursor()
    }

    /// The first byte of the name that was drawn last.
    #[cfg(test)]
    pub(crate) fn scroll(&self) -> usize {
        self.field.scroll()
    }

    /// Types `c` at the cursor.
    pub(crate) fn insert(&mut self, c: char) {
        self.field.insert(c);
    }

    /// Deletes the character (grapheme) before the cursor.
    pub(crate) fn backspace(&mut self) {
        // After a folder's separator the name of the folder goes with it: one step up.
        if self.step_up() {
            return;
        }
        self.field.backspace();
    }

    /// Deletes the character (grapheme) at the cursor.
    pub(crate) fn delete(&mut self) {
        self.field.delete();
    }

    pub(crate) fn clear(&mut self) {
        self.field.clear();
    }

    pub(crate) fn move_left(&mut self) {
        self.field.move_left();
    }

    pub(crate) fn move_right(&mut self) {
        self.field.move_right();
    }

    pub(crate) fn move_home(&mut self) {
        self.field.move_caret(Caret::Home);
    }

    pub(crate) fn move_end(&mut self) {
        self.field.move_caret(Caret::End);
    }

    pub(crate) fn move_word_left(&mut self) {
        self.field.move_word_left();
    }

    pub(crate) fn move_word_right(&mut self) {
        self.field.move_word_right();
    }

    /// Puts the cursor at the character under the cell (`column`, `row`) when that is
    /// on the text. Returns whether it was.
    pub(crate) fn click(&mut self, column: u16, row: u16) -> bool {
        self.field.click_at(column, row)
    }

    /// Finds where the box goes on `screen` and which part of a long name is shown, so
    /// that the cursor is in view. Call it before each draw.
    pub(crate) fn place(&mut self, screen: Rect) {
        let width = WIDTH.min(screen.width);
        let height = (3 + self.list_height()).min(screen.height);
        let area = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height / 8).min(screen.height - height),
            width,
            height,
        };
        let inner = Block::bordered().inner(area);
        let room = usize::from(inner.width).saturating_sub(PROMPT.width() + 1);
        self.field.visible(room);
        let text_x = inner.x + PROMPT.width().min(usize::from(inner.width)) as u16;
        self.field.set_drawn(Rect {
            x: text_x,
            y: inner.y,
            width: inner.right().saturating_sub(text_x),
            height: 1,
        });
        self.layout = Layout {
            area,
            text_x,
            text_y: inner.y,
            list: Rect {
                y: inner.y + 1,
                height: inner.height.saturating_sub(1),
                ..inner
            },
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
            Purpose::RenameSymbol { old } => format!(" Rename {old} to "),
            Purpose::OpenFile => {
                " Open file (a path; tab completes, ~ is your home folder) ".to_owned()
            }
            Purpose::OpenFolder => {
                " Open folder (a path; enter twice, or ctrl+enter, starts the editor again there) "
                    .to_owned()
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
        let (shown, before_cursor) = self.field.visible(room);
        let line = format!("{PROMPT}{shown}");
        frame.render_widget(Paragraph::new(line.as_str()), Rect { height: 1, ..inner });
        let cursor_x = self
            .layout
            .text_x
            .saturating_add(before_cursor.min(usize::from(u16::MAX)) as u16);
        frame.set_cursor_position(Position::new(cursor_x.min(inner.right()), inner.y));
        self.render_list(frame, theme, self.layout.list);
    }
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
