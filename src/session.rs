use crate::{
    context::{FileContext, SharedContext}, key_controller::{
        input_event::get_input_event,
        key_controller::{SessionEvent, handle_input},
    }, window::{Window, WindowKind, command_prompt::CommandPrompt, lookup_bar::LookupBar, text_editor::TextEditor}
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
use std::{
    io::Stdout, path::{Path, PathBuf}
};

type StdTerminal = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug)]
pub struct Session {
    terminal: StdTerminal,
    context: SharedContext,
    window_stack: Vec<WindowKind>,
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

        let file_context = FileContext {
            base_path,
            file_path: None,
            file_saved: true,
        };

        let context = SharedContext::new(file_context);
        let main_window = WindowKind::TextEditor(TextEditor::new(context.clone()));
        Ok(Self {
            context,
            terminal,
            window_stack: vec![main_window],
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
                    &self.context,
                )
            })?;
        }
        Ok(())
    }

    fn handle_input(&mut self, event: Event) -> Result<Loop> {
        let session_event = {
            let input_event = get_input_event(event);
            let window = current_window(&mut self.window_stack);

            match handle_input(window, input_event)? {
                Some(val) => val,
                None => return Ok(Loop::None),
            }
        };

        match session_event {
            SessionEvent::Exit => return Ok(Loop::Break),

            SessionEvent::OnRemove => {
                current_window(&mut self.window_stack).on_remove();
            }
            SessionEvent::OnInsert => {
                current_window(&mut self.window_stack).on_insert();
            }
            SessionEvent::Back => {
                let window_amount = self.window_stack.len();
                if window_amount <= 1 {
                    return Ok(Loop::None)
                }
                self.window_stack.pop();
                
                let file_context = &mut self.context.borrow_mut().file_context;
                let main = main_window(&mut self.window_stack);
                if window_amount == 2 && main.file != file_context.file_path {
                    
                    match &file_context.file_path {
                        Some(path) => main.load_file(path.clone())?,
                        None => main.set_to_no_file(),
                    }
                }
            }
            SessionEvent::SaveFile => {
                
                let file_context = &mut self.context.borrow_mut().file_context;
                if let Some(path) = &file_context.file_path {
                    main_window(&mut self.window_stack).save_file(path)?;
                }

                file_context.file_saved = true;
            }
            SessionEvent::OpenCommandPrompt => {
                if !matches!(
                    current_window(&mut self.window_stack),
                    WindowKind::CommandPrompt(_)
                ) {
                    let command_prompt = CommandPrompt::new(self.context.clone());
                    self.window_stack.push(WindowKind::CommandPrompt(command_prompt));
                }
            }
            SessionEvent::OpenLookup => {
                if !matches!(
                    current_window(&mut self.window_stack),
                    WindowKind::LookupBar(_)
                ) {
                    self.window_stack
                        .push(WindowKind::LookupBar(LookupBar::new(self.context.clone())));
                }
            }
        }

        Ok(Loop::None)
    }

    fn draw_ui<Win: Window>(frame: &mut Frame, window: &mut Win, context: &SharedContext) {
        use ratatui::style::Modifier;
        use ratatui::text::Span;
        use ratatui::widgets::{Block, Borders};

        fn relative_to<'a>(base: &Path, path: &'a Path) -> Option<&'a Path> {
            path.strip_prefix(base).ok()
        }

        let file_context = &context.borrow().file_context;

        let end_span = if let Some(path) = file_context.file_path.as_ref() {
            let saved = if file_context.file_saved { "" } else { "*" };
            let relative_path =
                relative_to(&file_context.base_path, path).unwrap_or(&Path::new(""));

            Span::styled(
                format!(
                    "-{}{saved} ⟧ {}",
                    relative_path.display(),
                    file_context.base_path.display()
                ),
                Style::default().fg(Color::Cyan),
            )
        } else {
            Span::styled(
                format!(" ⟧ {}", file_context.base_path.display()),
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

fn main_window<'a>(window_stack: &'a mut [WindowKind]) -> &'a mut TextEditor {
    match window_stack.get_mut(0).expect("should have main window") {
        WindowKind::TextEditor(text_editor) => text_editor,
        _ => unreachable!("main window has to be WindowKind::TextEditor"),
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
    #[allow(unused)]
    Continue,
}
