use std::path::Path;

use ratatui::{Terminal, backend::Backend};

use crate::app::Sources;
use crate::buffer::Buffer;
use crate::editor::Editor;
use crate::error::{IdeError, IdeResult};
use crate::state::AppState;

mod action;
mod app;
mod buffer;
mod component;
mod editor;
pub mod error;
mod event;
mod keymap;
mod state;
#[cfg(test)]
mod tests;
mod ui;

pub fn run<B>(mut terminal: Terminal<B>, path: Option<&Path>) -> IdeResult
where
    B: Backend,
    B::Error: Into<IdeError>,
{
    let editor = match path {
        Some(path) => Editor::new(Buffer::open_or_new(path)?),
        None => Editor::default(),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let (sources, _events) = Sources::spawn();
        app::run(&mut terminal, sources, AppState::new(editor)).await
    })
}
