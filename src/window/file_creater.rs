use std::{fs::File, path::PathBuf};

use crate::{
    key_controller::{
        InsertKind, WindowControlReponse, WindowsControl, default_controls,
        handle_input::SessionEvent,
    },
    utils::{cursor::Cursor, syntaxer::Syntaxer, text_buffer::TextBuffer},
    window::Window,
};
use anyhow::Result;
use crossterm::event;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Constraint::Length, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};

const BOTTOM_HEADER: &str = "[↑↓: Move]  [Enter: Open]  [ESC: Exit window]";

/// Simple file/directory creation dialog.
///
/// Enter a filename in the input field at the given `in_path`. Enter creates
/// files (with extension) or directories (no extension). Uses current directory
/// from file tree or project base path.
#[derive(Debug, Clone)]
pub struct FileCreater {
    cursor: Cursor,
    pub in_path: PathBuf,
    search_buffer: TextBuffer,
}
impl FileCreater {
    pub fn new(in_path: PathBuf) -> Self {
        Self {
            in_path,
            cursor: Cursor::default(),
            search_buffer: TextBuffer::new_single_line(String::new()),
        }
    }

    fn create_from_path(&mut self) -> Result<()> {
        let line = std::mem::take(&mut self.search_buffer[0]);
        self.in_path.push(line);
        if self.in_path.extension().is_none() {
            std::fs::create_dir(&self.in_path)?;
        } else {
            File::create(&self.in_path)?;
        }
        Ok(())
    }
}
impl Window for FileCreater {
    fn on_insert(&mut self) -> Result<()> {
        Ok(())
    }
    fn on_remove(&mut self) -> Result<()> {
        Ok(())
    }

    fn draw_ui(
        &mut self,
        frame: &mut Frame,
        header: Block,
        _syntaxer: &mut Syntaxer,
    ) -> Result<()> {
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

        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(" Create File/Folder (no extention means folder else file)");

        let input_area = overlay_area[0];
        let input_inner = input_block.inner(input_area);
        let input = Paragraph::new(format!("> {}", self.search_buffer[0]))
            .block(input_block)
            .style(Style::default().fg(Color::White));

        frame.render_widget(input, overlay_area[0]);

        let instructions = Paragraph::new(BOTTOM_HEADER).alignment(Alignment::Center);

        frame.render_widget(instructions, overlay_area[2]);

        let cursor_x = 3 + self.search_buffer.line_count() as u16; // 3 = "> " + border padding
        let cursor_y = input_inner.y; // Top of input area
        frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, cursor_y));
        Ok(())
    }
}

impl WindowsControl for FileCreater {
    fn move_up(&mut self) -> Result<WindowControlReponse> {
        Ok(WindowControlReponse::None)
    }

    fn move_down(&mut self) -> Result<WindowControlReponse> {
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
        self.create_from_path()?;
        Ok(WindowControlReponse::ToMainWindow)
    }

    fn backspace(&mut self) -> Result<WindowControlReponse> {
        Ok(WindowControlReponse::None)
    }

    fn insert(&mut self, insert: InsertKind) -> Result<WindowControlReponse> {
        default_controls::insert(&mut self.cursor, &mut self.search_buffer, insert);
        Ok(WindowControlReponse::None)
    }

    fn custom_action(&mut self, _action: event::Event) -> Result<Option<SessionEvent>> {
        Ok(None)
    }
}
