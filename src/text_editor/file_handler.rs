use anyhow::Result;
use std::path::PathBuf;
use crate::text_editor::{Cursor, TextEditor};

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
        self.file_path = Some(path);
        Ok(())
    }

    pub fn save_file(&mut self) -> Result<()> {
        let path = self.file_path.as_ref().ok_or_else(|| anyhow::anyhow!("No file open"))?;
        let content = self.buffer.join("\n");
        std::fs::write(&path, content)?;
        self.file_saved = true;
        Ok(())
    }
}