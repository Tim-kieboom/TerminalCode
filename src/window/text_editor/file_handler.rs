use anyhow::Result;
use std::path::PathBuf;

use crate::window::text_editor::{Cursor, TextEditor};

impl TextEditor {
    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        self.file = Some(path);
        let path_ref = self.file
            .as_ref()
            .expect("just assigned to Some");
        
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
