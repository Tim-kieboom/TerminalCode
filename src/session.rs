use anyhow::Result;
use crossterm::{
    event,
    terminal::ClearType,
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint::Length, Layout},
    style::{Color, Style},
    widgets::{Paragraph},
};
use std::{
    io::Stdout, path::PathBuf,
};
use crate::{text_editor::key_controller::Loop, text_editor::TextEditor};

type StdTerminal = Terminal<CrosstermBackend<Stdout>>;

const BOTTOM_HEADER: &str = "['ESC' exit] ['ctr+p' lookup] ['ctr+`' terminal]";

#[derive(Debug)]
pub struct Session {
    terminal: StdTerminal,
    editor: TextEditor,
}

impl Session {
    pub fn new(mut stdout: Stdout) -> Result<Self> {
        use crossterm::terminal::Clear;
        const CLEAR_EXISTING_ECHOS: Clear = Clear(ClearType::All);

        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            stdout,
            crossterm::event::DisableBracketedPaste,
            crossterm::terminal::EnterAlternateScreen,
            crossterm::cursor::Hide,
            CLEAR_EXISTING_ECHOS,
        )?; 

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self{
            terminal,
            editor: TextEditor::new(),
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let result = self.run_loop();

        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            self.terminal.backend_mut(),
            crossterm::event::EnableBracketedPaste,
            crossterm::terminal::LeaveAlternateScreen
        )?;
        self.terminal.show_cursor()?;
        result
    }

    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        self.editor.load_file(path)
    }

    fn run_loop(&mut self) -> Result<()> {
        
        loop {
            let event = event::read()?;
            
            match self.editor.handle_event(event)? {
                Loop::Break => break,
                Loop::Continue => {
                    self.terminal.draw(|f| Self::draw_ui(f, &self.editor))?;
                }
            }
        }
        Ok(())
    }

    fn draw_ui(frame: &mut Frame, text_editor: &TextEditor) {
        use ratatui::layout::Direction;
        use ratatui::widgets::{Block, Borders};
        use ratatui::text::{Span};
        use ratatui::style::Modifier;

        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Length(area.height - 1), Length(1)].as_ref())
            .split(area);

        
        let end_span = if let Some(path) = text_editor.file_path.as_ref() {
            let saved = if text_editor.file_saved {""} else {"*"};
            Span::styled(format!("-{}{saved} ⟧", path.as_os_str().display()), Style::default().fg(Color::Cyan))
        }else {
            Span::styled(" ⟧", Style::default().fg(Color::Cyan))
        };

        let text = text_editor.buffer.join("\n");
        let editor_box = Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::TOP | Borders::BOTTOM)
                    .title(vec![
                        Span::styled("⟦ ", Style::default().fg(Color::Cyan)),
                        Span::styled("TerminalCode", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        end_span,
                    ])
            );
        frame.render_widget(editor_box, chunks[0]);
        frame.render_widget(
            Paragraph::new(BOTTOM_HEADER),
            chunks[1],
        );

        let mut cursor = text_editor.cursor;
        cursor.line = cursor.line.saturating_add(1);
        frame.set_cursor_position(cursor);
    }
}



