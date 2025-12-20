use anyhow::Result;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::{
    context::SharedContext,
    key_controller::{InsertKind, KeyController, KeyDoneKind},
    window::Window,
};

const BOTTOM_HEADER: &str = "[↑↓: Move]  [ctr+alt+p: Set as BasePath]  [Enter: Open]  [Backspace: Back]  [ESC: Exit window]";

#[derive(Debug, Clone, Default)]
struct FileEntry {
    path: PathBuf,
    is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct FileTreeWindow {
    current_path: PathBuf,
    entries: Vec<FileEntry>,
    current_entry: usize,
    context: SharedContext,
}

impl FileTreeWindow {
    pub fn new(context: SharedContext) -> Self {
        let path = context.borrow().file_context.base_path.clone();
        let mut this = Self {
            entries: vec![],
            current_entry: 0,
            current_path: path,
            context,
        };

        Self::build_tree(&this.current_path, &mut this.entries);
        this
    }

    fn build_tree(path: &Path, entries: &mut Vec<FileEntry>) {
        entries.clear();
        let file_walker = WalkDir::new(path)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok());

        for entry in file_walker {
            let path = entry.path().to_path_buf();
            let is_dir = entry.file_type().is_dir();
            entries.push(FileEntry { path, is_dir });
        }
    }

    fn go_back(&mut self) {
        self.current_path.pop();
        Self::build_tree(&self.current_path, &mut self.entries);
    }

    fn render_lines(&self) -> Vec<ListItem<'_>> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let name = entry.path.file_name().unwrap_or_default().to_string_lossy();

                let icon = if entry.is_dir { "📁" } else { "📄" };

                let text = format!("{icon} {name}");
                let style = if self.current_entry == i {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                ListItem::new(Line::from(text)).style(style)
            })
            .collect()
    }

    fn consume_pick(&mut self) -> Option<FileEntry> {
        self.entries
            .get_mut(self.current_entry)
            .map(|el| std::mem::take(el))
    }
}

impl Window for FileTreeWindow {
    fn on_insert(&mut self) {}
    fn on_remove(&mut self) {}
    fn draw_ui(&mut self, frame: &mut Frame, header: Block) {
        let area = frame.area();
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)].as_ref())
            .split(area);

        frame.render_widget(
            Paragraph::new("").style(Style::default()).block(header),
            layout[0],
        );

        let list = List::new(self.render_lines())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" File Tree {} ", self.current_path.display())),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );
        frame.render_widget(list, layout[0]);

        let help = Paragraph::new(BOTTOM_HEADER).alignment(Alignment::Center);
        frame.render_widget(help, layout[1]);
    }
}

impl KeyController for FileTreeWindow {
    fn move_up(&mut self) -> Result<KeyDoneKind> {
        self.current_entry = self.current_entry.saturating_sub(1);
        Ok(KeyDoneKind::None)
    }

    fn move_down(&mut self) -> Result<KeyDoneKind> {
        let last = self.entries.len().saturating_sub(1);
        self.current_entry = (self.current_entry + 1).min(last);
        Ok(KeyDoneKind::None)
    }

    fn move_right(&mut self, _a: u16) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }

    fn move_left(&mut self, _a: u16) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }

    fn enter(&mut self) -> Result<KeyDoneKind> {
        let entry = match self.consume_pick() {
            Some(val) => val,
            None => return Ok(KeyDoneKind::None),
        };

        if entry.is_dir {
            self.current_path = entry.path;
            Self::build_tree(&self.current_path, &mut self.entries);
            return Ok(KeyDoneKind::None);
        }

        let path = Some(entry.path);
        self.context.set_file_path(path);
        Ok(KeyDoneKind::ToMainWindow)
    }

    fn insert(&mut self, _insert: InsertKind) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }

    fn backspace(&mut self) -> Result<KeyDoneKind> {
        self.go_back();
        Ok(KeyDoneKind::None)
    }
}
