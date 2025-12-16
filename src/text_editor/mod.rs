use std::path::PathBuf;

pub mod key_controller;
pub mod file_handler;

#[derive(Debug, Default)]
pub(crate) struct TextEditor {
    pub(super) buffer: Vec<String>,
    pub(super) cursor: Cursor,
    pub(super) file_path: Option<PathBuf>,
    pub(super) file_saved: bool,
}
impl TextEditor {
    pub fn new() -> Self {
        Self{
            file_saved: true,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Cursor {
    pub(super) line: u16,
    pub(super) offset: u16,
}
impl Into<ratatui::layout::Position> for Cursor {
    fn into(self) -> ratatui::layout::Position {
        ratatui::layout::Position::new(self.offset, self.line)
    }
}