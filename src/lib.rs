use std::ffi::OsString;
use std::path::PathBuf;

use ratatui::{Terminal, backend::Backend};

use crate::app::state::AppState;
use crate::app::{App, Sources};
use crate::buffer::Buffer;
use crate::clipboard::{Clipboard, System};
use crate::components::editor::Editor;
use crate::error::{IdeError, IdeResult};
use crate::files::watcher::FsWatcher;
use crate::terminal::Capabilities;

mod app;
use trcode_buffer as buffer;
use trcode_clipboard as clipboard;
use trcode_files as files;
use trcode_ids as ids;
use trcode_jumplist as jumplist;
use trcode_lsp as lsp;
use trcode_pty as pty;
use trcode_syntax as syntax;
use trcode_ui as ui;
mod cli;
mod components;
mod config;
pub mod error;
mod event;
mod keymap;
mod persist;
mod settings;
mod startup;
pub mod terminal;
#[cfg(test)]
mod tests;


/// Runs the editor until it ends. It returns the folder to start again in when the
/// user asked to open one (the caller restarts the program there), and `None` for
/// a plain quit.
pub fn run<B>(
    mut terminal: Terminal<B>,
    paths: &[PathBuf],
    capabilities: Capabilities,
) -> IdeResult<Option<PathBuf>>
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

    let config_dir = config::user_config_dir();
    let user = startup::load_user_config(&mut state, config_dir.as_deref(), capabilities.keyboard);

    state.learn_terminal_background(terminal::query_background);
    if let Some(hint) = state.theme().terminal_hint() {
        state.notify(hint);
    }

    let clipboard = Clipboard::new(System::detect());
    let mut app = App::with_keymap(state, user.keymap)
        .with_clipboard(clipboard)
        .with_keyboard_support(capabilities.keyboard)
        .with_config_dir(config_dir)
        .with_settings(user.settings, user.theme)
        .with_saved_state(user.saved)
        .with_recent(user.recent);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let (sources, events) = Sources::spawn();
        app = app.with_events(events.clone());
        app.start_terminal_if_shown();
        match FsWatcher::new(events) {
            Ok(watcher) => app = app.with_watcher(watcher),
            Err(error) => app.state_mut().notify_error(error.to_string()),
        }

        app.run(&mut terminal, sources).await
    })
}

pub use cli::{Invocation, help_text, version_text};

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
