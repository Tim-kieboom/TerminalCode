use crate::{
    key_controller::{
        InsertKind, WindowControlReponse, WindowsControl, default_controls,
        key_controller::SessionEvent,
    },
    utils::{cursor::Cursor, syntaxer::Syntaxer, text_buffer::TextBuffer},
    window::Window,
};
use anyhow::Result;
use crossterm::event;
use std::process::Command;

#[derive(Debug)]
pub struct CommandPrompt {
    cursor: Cursor,
    buffer: Vec<String>,
    output_message: String,
    input_line: TextBuffer,
}

const BOTTOM_HEADER: &str = "[ESC: Exit window]  [Enter: Execute]";

impl CommandPrompt {
    pub fn new() -> Self {
        Self {
            cursor: Cursor::default(),
            output_message: String::new(),
            buffer: vec!["TerminalCode shell".to_string()],
            input_line: TextBuffer::new_single_line(String::new()),
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

        let out = std::str::from_utf8(&output.stdout)?;
        let err = std::str::from_utf8(&output.stderr)?;

        let end_msg = if success { SUCCESS_MSG } else { FAIL_MSG };

        self.output_message = String::with_capacity(out.len() + err.len() + end_msg.len());
        self.output_message.push_str(out);
        self.output_message.push_str(err);
        self.output_message.push_str(end_msg);
        Ok(())
    }
}

impl Window for CommandPrompt {
    fn on_insert(&mut self) -> Result<()> {Ok(())}
    fn on_remove(&mut self) -> Result<()> {Ok(())}

    fn draw_ui(&mut self, frame: &mut ratatui::Frame, header: ratatui::widgets::Block, _syntaxer: &mut Syntaxer) -> Result<()> {
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
        Ok(())
    }
}

impl WindowsControl for CommandPrompt {
    fn move_up(&mut self) -> Result<WindowControlReponse> {
        Ok(WindowControlReponse::None)
    }

    fn move_down(&mut self) -> Result<WindowControlReponse> {
        Ok(WindowControlReponse::None)
    }

    fn move_left(&mut self, amount: u16) -> Result<WindowControlReponse> {
        default_controls::move_left(&mut self.cursor, &self.input_line, amount);
        Ok(WindowControlReponse::None)
    }

    fn move_right(&mut self, amount: u16) -> Result<WindowControlReponse> {
        default_controls::move_right(&mut self.cursor, &self.input_line, amount);
        Ok(WindowControlReponse::None)
    }

    fn enter(&mut self) -> Result<WindowControlReponse> {
        self.run_command()?;
        Ok(WindowControlReponse::None)
    }

    fn backspace(&mut self) -> Result<WindowControlReponse> {
        default_controls::remove(&mut self.cursor, &mut self.input_line);
        Ok(WindowControlReponse::None)
    }

    fn insert(&mut self, insert: InsertKind) -> Result<WindowControlReponse> {
        default_controls::insert(&mut self.cursor, &mut self.input_line, insert);
        Ok(WindowControlReponse::None)
    }

    fn custom_action(&mut self, action: event::Event) -> Result<Option<SessionEvent>> {
        match action {
            _ => Ok(None),
        }
    }
}
