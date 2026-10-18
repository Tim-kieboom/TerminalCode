pub mod action;
pub mod mouse;
use std::path::PathBuf;

use crate::components::popups::search::Hit;
use crate::ui::PluginViewId;
use crate::ui::view::ViewNode;

/// Non-input events delivered to the app loop (PTY output, file watcher,
/// parse results, plugin messages, ...). Terminal input has its own channel
/// so that it is never queued behind these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    SetPluginView {
        id: PluginViewId,
        content: ViewNode,
    },
    /// Files or directories changed outside the editor.
    FilesChanged(Vec<PathBuf>),
    /// Files the file finder's walk found, as paths from the project root.
    FinderBatch {
        scan: u64,
        files: Vec<PathBuf>,
    },
    /// The file finder's walk is over.
    FinderDone {
        scan: u64,
        unreadable: usize,
    },
    /// Matching lines the project search found.
    SearchBatch {
        search: u64,
        hits: Vec<Hit>,
    },
    /// The project search is over.
    SearchDone {
        search: u64,
        files: usize,
        truncated: bool,
    },
    /// The shell in the terminal pane printed something, or ended.
    TerminalChanged,
    /// The syntax worker found the colors of a document.
    Highlighted(crate::syntax::Output),
    /// The file watcher reported a problem.
    WatchFailed(String),
    /// A language server finished starting, said something, or died.
    LspChanged,
    /// Something a plugin did, or happened to it.
    Plugin(crate::plugins::Notice),
    /// A refresh of what git says is over, and how long it took.
    GitStatus {
        took: std::time::Duration,
        result: Result<crate::git::Status, crate::git::GitError>,
    },
    /// The index text of a document's file, as the hash of each line, to mark the
    /// lines that differ from it.
    GitBase {
        document: crate::ids::DocumentId,
        request: u64,
        result: Result<Option<Vec<u64>>, crate::git::GitError>,
    },
    /// A save as root is over: it worked, or why not.
    RootSaved(Result<(), crate::files::rootsave::RootError>),
}

impl From<trcode_files::watcher::WatchEvent> for Event {
    fn from(event: trcode_files::watcher::WatchEvent) -> Self {
        match event {
            trcode_files::watcher::WatchEvent::Changed(paths) => Self::FilesChanged(paths),
            trcode_files::watcher::WatchEvent::Failed(message) => Self::WatchFailed(message),
        }
    }
}
