mod app;
pub mod keybinds;
pub mod terminal;
pub mod theme;
pub mod utils;
pub use app::App;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct StartupArgs {
    pub(crate) path: PathBuf,
    pub(crate) _flags: (),
}

impl StartupArgs {
    pub fn new(path: PathBuf) -> Self {
        Self { path, _flags: () }
    }

    pub fn config_dir(&self) -> &std::path::Path {
        &self.path
    }

    pub fn add_flag(&mut self, _flag: ()) {}
}
