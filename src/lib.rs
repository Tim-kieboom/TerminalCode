use std::ffi::OsString;
use std::path::PathBuf;

use ratatui::{Terminal, backend::Backend};

use crate::app::state::AppState;
use crate::app::{App, Sources};
use crate::buffer::Buffer;
use crate::clipboard::{Clipboard, System};
use crate::components::editor::Editor;
use crate::error::{IdeError, IdeResult};
use crate::terminal::Capabilities;
use crate::watcher::FsWatcher;

mod app;
mod buffer;
mod clipboard;
mod components;
mod config;
mod entries;
pub mod error;
mod event;
mod keymap;
mod paths;
mod pty;
mod removal;
mod syntax;
pub mod terminal;
#[cfg(test)]
mod tests;
mod ui;
mod watcher;

pub fn run<B>(mut terminal: Terminal<B>, paths: &[PathBuf], capabilities: Capabilities) -> IdeResult
where
    B: Backend,
    B::Error: Into<IdeError>,
{
    let (project, files) = project_and_files(paths);
    let mut state = initial_state(&files)?;
    let root = project.or_else(|| std::env::current_dir().ok());
    if let Some(error) = root.and_then(|root| state.open_project(&root).err()) {
        state.notify_error(error.to_string());
    }

    state.learn_terminal_background(terminal::query_background);
    if let Some(hint) = state.theme().terminal_hint() {
        state.notify(hint);
    }

    let layout_path = config::user_layout_path();
    let loaded_layout = config::load_layout(layout_path.as_deref());
    state.set_layout(loaded_layout.layout);
    if let Some(warning) = loaded_layout.warning {
        state.notify_error(warning);
    }

    let keymap_path = config::user_keymap_path();
    let loaded = config::load_keymap(keymap_path.as_deref(), capabilities.keyboard);
    if let Some(warning) = loaded.warning {
        state.notify_error(warning);
    }

    let clipboard = Clipboard::new(System::detect());
    let mut app = App::with_keymap(state, loaded.keymap).with_clipboard(clipboard);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let (sources, events) = Sources::spawn();
        app = app.with_events(events.clone());
        match FsWatcher::new(events) {
            Ok(watcher) => app = app.with_watcher(watcher),
            Err(error) => app.state_mut().notify_error(error.to_string()),
        }

        app.run(&mut terminal, sources).await
    })
}

/// The files to open: every argument after the program name. A leading `--`
/// marks the end of options and is skipped.
pub fn paths_from_args(args: impl Iterator<Item = OsString>) -> Vec<PathBuf> {
    let mut args = args.skip(1).peekable();
    if args.peek().is_some_and(|first| first == "--") {
        args.next();
    }
    args.map(PathBuf::from).collect()
}

/// Splits the command line into the project directory, if one is named (the
/// first directory), and the files to open.
fn project_and_files(paths: &[PathBuf]) -> (Option<PathBuf>, Vec<PathBuf>) {
    let (dirs, files): (Vec<_>, Vec<_>) = paths.iter().cloned().partition(|path| path.is_dir());
    (dirs.into_iter().next(), files)
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
