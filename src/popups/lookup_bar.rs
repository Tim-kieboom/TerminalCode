use anyhow::Result;
use ratatui::{Frame, layout::{Alignment, Constraint, Direction, Layout}, style::{Color, Modifier, Style}, text::{Line, Span, Text}, widgets::{Block, Borders, List, ListItem, Paragraph}};
use walkdir::WalkDir;
use std::path::PathBuf;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};

use crate::text_editor::TextEditor;

#[derive(Debug, Clone, Default)]
pub struct LookupBar {
    pub search_buffer: String,
    pub selected_result: usize,
    pub search_results: Vec<PathBuf>,
}
impl LookupBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scan_files(&mut self, base_path: &PathBuf) {
        let matcher = SkimMatcherV2::default();
        self.search_results.clear();
        
        for entry in WalkDir::new(base_path).max_depth(3).into_iter().filter_map(|e| e.ok()) {
            
            let path = entry.path().to_string_lossy();
            if entry.path().is_file() && matcher.fuzzy_match(&path, &self.search_buffer).is_some() {
                self.search_results.push(entry.path().to_path_buf());
            }
        }
        self.selected_result = 0;
    }

    pub fn open_selected_file(&mut self, editor: &mut TextEditor) -> Result<()> {
        
        if self.search_results.get(self.selected_result).is_some() {
            let path = std::mem::take(&mut self.search_results[self.selected_result]);
            editor.load_file(path)?;
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        *self = Self::default()
    }

    pub fn draw_ui(&mut self, frame: &mut Frame) {
        let area = frame.area();
        
        let overlay_area = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(1),
            ].as_ref())
            .split(area);

        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(" Ctrl+P: Find ");

        let input_area = overlay_area[0];
        let input_inner = input_block.inner(input_area);
        let input = Paragraph::new(format!("> {}", self.search_buffer))
            .block(input_block)
            .style(Style::default().fg(Color::White));
        frame.render_widget(input, overlay_area[0]);

        let items: Vec<ListItem> = self.search_results.iter()
            .enumerate()
            .map(|(i, path)| {
                let name = path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                
                let prefix = if i == self.selected_result {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                ListItem::new(vec![Line::raw(format!(" {}", name))])
                    .style(prefix)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} matches ", self.search_results.len()))
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            )
            .highlight_symbol(">");
        
        frame.render_widget(list, overlay_area[1]);

        let instructions = Paragraph::new("↑↓ navigate  Enter:open  Esc:cancel")
            .alignment(Alignment::Center);
        
        frame.render_widget(instructions, overlay_area[2]);

        let cursor_x = 3 + self.search_buffer.len() as u16; // 3 = "> " + border padding
        let cursor_y = input_inner.y; // Top of input area
        frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, cursor_y));
    }
}
