mod app;
pub mod keybinds;
mod layout;
pub mod terminal;
pub mod theme;
pub mod utils;
pub use app::App;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct StartupArgs {
    pub(crate) project_path: PathBuf,
    pub(crate) _flags: (),
}

impl StartupArgs {
    pub fn new(project_path: PathBuf) -> Self {
        Self {
            project_path,
            _flags: (),
        }
    }

    pub fn project_path(&self) -> &std::path::Path {
        &self.project_path
    }

    pub fn add_flag(&mut self, _flag: ()) {}
}
