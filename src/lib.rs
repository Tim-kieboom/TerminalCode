mod app;
pub mod keybinds;
pub mod terminal;
pub use app::App;
use std::path::PathBuf;

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
