use std::ffi::OsString;
use std::path::PathBuf;

use ratatui::{Terminal, backend::Backend};

use crate::app::{App, Sources};
use crate::buffer::Buffer;
use crate::clipboard::{Clipboard, System};
use crate::components::editor::Editor;
use crate::error::{IdeError, IdeResult};
use crate::state::AppState;
use crate::terminal::Capabilities;

mod app;
mod buffer;
mod clipboard;
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

/// The files to open: every argument after the program name. A leading `--`
/// marks the end of options and is skipped.
pub fn paths_from_args(args: impl Iterator<Item = OsString>) -> Vec<PathBuf> {
    let mut args = args.skip(1).peekable();
    if args.peek().is_some_and(|first| first == "--") {
        args.next();
    }
    args.map(PathBuf::from).collect()
}

/// State at startup: one tab per file named on the command line, the first
/// one active. A path that does not exist yet opens an empty buffer and says
/// so in the status bar, so a mistyped path is noticed.
fn initial_state(paths: &[PathBuf]) -> IdeResult<AppState> {
    let Some((first, rest)) = paths.split_first() else {
        return Ok(AppState::default());
    };

    let buffer = Buffer::open_or_new(first)?;
    let mut state = AppState::new(Editor::new(buffer));
    for path in rest {
        state
            .workspace_mut()
            .open_buffer(Buffer::open_or_new(path)?);
    }
    state.workspace_mut().activate_tab(0);

    let new_files: Vec<_> = paths
        .iter()
        .filter(|path| !path.exists())
        .map(|path| path.display().to_string())
        .collect();

    if !new_files.is_empty() {
        state.notify(format!("new file: {}", new_files.join(", ")));
    }
    Ok(state)
}

pub fn run<B>(mut terminal: Terminal<B>, paths: &[PathBuf], capabilities: Capabilities) -> IdeResult
where
    B: Backend,
    B::Error: Into<IdeError>,
{
    let mut state = initial_state(paths)?;
    state.learn_terminal_background(terminal::query_background);
    if let Some(hint) = state.theme().terminal_hint() {
        state.notify(hint);
    }
    let loaded = config::load_keymap(config::user_keymap_path().as_deref(), capabilities.keyboard);
    if let Some(warning) = loaded.warning {
        state.notify_error(warning);
    }

    let clipboard = Clipboard::new(System::detect());
    let app = App::with_keymap(state, loaded.keymap).with_clipboard(clipboard);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let (sources, _events) = Sources::spawn();
        app::run(&mut terminal, sources, app).await
    })
}
