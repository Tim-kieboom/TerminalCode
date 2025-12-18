use crate::{
    key_controller::{InsertKind, KeyController, KeyDoneKind},
    window::Window,
};
use anyhow::{Error, Result};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::Paragraph,
};

const BOTTOM_HEADER: &str = " [ESC: Exit window] ";

#[derive(Debug)]
pub enum NotificationLevel {
    #[allow(unused)]
    Note,
    #[allow(unused)]
    Error,
    #[allow(unused)]
    Warning,
}

#[derive(Debug)]
pub struct NotificationWindow {
    buffer: String,
    level: NotificationLevel,
}
impl NotificationWindow {
    pub fn new_error(error: Error) -> Self {
        Self {
            buffer: error.to_string(),
            level: NotificationLevel::Error,
        }
    }
}
impl Window for NotificationWindow {
    fn on_insert(&mut self) {}

    fn on_remove(&mut self) {}

    fn draw_ui(&mut self, frame: &mut ratatui::Frame, header: ratatui::widgets::Block) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(area.height - 2), Constraint::Length(2)].as_ref())
            .split(area);

        let color = match self.level {
            NotificationLevel::Note => Color::Blue,
            NotificationLevel::Error => Color::Red,
            NotificationLevel::Warning => Color::Yellow,
        };

        let text = self.buffer.as_str();
        let editor_box = Paragraph::new(text)
            .style(Style::default().fg(color))
            .block(header);

        frame.render_widget(editor_box, chunks[0]);
        frame.render_widget(Paragraph::new(BOTTOM_HEADER), chunks[1]);
    }
}
impl KeyController for NotificationWindow {
    fn move_up(&mut self) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }

    fn move_down(&mut self) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }

    fn move_left(&mut self, _amount: u16) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }

    fn move_right(&mut self, _amount: u16) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }

    fn enter(&mut self) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }

    fn backspace(&mut self) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }

    fn insert(&mut self, _insert: InsertKind) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }
}
