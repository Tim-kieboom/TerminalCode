use ratatui::{
    Frame,
    layout::{Constraint::Length, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Paragraph},
};

use crate::{key_controller::key_controller::KeyController, window::Window};

pub mod file_handler;

const BOTTOM_HEADER: &str = "['ESC' exit] ['ctr+p' lookup] ['ctr+`' terminal]";

#[derive(Debug, Default)]
pub(crate) struct TextEditor {
    pub(crate) buffer: Vec<String>,
    pub(crate) cursor: Cursor,
}
impl TextEditor {
    pub fn new() -> Self {
        Self {
            buffer: vec![String::new()],
            ..Default::default()
        }
    }
}
impl Window for TextEditor {
    fn draw_ui(&mut self, frame: &mut Frame, header: Block) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Length(area.height - 1), Length(1)].as_ref())
            .split(area);

        let text = self.buffer.join("\n");
        let editor_box = Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .block(header);

        frame.render_widget(editor_box, chunks[0]);
        frame.render_widget(Paragraph::new(BOTTOM_HEADER), chunks[1]);

        let mut cursor = self.cursor;
        cursor.line = cursor.line.saturating_add(1);
        frame.set_cursor_position(cursor);
    }

    fn new_key_controller<'a>(&'a mut self, file_saved: &'a mut bool) -> KeyController<'a> {
        KeyController::new(&mut self.cursor, &mut self.buffer, file_saved)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Cursor {
    pub(crate) line: u16,
    pub(crate) offset: u16,
}
impl From<Cursor> for ratatui::layout::Position {
    fn from(value: Cursor) -> Self {
        ratatui::layout::Position::new(value.offset, value.line)
    }
}
