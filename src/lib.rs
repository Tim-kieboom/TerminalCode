mod app;
pub mod theme;
pub mod utils;
pub mod terminal;
pub mod keybinds;
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

    pub fn add_flag(&mut self, _flag: ()) {}
}
