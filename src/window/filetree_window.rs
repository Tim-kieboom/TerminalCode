use crate::{
    context::SharedContext,
    key_controller::{
        InsertKind, WindowControlReponse, WindowsControl, key_controller::SessionEvent,
    },
    utils::{path_display::display_path, syntaxer::Syntaxer},
    window::Window,
};
use anyhow::{Error, Result};
use crossterm::event::{self, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const BOTTOM_HEADER: &str =
    "[↑↓: Move]  [b: Set as BasePath]  [Enter: Open]  [Backspace: Back]  [ESC: Exit window]";

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
        let path = context.get_file_context(|file_context| file_context.base_path.clone());
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
            let entry_path = entry.path().to_path_buf();
            if path == &entry_path {
                continue;
            }

            let is_dir = entry.file_type().is_dir();
            entries.push(FileEntry {
                path: entry_path,
                is_dir,
            });
        }
    }

    fn change_base_path_to_current(&mut self) -> Result<()> {
        let entry = match self.clone_pick() {
            Some(val) => val,
            None => {
                return Err(Error::msg(
                    "tried to change project path to current entry but no entry found",
                ));
            }
        };

        if !entry.is_dir {
            return Err(Error::msg(
                "tried to change project path to current entry but entry is file and not folder",
            ));
        }

        self.context.set_file_context(move |file_context| {
            file_context.base_path = entry.path
        });
        Ok(())
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

    fn clone_pick(&mut self) -> Option<FileEntry> {
        self.entries
            .get_mut(self.current_entry)
            .map(|el| el.clone())
    }

    fn consume_pick(&mut self) -> Option<FileEntry> {
        self.entries
            .get_mut(self.current_entry)
            .map(|el| std::mem::take(el))
    }
}

impl Window for FileTreeWindow {
    fn on_insert(&mut self) -> Result<()> {Ok(())}
    fn on_remove(&mut self) -> Result<()> {Ok(())}
    fn draw_ui(&mut self, frame: &mut Frame, header: Block, _syntaxer: &mut Syntaxer) -> Result<()> {
        let area = frame.area();
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)].as_ref())
            .split(area);

        frame.render_widget(
            Paragraph::new("").style(Style::default()).block(header),
            layout[0],
        );

        let max_path_len = (frame.area().width / 2) as usize;

        let list = List::new(self.render_lines())
            .block(Block::default().borders(Borders::ALL).title(format!(
                " File Tree {} ",
                display_path(&self.current_path, max_path_len)
            )))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );
        frame.render_widget(list, layout[0]);

        let help = Paragraph::new(BOTTOM_HEADER).alignment(Alignment::Center);
        frame.render_widget(help, layout[1]);
        Ok(())
    }
}

const RESPONSE_NONE: Result<WindowControlReponse> = Ok(WindowControlReponse::None);
impl WindowsControl for FileTreeWindow {
    fn move_up(&mut self) -> Result<WindowControlReponse> {
        self.current_entry = self.current_entry.saturating_sub(1);
        RESPONSE_NONE
    }

    fn move_down(&mut self) -> Result<WindowControlReponse> {
        let last = self.entries.len().saturating_sub(1);
        self.current_entry = (self.current_entry + 1).min(last);
        RESPONSE_NONE
    }

    fn move_right(&mut self, _a: u16) -> Result<WindowControlReponse> {
        RESPONSE_NONE
    }

    fn move_left(&mut self, _a: u16) -> Result<WindowControlReponse> {
        RESPONSE_NONE
    }

    fn enter(&mut self) -> Result<WindowControlReponse> {
        let entry = match self.consume_pick() {
            Some(val) => val,
            None => return RESPONSE_NONE,
        };

        if entry.is_dir {
            self.current_path = entry.path;
            Self::build_tree(&self.current_path, &mut self.entries);
            return RESPONSE_NONE;
        }

        let path = Some(entry.path);
        self.context.set_file_context(|file_context|{
            file_context.file_path = path
        });
        Ok(WindowControlReponse::ToMainWindow)
    }

    fn insert(&mut self, _insert: InsertKind) -> Result<WindowControlReponse> {
        RESPONSE_NONE
    }

    fn backspace(&mut self) -> Result<WindowControlReponse> {
        self.go_back();
        RESPONSE_NONE
    }

    fn custom_action(&mut self, action: event::Event) -> Result<Option<SessionEvent>> {
        let key = match action {
            event::Event::Key(val) => val,
            _ => return Ok(None),
        };

        match key.code {
            event::KeyCode::Char('n') if key.modifiers == KeyModifiers::NONE => {
                Ok(Some(SessionEvent::OpenFileCreater{in_path: self.current_path.clone()}))
            }
            event::KeyCode::Char('b') if key.modifiers == KeyModifiers::NONE => {
                self.change_base_path_to_current()?;
                Ok(Some(SessionEvent::ToMainWindow))
            }
            _ => Ok(None),
        }
    }
}
