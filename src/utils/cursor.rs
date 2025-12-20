#[derive(Debug, Clone, Copy, Default)]
pub struct Cursor {
    pub line: u16,
    pub offset: u16,
}

impl From<Cursor> for ratatui::layout::Position {
    fn from(value: Cursor) -> Self {
        ratatui::layout::Position::new(value.offset, value.line)
    }
}
