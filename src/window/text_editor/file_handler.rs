use anyhow::Result;
use std::path::PathBuf;

use crate::window::text_editor::{Cursor, TextEditor};

impl TextEditor {
    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        let content = std::fs::read_to_string(&path)?;
        self.buffer = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(String::from).collect()
        };
        self.cursor = Cursor::default();
        self.cursor.line = 0;
        self.cursor.offset = 0;
        Ok(())
    }

    pub fn save_file(&mut self, path: &PathBuf) -> Result<()> {
        let content = self.buffer.join("\n");
        std::fs::write(path, content)?;
        Ok(())
    }
}
