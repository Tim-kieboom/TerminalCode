//! A one-line prompt for the name of a new file or folder.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::entries::EntryKind;
use crate::ui::theme::Theme;

const WIDTH: u16 = 60;
const PROMPT: &str = "> ";

/// What a key press asks the prompt to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Insert(char),
    Backspace,
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
            (KeyCode::Char('u'), true) => Self::ClearName,
            (KeyCode::Char(c), false) => Self::Insert(c),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamePrompt {
    kind: EntryKind,
    /// The folder the new entry is made in.
    dir: PathBuf,
    name: String,
}

impl NamePrompt {
    pub(crate) fn new(kind: EntryKind, dir: PathBuf) -> Self {
        Self {
            kind,
            dir,
            name: String::new(),
        }
    }

    pub(crate) fn kind(&self) -> EntryKind {
        self.kind
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn insert(&mut self, c: char) {
        self.name.push(c);
    }

    pub(crate) fn backspace(&mut self) {
        self.name.pop();
    }

    pub(crate) fn clear(&mut self) {
        self.name.clear();
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        let width = WIDTH.min(screen.width);
        let height = 3.min(screen.height);
        let area = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height / 8).min(screen.height - height),
            width,
            height,
        };
        let what = match self.kind {
            EntryKind::File => "New file",
            EntryKind::Folder => "New folder",
        };
        let folder = self.dir.file_name().map_or_else(
            || self.dir.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(
                format!(" {what} in {folder} "),
                theme.style("pane.title"),
            ));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        if inner.height == 0 {
            return;
        }

        let room = usize::from(inner.width).saturating_sub(PROMPT.width() + 1);
        let shown = tail_fitting(&self.name, room);
        let line = format!("{PROMPT}{shown}");
        frame.render_widget(Paragraph::new(line.as_str()), Rect { height: 1, ..inner });
        let cursor_x = inner.x + line.width().min(usize::from(inner.width)) as u16;
        frame.set_cursor_position(Position::new(cursor_x.min(inner.right()), inner.y));
    }
}

/// The end of `text` that fits in `width` cells, so what is being typed stays
/// visible.
fn tail_fitting(text: &str, width: usize) -> &str {
    let mut used = 0;
    let mut start = text.len();
    for (index, c) in text.char_indices().rev() {
        used += c.width().unwrap_or(0);
        if used > width {
            break;
        }
        start = index;
    }
    &text[start..]
}
