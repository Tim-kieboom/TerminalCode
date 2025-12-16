use crate::{
    key_controller::{input_event::get_input_event, key_controller::SessionEvent},
    window::{Window, WindowKind, lookup_bar::LookupBar, text_editor::TextEditor},
};
use anyhow::Result;
use crossterm::{
    event::{self, Event},
    terminal::ClearType,
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    style::{Color, Style},
};
use std::{io::Stdout, path::PathBuf};

type StdTerminal = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug)]
pub struct Session {
    terminal: StdTerminal,

    base_path: PathBuf,
    window_stack: Vec<WindowKind>,

    file_saved: bool,
    file_path: Option<PathBuf>,
}

impl Session {
    pub fn new(mut stdout: Stdout, base_path: PathBuf) -> Result<Self> {
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

        let main_window = WindowKind::TextEditor(TextEditor::new());
        Ok(Self {
            terminal,

            base_path,
            window_stack: vec![main_window],

            file_path: None,
            file_saved: true,
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

    fn run_loop(&mut self) -> Result<()> {
        loop {
            let input = event::read()?;
            match self.handle_input(input)? {
                Loop::None => (),
                Loop::Break => break,
                Loop::Continue => continue,
            }

            self.terminal.draw(|frame| {
                Self::draw_ui(
                    frame,
                    current_window(&mut self.window_stack),
                    &self.base_path,
                    &self.file_path,
                    self.file_saved,
                )
            })?;
        }
        Ok(())
    }

    fn handle_input(&mut self, event: Event) -> Result<Loop> {
        let input_event = get_input_event(event);

        let session_event = {
            let window = current_window(&mut self.window_stack);
            let mut key_controller = window.new_key_controller(&mut self.file_saved);

            match key_controller.handle_input(input_event) {
                Some(val) => val,
                None => return Ok(Loop::None),
            }
        };

        match session_event {
            SessionEvent::Exit => return Ok(Loop::Break),

            SessionEvent::Back => {
                if self.window_stack.len() > 1 {
                    self.window_stack.pop();
                }
            }
            SessionEvent::SaveFile => {
                let editor = match &mut self.window_stack[0] {
                    WindowKind::TextEditor(val) => val,
                    _ => unreachable!("window_stack[0] should be TextEditor"),
                };

                if let Some(path) = &self.file_path {
                    editor.save_file(path)?;
                }

                self.file_saved = true;
            }
            SessionEvent::OpenLookup => {
                if !matches!(
                    current_window(&mut self.window_stack),
                    WindowKind::LookupBar(_)
                ) {
                    self.window_stack
                        .push(WindowKind::LookupBar(LookupBar::new()));
                }
            }
        }

        Ok(Loop::None)
    }

    fn draw_ui<Win: Window>(
        frame: &mut Frame,
        window: &mut Win,
        base_path: &PathBuf,
        file_path: &Option<PathBuf>,
        file_saved: bool,
    ) {
        use ratatui::style::Modifier;
        use ratatui::text::Span;
        use ratatui::widgets::{Block, Borders};

        let end_span = if let Some(path) = file_path.as_ref() {
            let saved = if file_saved { "" } else { "*" };
            Span::styled(
                format!("-{}{saved} ⟧ {}", path.display(), base_path.display()),
                Style::default().fg(Color::Cyan),
            )
        } else {
            Span::styled(
                format!(" ⟧ {}", base_path.display()),
                Style::default().fg(Color::Cyan),
            )
        };

        let header = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .title(vec![
                Span::styled("⟦ ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    "TerminalCode",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                end_span,
            ]);

        window.draw_ui(frame, header);
    }
}

fn current_window<'a>(window_stack: &'a mut [WindowKind]) -> &'a mut WindowKind {
    window_stack
        .last_mut()
        .expect("should not have empty window_stack")
}

enum Loop {
    None,
    Break,
    Continue,
}
