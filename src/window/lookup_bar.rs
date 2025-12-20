use anyhow::Result;
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

use crate::{
    context::SharedContext,
    key_controller::{InsertKind, KeyController, KeyDoneKind, default_controls},
    window::{Window, utils::Cursor},
};

const BOTTOM_HEADER: &str = "[↑↓: Move]  [Enter: Open]  [ESC: Exit window]";

#[derive(Debug, Clone)]
pub struct LookupBar {
    cursor: Cursor,
    // type is [String; 1] so that search buffer can be used as &[String] in fucntions
    search_buffer: [String; 1],
    current_entry: usize,
    entries: Vec<PathBuf>,
    matches: usize,
    context: SharedContext,
}
impl LookupBar {
    pub fn new(context: SharedContext) -> Self {
        Self {
            search_buffer: [String::new()],
            current_entry: 0,
            entries: vec![],
            matches: 0,
            cursor: Cursor::default(),
            context,
        }
    }

    pub fn scan_files(&mut self) {
        let matcher = SkimMatcherV2::default();
        self.entries.clear();
        self.matches = 0;

        let base_path = &self.context.borrow().file_context.base_path;

        for entry in WalkDir::new(base_path)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path().to_string_lossy();
            let max_entries = self.get_showable_entries_count();
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
    }

    pub fn pick_entry(&mut self) -> Result<Option<PathBuf>> {
        if self.entries.get(self.current_entry).is_some() {
            let path = std::mem::take(&mut self.entries[self.current_entry]);
            return Ok(Some(path));
        }
        Ok(None)
    }

    fn get_showable_entries_count(&self) -> usize {
        const LINES_NON_SHOWABLE: usize = 10;
        self.context.borrow().screen_area.height as usize - LINES_NON_SHOWABLE
    }
}
impl Window for LookupBar {
    fn on_insert(&mut self) {
        self.scan_files();
    }

    fn on_remove(&mut self) {
        self.scan_files();
    }

    fn draw_ui(&mut self, frame: &mut Frame, header: Block) {
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
    }
}

impl KeyController for LookupBar {
    fn move_up(&mut self) -> Result<KeyDoneKind> {
        self.current_entry = self.current_entry.saturating_sub(1);
        Ok(KeyDoneKind::None)
    }

    fn move_down(&mut self) -> Result<KeyDoneKind> {
        let last_index = self.entries.len() - 1;
        self.current_entry = (self.current_entry + 1).min(last_index);
        Ok(KeyDoneKind::None)
    }

    fn move_left(&mut self, amount: u16) -> Result<KeyDoneKind> {
        default_controls::move_left(&mut self.cursor, &self.search_buffer, amount);
        Ok(KeyDoneKind::None)
    }

    fn move_right(&mut self, amount: u16) -> Result<KeyDoneKind> {
        default_controls::move_right(&mut self.cursor, &self.search_buffer, amount);
        Ok(KeyDoneKind::None)
    }

    fn enter(&mut self) -> Result<KeyDoneKind> {
        let path = self.pick_entry()?;
        self.context.set_file_path(path);
        Ok(KeyDoneKind::ToMainWindow)
    }

    fn backspace(&mut self) -> Result<KeyDoneKind> {
        default_controls::remove_single_line(&mut self.cursor, &mut self.search_buffer[0]);
        Ok(KeyDoneKind::None)
    }

    fn insert(&mut self, insert: InsertKind) -> Result<KeyDoneKind> {
        default_controls::insert_single_line(&mut self.cursor, &mut self.search_buffer[0], insert);
        Ok(KeyDoneKind::None)
    }
}
