use std::{
    fs, io,
    path::{Path, PathBuf},
};

use ropey::Rope;

#[cfg(test)]
#[path = "tests/file_content_tests.rs"]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContent {
    path: PathBuf,
    name: String,
    content: Rope,
    dirty: bool,
}

impl FileContent {
    pub fn new(path: impl AsRef<Path>, content: impl Into<Rope>) -> Self {
        let path = path.as_ref().to_path_buf();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<file>")
            .to_string();

        Self {
            path,
            name,
            content: content.into(),
            dirty: false,
        }
    }

    pub fn read_from_path(path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(path)?.replace("\r\n", "\n");
        Ok(Self::new(path, content))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn content(&self) -> &Rope {
        &self.content
    }

    pub fn content_mut(&mut self) -> &mut Rope {
        &mut self.content
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }
}
