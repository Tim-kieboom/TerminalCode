use std::{
    io::{Read, Write},
    path::{Component as PathComponent, Path, PathBuf},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::{
    Frame,
    layout::Rect,
    text::Span,
    widgets::{Block, Borders, Paragraph},
};
use tui_term::widget::PseudoTerminal;
use vt100::Parser;

use crate::{StartupArgs, app::components::Component, keybinds::PanelContext, theme::Theme};

#[cfg(test)]
#[path = "tests/terminal_tests.rs"]
mod tests;

pub struct Terminal {
    project_path: PathBuf,
    session: Option<Session>,
    last_size: Option<(u16, u16)>,
    exited: bool,
    spawn_failed: bool,
    errors: Vec<String>,
}

struct Session {
    current_directory: PathBuf,
    command_buffer: Vec<u8>,
    screen: Arc<Mutex<Parser>>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    reader: Option<JoinHandle<()>>,
}

impl Session {
    fn spawn(rows: u16, cols: u16, current_directory: &Path) -> anyhow::Result<Self> {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(size)?;

        let mut command = CommandBuilder::new_default_prog();
        command.cwd(current_directory);

        let child = pair.slave.spawn_command(command)?;

        let screen = Arc::new(Mutex::new(Parser::new(rows, cols, 0)));
        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let reader_screen = Arc::clone(&screen);
        let reader = std::thread::Builder::new()
            .name("terminal-reader".to_string())
            .spawn(move || read_loop(reader, reader_screen))?;

        Ok(Self {
            current_directory: current_directory.to_path_buf(),
            command_buffer: Vec::new(),
            screen,
            master: pair.master,
            writer,
            child,
            reader: Some(reader),
        })
    }

    fn resize(&mut self, rows: u16, cols: u16) -> anyhow::Result<()> {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        self.master.resize(size)?;

        let Ok(mut screen) = self.screen.lock() else {
            return Ok(());
        };
        screen.screen_mut().set_size(rows, cols);

        Ok(())
    }

    fn write(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.track_command(bytes);
        self.writer.write_all(bytes)?;
        self.writer.flush()?;
        Ok(())
    }

    fn track_command(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            match byte {
                b'\r' | b'\n' => {
                    let command = String::from_utf8_lossy(&self.command_buffer);
                    if let Some(target) = parse_cd_target(&command)
                        && let Some(directory) = resolve_directory(&self.current_directory, &target)
                    {
                        self.current_directory = directory;
                    }
                    self.command_buffer.clear();
                }
                0x03 | 0x04 => self.command_buffer.clear(),
                0x7f | 0x08 => {
                    self.command_buffer.pop();
                }
                byte if byte >= 0x20 => self.command_buffer.push(byte),
                _ => {}
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        _ = self.child.kill();
        self.reader.take();
    }
}

impl Terminal {
    pub fn new(args: &StartupArgs) -> Self {
        Self {
            project_path: args.project_path().to_path_buf(),
            session: None,
            last_size: None,
            exited: false,
            spawn_failed: false,
            errors: Vec::new(),
        }
    }

    pub(super) fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    pub(super) fn write_input(&mut self, bytes: &[u8]) {
        let Some(session) = &mut self.session else {
            return;
        };
        if let Err(err) = session.write(bytes) {
            self.errors
                .push(format!("Failed to write to terminal: {err}"));
        }
    }

    fn title(&self, width: u16, style: ratatui::style::Style) -> Span<'static> {
        let text = match &self.session {
            Some(session) => {
                let directory = session.current_directory.display();
                format!(" Terminal │ {directory} ")
            }
            None => " Terminal ".to_string(),
        };

        let max_len = width.saturating_sub(4) as usize;
        Span::styled(truncate_start(&text, max_len), style)
    }

    fn ensure_spawned(&mut self, rows: u16, cols: u16) {
        if self.session.is_some() || self.exited || self.spawn_failed {
            return;
        }
        if rows == 0 || cols == 0 {
            return;
        }

        match Session::spawn(rows, cols, &self.project_path) {
            Ok(session) => {
                self.session = Some(session);
                self.last_size = Some((rows, cols));
            }
            Err(err) => {
                self.spawn_failed = true;
                self.errors.push(format!("Failed to spawn terminal: {err}"));
            }
        }
    }

    fn check_exit(&mut self) {
        let Some(session) = &mut self.session else {
            return;
        };
        let exited = matches!(session.child.try_wait(), Ok(Some(_)));
        if exited {
            self.exited = true;
        }
    }

    fn resize_if_needed(&mut self, rows: u16, cols: u16) {
        if self.exited {
            return;
        }
        if self.last_size == Some((rows, cols)) {
            return;
        }

        let Some(session) = &mut self.session else {
            return;
        };
        match session.resize(rows, cols) {
            Ok(()) => self.last_size = Some((rows, cols)),
            Err(err) => self
                .errors
                .push(format!("Failed to resize terminal: {err}")),
        }
    }
}

impl Component for Terminal {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let focused = context == PanelContext::BottomBar;

        let title_style = if focused {
            Theme::title_focused()
        } else {
            Theme::title_default()
        };

        let border_style = if focused {
            Theme::border_focused()
        } else {
            Theme::border_default()
        };

        let inner = inner_area(area);
        let rows = inner.height;
        let cols = inner.width;

        self.ensure_spawned(rows, cols);
        self.check_exit();
        self.resize_if_needed(rows, cols);

        let title = self.title(area.width, title_style);

        let block = Block::default()
            .title(title)
            .borders(Borders::TOP)
            .border_style(border_style);

        let Some(session) = &mut self.session else {
            let message = if self.spawn_failed {
                "Failed to spawn shell"
            } else if self.exited {
                "Process exited"
            } else {
                ""
            };

            let paragraph = Paragraph::new(message)
                .style(Theme::editor_background())
                .block(block);
            frame.render_widget(paragraph, area);
            return;
        };

        let Ok(screen) = session.screen.lock() else {
            frame.render_widget(block, area);
            return;
        };

        let widget = PseudoTerminal::new(screen.screen());
        frame.render_widget(block, area);
        frame.render_widget(widget, inner);
    }
}

