use std::process::{Command, Output};

use anyhow::Result;
use crate::{context::SharedContext, key_controller::{InsertKind, KeyController, KeyDoneKind, default_controls}, window::{Window, text_editor::Cursor}};

#[derive(Debug)]
pub struct CommandPrompt {
    buffer: Vec<String>,
    input_line: [String; 1],
    cursor: Cursor,
    context: SharedContext,
}

impl CommandPrompt {
    pub fn new(context: SharedContext) -> Self {
        Self {
            context,
            input_line: [String::new()],
            cursor: Cursor::default(),
            buffer: vec!["TerminalCode shell".to_string()],
        }
    }

    fn push_output<S: Into<String>>(&mut self, string: S) {
        self.buffer.push(string.into());
    }

    fn run_command(&mut self) -> Result<()> {
        let command = &self.input_line[0];
        let output = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .args(["/C", command])
                .output()?
        } else {
            Command::new("sh")
                .args(["-c", command])
                .output()?
        };

        self.buffer.push(
            std::str::from_utf8(&output.stderr)?.to_string()
        );
        self.buffer.push(
            std::str::from_utf8(&output.stdout)?.to_string()
        );
        Ok(())
    }
}

impl Window for CommandPrompt {
    fn on_insert(&mut self) {}
    fn on_remove(&mut self) {}

    fn draw_ui(&mut self, frame: &mut ratatui::Frame, header: ratatui::widgets::Block) {
        use ratatui::{
            widgets::Paragraph,
            style::{Color, Style},
            layout::{Constraint::Length, Direction, Layout},
        };

        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Length(area.height - 1), Length(1)].as_ref())
            .split(area);

        let mut lines = self.buffer.clone();
        lines.push(format!("$ {}", self.input_line[0]));

        let text = lines.join("\n");
        let main = Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .block(header);

        frame.render_widget(main, chunks[0]);

        frame.render_widget(
            Paragraph::new(" test "),
            chunks[1],
        );
                         
        let cursor = Cursor{
            line: chunks[0].x + 2 + self.input_line.len() as u16,
            offset: chunks[0].y + (lines.len() as u16) - 1,
        };
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