use crate::{
    key_controller::{InsertKind, KeyController, KeyDoneKind, default_controls},
    window::{Window, text_editor::Cursor},
};
use anyhow::Result;
use std::process::Command;

#[derive(Debug)]
pub struct CommandPrompt {
    output_message: String,
    buffer: Vec<String>,
    input_line: [String; 1],
    cursor: Cursor,
}

const BOTTOM_HEADER: &str = "[ESC: Exit window]  [Enter: Execute]";

impl CommandPrompt {
    pub fn new() -> Self {
        Self {
            output_message: String::new(),
            input_line: [String::new()],
            cursor: Cursor::default(),
            buffer: vec!["TerminalCode shell".to_string()],
        }
    }

    fn run_command(&mut self) -> Result<()> {
        let command = std::mem::take(&mut self.input_line[0]);
        self.cursor = Cursor::default();
        let output = if cfg!(target_os = "windows") {
            Command::new("cmd").args(["/C", &command]).output()?
        } else {
            Command::new("sh").args(["-c", &command]).output()?
        };

        const SUCCESS_MSG: &str = "\ncommand successfull";
        const FAIL_MSG: &str = "\ncommand failed";

        let success = output.status.success();

        let log_msg = if success {
            std::str::from_utf8(&output.stdout)?
        } else {
            std::str::from_utf8(&output.stderr)?
        };

        let end_msg = if success { SUCCESS_MSG } else { FAIL_MSG };

        self.output_message = String::with_capacity(log_msg.len() + end_msg.len());
        self.output_message.push_str(log_msg);
        self.output_message.push_str(end_msg);
        Ok(())
    }
}

impl Window for CommandPrompt {
    fn on_insert(&mut self) {}
    fn on_remove(&mut self) {}

    fn draw_ui(&mut self, frame: &mut ratatui::Frame, header: ratatui::widgets::Block) {
        use ratatui::{
            layout::{Constraint::Length, Direction, Layout},
            style::{Color, Style},
            widgets::{Borders, Paragraph},
        };

        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Length(area.height - 1), Length(1)].as_ref())
            .split(area);

        let mut lines = self.buffer.clone();
        lines.push(format!("$ {}\n{}", self.input_line[0], self.output_message));

        let text = lines.join("\n");
        let main = Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .block(header.borders(Borders::all()));

        frame.render_widget(main, chunks[0]);

        frame.render_widget(Paragraph::new(BOTTOM_HEADER), chunks[1]);

        const LINE_OFFSET: u16 = 2;
        const OFFSET_OFFSET: u16 = 3;

        let mut cursor = self.cursor;
        cursor.line += LINE_OFFSET;
        cursor.offset += OFFSET_OFFSET;

        frame.set_cursor_position(cursor);
    }
}

impl KeyController for CommandPrompt {
    fn move_up(&mut self) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }

    fn move_down(&mut self) -> Result<KeyDoneKind> {
        Ok(KeyDoneKind::None)
    }

    fn move_left(&mut self, amount: u16) -> Result<KeyDoneKind> {
        default_controls::move_left(&mut self.cursor, &self.input_line, amount);
        Ok(KeyDoneKind::None)
    }

    fn move_right(&mut self, amount: u16) -> Result<KeyDoneKind> {
        default_controls::move_right(&mut self.cursor, &self.input_line, amount);
        Ok(KeyDoneKind::None)
    }

    fn enter(&mut self) -> Result<KeyDoneKind> {
        self.run_command()?;
        Ok(KeyDoneKind::None)
    }

    fn backspace(&mut self) -> Result<KeyDoneKind> {
        default_controls::remove_single_line(&mut self.cursor, &mut self.input_line[0]);
        Ok(KeyDoneKind::None)
    }

    fn insert(&mut self, insert: InsertKind) -> Result<KeyDoneKind> {
        default_controls::insert_single_line(&mut self.cursor, &mut self.input_line[0], insert);
        Ok(KeyDoneKind::None)
    }
}
