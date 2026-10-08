use std::path::Path;

use ratatui::{Terminal, backend::Backend};

use crate::app::{App, Sources};
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::error::{IdeError, IdeResult};
use crate::state::AppState;
use crate::terminal::Capabilities;

mod action;
mod app;
mod buffer;
mod components;
mod config;
pub mod error;
mod event;
mod keymap;
mod state;
pub mod terminal;
#[cfg(test)]
mod tests;
mod ui;

/// State at startup: the file named on the command line, if any. A path that does
/// not exist yet opens an empty buffer and says so in the status bar, so a
/// mistyped path is noticed.
fn initial_state(path: Option<&Path>) -> IdeResult<AppState> {
    let Some(path) = path else {
        return Ok(AppState::default());
    };
    let is_new = !path.exists();
    let mut state = AppState::new(Editor::new(Buffer::open_or_new(path)?));
    if is_new {
        state.set_status(format!("new file: {}", path.display()));
    }
    Ok(state)
}

pub fn run<B>(
    mut terminal: Terminal<B>,
    path: Option<&Path>,
    capabilities: Capabilities,
) -> IdeResult
where
    B: Backend,
    B::Error: Into<IdeError>,
{
    let mut state = initial_state(path)?;
    let loaded = config::load_keymap(config::user_keymap_path().as_deref(), capabilities.keyboard);
    if let Some(warning) = loaded.warning {
        state.set_status(warning);
    }
    let app = App::with_keymap(state, loaded.keymap);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let (sources, _events) = Sources::spawn();
        app::run(&mut terminal, sources, app).await
    })
}
