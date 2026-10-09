pub mod action;
pub mod mouse;
use std::path::PathBuf;

use crate::components::PluginViewId;
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
    /// The file watcher reported a problem.
    WatchFailed(String),
}
