use crate::{
    context::{FileContext, SharedContext},
    key_controller::{
        WindowsControl,
        input_event::get_input_event,
        key_controller::{SessionEvent, handle_input},
    },
    utils::{path_display::display_path, syntaxer::Syntaxer},
    window::{
        Window, WindowKind, command_prompt::CommandPrompt, file_creater::FileCreater, filetree_window::FileTreeWindow, lookup_bar::LookupBar, notification_window::NotificationWindow, text_editor::TextEditor
    },
};
use anyhow::{Error, Result};
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
    io::Stdout,
    path::{Path, PathBuf},
};

type StdTerminal = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug)]
pub struct Session {
    terminal: StdTerminal,
    context: SharedContext,
    window_stack: Vec<WindowKind>,
    syntaxer: Syntaxer,
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
        let mut terminal = Terminal::new(backend)?;

        let file_context = FileContext {
            base_path,
            file_path: None,
            file_saved: true,
        };

        let context = SharedContext::new(file_context, terminal.get_frame().area());
        let main_window = WindowKind::TextEditor(TextEditor::new(context.clone()));
        Ok(Self {
            context,
            terminal,
            syntaxer: Syntaxer::default(),
            window_stack: vec![main_window],
        })
    }

    pub fn run(&mut self) -> Result<()> {
        loop {
            let input = event::read()?;
            match self.handle_input(input)? {
                Loop::None => (),
                Loop::Break => break,
                Loop::Continue => continue,
            }

            let window = current_window(&mut self.window_stack)?;
            let mut error = Ok(());
            self.terminal.draw(|frame| {
                if let Err(err) = Self::draw_ui(frame, window, &self.context, &mut self.syntaxer) {
                    error = Err(err);
                    return
                } 

                self.context.set_area(frame.area());
            })?;
            error?;
        }
        Ok(())
    }

    pub fn dispose(&mut self) -> Result<()> {
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            self.terminal.backend_mut(),
            crossterm::event::EnableBracketedPaste,
            crossterm::terminal::LeaveAlternateScreen
        )?;
        self.terminal.show_cursor()?;
        Ok(())
    }

    pub fn display_error(&mut self, error: Error) {
        let notification = WindowKind::NotificationWindow(NotificationWindow::new_error(error));
        self.window_stack.push(notification);
    }

    fn handle_input(&mut self, event: Event) -> Result<Loop> {
        let session_event = {
            let input_event = get_input_event(event.clone());
            let window = current_window(&mut self.window_stack)?;

            let session_event = match handle_input(window, input_event)? {
                Some(val) => val,
                None => return Ok(Loop::None),
            };

            if let Some(session) = window.custom_action(event)? {
                session
            } else {
                session_event
            }
        };

        match session_event {
            SessionEvent::Exit => return Ok(Loop::Break),
            SessionEvent::OnRemove => {
                current_window(&mut self.window_stack)?.on_remove()?;
            }
            SessionEvent::OnInsert => {
                current_window(&mut self.window_stack)?.on_insert()?;
            }
            SessionEvent::SaveFile => {

                self.context.get_file_context(|file_context| -> Result<bool> {

                    if let Some(path) = &file_context.file_path {
                        main_window(&mut self.window_stack)?.save_file(&path)?;
                    }

                    Ok(true)
                })?;

                self.context.set_file_context(|file_context| {
                    file_context.file_saved = true
                });
            }
            SessionEvent::OpenFileTreeWindow => {
                if !matches!(
                    current_window(&mut self.window_stack)?,
                    WindowKind::FileTreeWindow(_)
                ) {
                    let window = FileTreeWindow::new(self.context.clone());
                    self.window_stack.push(WindowKind::FileTreeWindow(window));
                }
            }
            SessionEvent::OpenCommandPrompt => {
                if !matches!(
                    current_window(&mut self.window_stack)?,
                    WindowKind::CommandPrompt(_)
                ) {
                    let command_prompt = CommandPrompt::new();
                    self.window_stack
                        .push(WindowKind::CommandPrompt(command_prompt));
                }
            }
            SessionEvent::OpenLookup => {
                if !matches!(
                    current_window(&mut self.window_stack)?,
                    WindowKind::LookupBar(_)
                ) {
                    self.window_stack
                        .push(WindowKind::LookupBar(LookupBar::new(self.context.clone())));
                }
            }
            SessionEvent::OpenFileCreater{in_path} => {
                
                match current_window(&mut self.window_stack)? {
                    WindowKind::FileCreater(file_creater) => file_creater.in_path = in_path,
                    _ => self.window_stack.push(WindowKind::FileCreater(FileCreater::new(in_path))),
                }
            },
            SessionEvent::Back => {
                let window_amount = self.window_stack.len();
                if window_amount <= 1 {
                    return Ok(Loop::None);
                }
                self.window_stack.pop();
                self.on_window_pop()?;
            }
            SessionEvent::ToMainWindow => {
                self.window_stack.truncate(1);
                self.on_window_pop()?;
            }
            SessionEvent::TestDebugEvent => {
                return Err(Error::msg("testing error message\n test \n\nboo"));
            },
        }

        Ok(Loop::None)
    }

    fn draw_ui<Win: Window>(frame: &mut Frame, window: &mut Win, context: &SharedContext, syntaxer: &mut Syntaxer) -> Result<()> {
        use ratatui::style::Modifier;
        use ratatui::text::Span;
        use ratatui::widgets::{Block, Borders};

        fn relative_to<'a>(base: &Path, path: &'a Path) -> Option<&'a Path> {
            path.strip_prefix(base).ok()
        }

        let max_path_len = (frame.area().width / 3) as usize;

        let mut end_span = Span::raw("");
        context.get_file_context(|file_context| {

            let base_path = &file_context.base_path;
            end_span = if let Some(path) = file_context.file_path.as_ref() {
                let saved = if file_context.file_saved { "" } else { "*" };
                let relative_path = relative_to(base_path, path).unwrap_or(&Path::new(""));

                Span::styled(
                    format!(
                        "-{}{saved} ⟧ {}",
                        display_path(relative_path, max_path_len),
                        display_path(base_path, max_path_len),
                    ),
                    Style::default().fg(Color::Cyan),
                )
            } else {
                Span::styled(
                    format!(" ⟧ {} ", display_path(base_path, max_path_len)),
                    Style::default().fg(Color::Cyan),
                )
            };
        });

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

        window.draw_ui(frame, header, syntaxer)
    }

    fn on_window_pop(&mut self) -> Result<()> {
        let window_amount = self.window_stack.len();
        let main = main_window(&mut self.window_stack)?;

        self.context.get_file_context(|file_context| -> Result<()> {

            let file_path = &file_context.file_path;
            let is_new_file = main.get_file_path() != file_path;
            if window_amount == 1 && is_new_file {
                match &file_path {
                    Some(path) => main.load_file(path.clone(), &mut self.syntaxer)?,
                    None => main.set_to_no_file(),
                }
            }

            Ok(())
        })?;

        Ok(())
    }
}

fn main_window<'a>(window_stack: &'a mut [WindowKind]) -> Result<&'a mut TextEditor> {
    let window = window_stack
        .get_mut(0)
        .ok_or(Error::msg("should have main window"))?;

    match window {
        WindowKind::TextEditor(text_editor) => Ok(text_editor),
        _ => Err(Error::msg(
            "unreachable window_stack[0] has to be WindowKind::TextEditor",
        )),
    }
}

fn current_window<'a>(window_stack: &'a mut [WindowKind]) -> Result<&'a mut WindowKind> {
    window_stack
        .last_mut()
        .ok_or(Error::msg("should not have empty window_stack"))
}

enum Loop {
    None,
    Break,
    #[allow(unused)]
    Continue,
}
