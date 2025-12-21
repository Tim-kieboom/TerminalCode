use crate::{
    context::SharedContext,
    key_controller::{
        WindowControlReponse, WindowsControl, default_controls, key_controller::SessionEvent,
    },
    utils::{
        cursor::Cursor, scrollable_view::ScrollableView, syntaxer::Syntaxer, text_buffer::TextBuffer
    },
    window::Window,
};
use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint::Length, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Paragraph},
};
use std::path::PathBuf;

const HEADER_HEIGHT_TOP: u16 = 1;
const HEADER_HEIGHT_BOTTOM: u16 = 2;
const TOTAL_HEADER_HEIGHT: u16 = HEADER_HEIGHT_TOP + HEADER_HEIGHT_BOTTOM;

const BOTTOM_HEADER: &str = "[shift+ESC: Exit] [ctr+p: Lookup] [ctr+`: Terminal]";

/// Core text editing component for the IDE.
///
/// Manages buffer content, cursor position, scrolling, syntax highlighting,
/// and file I/O operations. Implements the main text editing experience.
#[derive(Debug)]
pub(crate) struct TextEditor {
    cursor: Cursor,
    view: ScrollableView,
    buffer: TextBuffer,
    file: Option<PathBuf>,
    context: SharedContext,
}
impl TextEditor {
    pub fn new(context: SharedContext) -> Self {
        let area = context.get_area().clone();

        Self {
            context,
            file: None,
            cursor: Cursor::default(),
            buffer: TextBuffer::new_multi_line(vec![String::new()]),
            view: ScrollableView::from_area(area, TOTAL_HEADER_HEIGHT),
        }
    }

    /// Returns a reference to the currently loaded file path.
    ///
    /// Used by session management to sync file context across windows.
    pub fn get_file_path(&self) -> &Option<PathBuf> {
        &self.file
    }

    /// Marks the current file as having unsaved changes.
    pub fn mark_file_unsaved(&mut self) {
        self.context.set_file_context(|file_context| {
            file_context.file_saved = false
        });
    }

    /// Loads file content from disk and populates the editor buffer.
    ///
    /// Updates syntax highlighting rules based on file extension and resets
    /// cursor position.
    pub fn load_file(&mut self, path: PathBuf, syntaxer: &mut Syntaxer) -> Result<()> {
        syntaxer.update_syntax(&path);

        self.file = Some(path);
        let path_ref = self.file.as_ref().expect("just assigned to Some");

        let content = std::fs::read_to_string(path_ref)?;
        let lines = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(String::from).collect()
        };
        self.buffer = TextBuffer::new_multi_line(lines);

        self.cursor = Cursor::default();
        Ok(())
    }

    /// Writes current buffer content to disk at the given path.
    pub fn save_file(&mut self, path: &PathBuf) -> Result<()> {
        let content = self.buffer.as_slice().join("\n");
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Resets editor to empty "no file" state.
    pub fn set_to_no_file(&mut self) {
        self.file = None;
        self.buffer.clear();
        self.cursor = Cursor::default();
    }
}

impl Window for TextEditor {
    fn on_insert(&mut self) -> Result<()>{
        self.mark_file_unsaved();
        Ok(())
    }

    fn on_remove(&mut self) -> Result<()>{
        self.mark_file_unsaved();
        Ok(())
    }

    fn draw_ui(&mut self, frame: &mut Frame, header: Block, syntaxer: &mut Syntaxer) -> Result<()>{
        let area = frame.area();
        self.view.update_area(area, TOTAL_HEADER_HEIGHT);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Length(area.height - 1), Length(1)].as_ref())
            .split(area);

        let text = self.view.text_buffer_to_view(&self.cursor, &self.buffer);
        let syntax_text = syntaxer.to_highighed(&text)?;

        let editor_box = Paragraph::new(syntax_text)
            .style(Style::default().fg(Color::White))
            .block(header);

        frame.render_widget(editor_box, chunks[0]);
        frame.render_widget(Paragraph::new(BOTTOM_HEADER), chunks[1]);

        let mut cursor = self.cursor;
        let max_curser_line = area.height.saturating_sub(HEADER_HEIGHT_BOTTOM + 1);
        cursor.line = cursor
            .line
            .saturating_add(HEADER_HEIGHT_TOP)
            .min(max_curser_line);
        frame.set_cursor_position(cursor);
        Ok(())
    }
}

impl WindowsControl for TextEditor {
    fn move_up(&mut self) -> Result<WindowControlReponse> {
        default_controls::move_up(&mut self.cursor, &self.buffer);

        Ok(WindowControlReponse::None)
    }

    fn move_down(&mut self) -> Result<WindowControlReponse> {
        default_controls::move_down(&mut self.cursor, &self.buffer);
        Ok(WindowControlReponse::None)
    }

    fn move_left(&mut self, amount: u16) -> Result<WindowControlReponse> {
        default_controls::move_left(&mut self.cursor, &self.buffer, amount);
        Ok(WindowControlReponse::None)
    }

    fn move_right(&mut self, amount: u16) -> Result<WindowControlReponse> {
        default_controls::move_right(&mut self.cursor, &self.buffer, amount);
        Ok(WindowControlReponse::None)
    }

    fn enter(&mut self) -> Result<WindowControlReponse> {
        default_controls::enter(&mut self.cursor, &mut self.buffer);
        Ok(WindowControlReponse::None)
    }

    fn backspace(&mut self) -> Result<WindowControlReponse> {
        default_controls::remove(&mut self.cursor, &mut self.buffer);
        Ok(WindowControlReponse::None)
    }

    fn insert(
        &mut self,
        insert: crate::key_controller::InsertKind,
    ) -> Result<WindowControlReponse> {
        default_controls::insert(&mut self.cursor, &mut self.buffer, insert);
        Ok(WindowControlReponse::None)
    }

    fn custom_action(&mut self, action: event::Event) -> Result<Option<SessionEvent>> {
        match action {
            event::Event::Key(key) if key.code == KeyCode::Char('n') && key.modifiers == KeyModifiers::NONE => {
                
                Ok(None)
            },
            _ => Ok(None),
        }
    }
}