fn inner_area(area: Rect) -> Rect {
    Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height.saturating_sub(1),
    }
}

fn truncate_start(text: &str, max_len: usize) -> String {
    let len = text.chars().count();
    if len <= max_len {
        return text.to_string();
    }

    let keep = max_len.saturating_sub(1);
    let mut truncated = String::with_capacity(max_len + 1);
    truncated.push('…');
    truncated.extend(text.chars().skip(len - keep));
    truncated
}

fn parse_cd_target(command: &str) -> Option<String> {
    let command = command.trim();
    let mut chars = command.chars();
    if !chars.next()?.eq_ignore_ascii_case(&'c') || !chars.next()?.eq_ignore_ascii_case(&'d') {
        return None;
    }

    let rest = chars.as_str().trim_start();
    if rest.is_empty() {
        return None;
    }
    if rest.starts_with(['~', '$', '-']) || rest.contains(['&', ';', '|', '<', '>']) {
        return None;
    }

    let target = unquote(rest);
    if target.is_empty() {
        return None;
    }
    Some(target)
}

fn unquote(text: &str) -> String {
    let mut chars = text.chars();
    let first = chars.next();
    let last = chars.last();
    match (first, last) {
        (Some('"'), Some('"')) | (Some('\''), Some('\'')) => text[1..text.len() - 1].to_string(),
        _ => text.to_string(),
    }
}

fn resolve_directory(current: &Path, target: &str) -> Option<PathBuf> {
    Some(normalize(&current.join(target)))
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            PathComponent::CurDir => {}
            PathComponent::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn read_loop(mut reader: Box<dyn Read + Send>, screen: Arc<Mutex<Parser>>) {
    let mut buffer = [0u8; 8192];

    loop {
        let n = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };

        let Ok(mut parser) = screen.lock() else {
            break;
        };
        parser.process(&buffer[..n]);
    }
}
