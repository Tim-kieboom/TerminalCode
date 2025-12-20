use std::path::PathBuf;

use anyhow::Result;
use crossterm::event;
use ratatui::{
    Frame,
    layout::{Constraint::Length, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Paragraph},
};

use crate::{
    context::SharedContext,
    key_controller::{WindowControlReponse, WindowsControl, default_controls, key_controller::SessionEvent},
    utils::{cursor::Cursor, scrollable_view::ScrollableView},
    window::Window,
};

const HEADER_HEIGHT_TOP: u16 = 1;
const HEADER_HEIGHT_BOTTOM: u16 = 2;
const TOTAL_HEADER_HEIGHT: u16 = HEADER_HEIGHT_TOP + HEADER_HEIGHT_BOTTOM;

const BOTTOM_HEADER: &str = "[shift+ESC: Exit] [ctr+p: Lookup] [ctr+`: Terminal]";

#[derive(Debug)]
pub(crate) struct TextEditor {
    cursor: Cursor,
    view: ScrollableView,
    buffer: Vec<String>,
    file: Option<PathBuf>,
    context: SharedContext,
}
impl TextEditor {
    pub fn new(context: SharedContext) -> Self {
        let area = context.borrow().screen_area.clone();

        Self {
            context,
            file: None,
            cursor: Cursor::default(),
            buffer: vec![String::new()],
            view: ScrollableView::from_area(area, TOTAL_HEADER_HEIGHT),
        }
    }

    pub fn get_file_path(&self) -> &Option<PathBuf> {
        &self.file
    }

    pub fn mark_file_unsaved(&mut self) {
        self.context.borrow_mut().file_context.file_saved = false;
    }

    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        self.file = Some(path);
        let path_ref = self.file.as_ref().expect("just assigned to Some");

        let content = std::fs::read_to_string(path_ref)?;
        self.buffer = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(String::from).collect()
        };
        self.cursor = Cursor::default();
        Ok(())
    }

    pub fn save_file(&mut self, path: &PathBuf) -> Result<()> {
        let content = self.buffer.join("\n");
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn set_to_no_file(&mut self) {
        self.file = None;
        self.buffer.clear();
        self.cursor = Cursor::default();
    }
}

impl Window for TextEditor {
    fn on_insert(&mut self) {
        self.mark_file_unsaved();
    }

    fn on_remove(&mut self) {
        self.mark_file_unsaved();
    }

    fn draw_ui(&mut self, frame: &mut Frame, header: Block) {
        let area = frame.area();
        self.view.update_area(area, TOTAL_HEADER_HEIGHT);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Length(area.height - 1), Length(1)].as_ref())
            .split(area);

        let text = self.view.text_buffer_to_view(&self.cursor, &self.buffer);
        let editor_box = Paragraph::new(text)
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
        default_controls::remove_multi_line(&mut self.cursor, &mut self.buffer);
        Ok(WindowControlReponse::None)
    }

    fn insert(&mut self, insert: crate::key_controller::InsertKind) -> Result<WindowControlReponse> {
        default_controls::insert_multi_line(&mut self.cursor, &mut self.buffer, insert);
        Ok(WindowControlReponse::None)
    }

    fn custom_action(&mut self, action: event::Event) -> Result<Option<SessionEvent>> {
        match action {
            _ => Ok(None),
        }
    }
}
