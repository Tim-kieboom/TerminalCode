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
use trcode_breakpoints as breakpoints;
use trcode_buffer as buffer;
use trcode_clipboard as clipboard;
use trcode_dap as dap;
use trcode_files as files;
use trcode_ids as ids;
use trcode_jumplist as jumplist;
use trcode_launch as launch;
use trcode_lsp as lsp;
use trcode_plugin_api as plugin_api;
use trcode_pty as pty;
use trcode_syntax as syntax;
use trcode_ui as ui;
mod cli;
mod components;
mod config;
mod direnv;
pub mod error;
mod event;
mod git;
mod keymap;
mod persist;
// Nothing loads plugins yet (that comes with the manifest and the prompt that
// enables one, todo plugin step 5), so outside the tests most of it is unused.
#[cfg_attr(not(test), allow(dead_code))]
mod plugins;
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
    note: Option<String>,
) -> IdeResult<Option<PathBuf>>
where
    B: Backend,
    B::Error: Into<IdeError>,
{
    let mut state = new_state(paths)?;
    if let Some(note) = note {
        state.notify(note);
    }

    let config_dir = config::user_config_dir();
    let user = startup::load_user_config(&mut state, config_dir.as_deref(), capabilities.keyboard);

    state.learn_terminal_background(terminal::query_background);
    if let Some(hint) = state.theme().terminal_hint() {
        state.notify(hint);
    }

    let clipboard = Clipboard::new(System::detect());
    let mut app = App::new(state)
        .with_keymap(user.keymap)
        .with_recent(user.recent)
        .with_clipboard(clipboard)
        .with_config_dir(config_dir)
        .with_saved_state(user.saved)
        .with_settings(user.settings, user.theme)
        .with_keyboard_support(capabilities.keyboard);

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

fn new_state(paths: &[PathBuf]) -> Result<AppState, IdeError> {
    let (project, files) = project_and_files(paths);
    let mut state = initial_state(&files)?;
    let Some(root) = project.or_else(|| std::env::current_dir().ok()) else {
        return Ok(state);
    };

    if let Some(error) = state.open_project(&root).err() {
        state.notify_error(error.to_string());
    }
    Ok(state)
}

pub use cli::{Invocation, help_text, version_text};

/// The folder `--recent` opens: the newest of the recent project folders that is still
/// there, and what to tell the user when it was not the newest or there is none (then
/// the current folder is the project).
pub fn recent_project() -> (Option<PathBuf>, Option<String>) {
    let dir = config::user_config_dir();
    let (recent, _) = persist::recent::load(dir.as_deref());
    match recent.newest_folder() {
        (Some(folder), 0) => (Some(folder), None),
        (Some(folder), gone) => {
            let note = format!(
                "--recent: {gone} newer project folder(s) are gone; opened {}",
                folder.display()
            );
            (Some(folder), Some(note))
        }
        (None, _) => (
            None,
            Some("--recent: no recent project folder is there; opened the current folder".into()),
        ),
    }
}

/// Loads the environment of the project's `.envrc` through `direnv` into this process,
/// so that what the editor starts sees it. Call it before anything else is started.
/// It returns what to tell the user, if anything: that it loaded, or why it did not.
pub fn load_project_environment(paths: &[PathBuf]) -> Option<String> {
    let project = project_and_files(paths)
        .0
        .or_else(|| std::env::current_dir().ok())?;
    let envrc = direnv::find_envrc(&project)?;

    // Nix can take a while the first time, and the screen is still the shell's.
    eprintln!("trcode: loading {} with direnv...", envrc.display());
    match direnv::load(&envrc, &project) {
        Ok(changes) => {
            direnv::apply(&changes);
            Some(format!("loaded the environment of {}", envrc.display()))
        }
        Err(message) => Some(message),
    }
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
