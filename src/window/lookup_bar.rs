use crate::{
    context::SharedContext,
    key_controller::{
        InsertKind, WindowControlReponse, WindowsControl, default_controls,
        key_controller::SessionEvent,
    },
    utils::{cursor::Cursor, syntaxer::Syntaxer, text_buffer::TextBuffer},
    window::Window,
};
use anyhow::Result;
use crossterm::event;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Constraint::Length, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use std::path::PathBuf;
use walkdir::WalkDir;

const BOTTOM_HEADER: &str = "[↑↓: Move]  [Enter: Open]  [ESC: Exit window]";

#[derive(Debug, Clone)]
pub struct LookupBar {
    cursor: Cursor,
    matches: usize,
    current_entry: usize,
    entries: Vec<PathBuf>,
    context: SharedContext,
    search_buffer: TextBuffer,
}
impl LookupBar {
    pub fn new(context: SharedContext) -> Self {
        Self {
            search_buffer: TextBuffer::new_single_line(String::new()),
            current_entry: 0,
            entries: vec![],
            matches: 0,
            cursor: Cursor::default(),
            context,
        }
    }

    pub fn scan_files(&mut self) -> Result<()> {
        let matcher = SkimMatcherV2::default();
        self.entries.clear();
        self.matches = 0;

        let dir_walker = self.context.get_file_context(|file_context| {
            WalkDir::new(file_context.base_path.clone())
                .max_depth(3)
                .into_iter()
                .filter_map(|e| e.ok())}
        );

        for entry in dir_walker {
            let path = entry.path().to_string_lossy();
            let max_entries = self.get_showable_entries_count()?;
            let is_match = entry.path().is_file()
                && matcher.fuzzy_match(&path, &self.search_buffer[0]).is_some();

            if is_match {
                self.matches += 1;
            }

            if is_match && self.entries.len() != max_entries {
                self.entries.push(entry.path().to_path_buf());
            }
        }
        self.current_entry = 0;
        Ok(())
    }

    pub fn pick_entry(&mut self) -> Result<Option<PathBuf>> {
        if self.entries.get(self.current_entry).is_some() {
            let path = std::mem::take(&mut self.entries[self.current_entry]);
            return Ok(Some(path));
        }
        Ok(None)
    }

    fn get_showable_entries_count(&self) -> Result<usize> {
        const LINES_NON_SHOWABLE: usize = 10;
        let count = self.context.get_area().height as usize - LINES_NON_SHOWABLE;
        Ok(count)
    }
}
impl Window for LookupBar {
    fn on_insert(&mut self) -> Result<()> {
        self.scan_files()?;
        Ok(())
    }

    fn on_remove(&mut self) -> Result<()> {
        self.scan_files()?;
        Ok(())
    }

    fn draw_ui(&mut self, frame: &mut Frame, header: Block, _syntaxer: &mut Syntaxer) -> Result<()> {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Length(area.height - 1), Length(1)].as_ref())
            .split(area);

        let main_box = Paragraph::new("")
            .style(Style::default().fg(Color::White))
            .block(header);

        frame.render_widget(main_box, chunks[0]);

        let overlay_area = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Min(10),
                    Constraint::Length(1),
                ]
                .as_ref(),
            )
            .split(area);

        let input_block = Block::default().borders(Borders::ALL).title(" Find File ");

        let input_area = overlay_area[0];
        let input_inner = input_block.inner(input_area);
        let input = Paragraph::new(format!("> {}", self.search_buffer[0]))
            .block(input_block)
            .style(Style::default().fg(Color::White));

        frame.render_widget(input, overlay_area[0]);

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                let prefix = if i == self.current_entry {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                ListItem::new(vec![Line::raw(format!(" {}", name))]).style(prefix)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} matches ", self.matches)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">");

        frame.render_widget(list, overlay_area[1]);

        let instructions = Paragraph::new(BOTTOM_HEADER).alignment(Alignment::Center);

        frame.render_widget(instructions, overlay_area[2]);

        let cursor_x = 3 + self.search_buffer.len() as u16; // 3 = "> " + border padding
        let cursor_y = input_inner.y; // Top of input area
        frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, cursor_y));
        Ok(())
    }
}

impl WindowsControl for LookupBar {
    fn move_up(&mut self) -> Result<WindowControlReponse> {
        self.current_entry = self.current_entry.saturating_sub(1);
        Ok(WindowControlReponse::None)
    }

    fn move_down(&mut self) -> Result<WindowControlReponse> {
        let last_index = self.entries.len() - 1;
        self.current_entry = (self.current_entry + 1).min(last_index);
        Ok(WindowControlReponse::None)
    }

    fn move_left(&mut self, amount: u16) -> Result<WindowControlReponse> {
        default_controls::move_left(&mut self.cursor, &self.search_buffer, amount);
        Ok(WindowControlReponse::None)
    }

    fn move_right(&mut self, amount: u16) -> Result<WindowControlReponse> {
        default_controls::move_right(&mut self.cursor, &self.search_buffer, amount);
        Ok(WindowControlReponse::None)
    }

    fn enter(&mut self) -> Result<WindowControlReponse> {
        let path = self.pick_entry()?;
        self.context.set_file_context(|file_context| {
            file_context.file_path = path
        });
        Ok(WindowControlReponse::ToMainWindow)
    }

    fn backspace(&mut self) -> Result<WindowControlReponse> {
        default_controls::remove(&mut self.cursor, &mut self.search_buffer);
        Ok(WindowControlReponse::None)
    }

    fn insert(&mut self, insert: InsertKind) -> Result<WindowControlReponse> {
        default_controls::insert(&mut self.cursor, &mut self.search_buffer, insert);
        Ok(WindowControlReponse::None)
    }

    fn custom_action(&mut self, action: event::Event) -> Result<Option<SessionEvent>> {
        match action {
            _ => Ok(None),
        }
    }
}
