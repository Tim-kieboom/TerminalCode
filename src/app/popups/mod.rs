//! The popups: each one opens from an action, takes all keys while it is open,
//! and does something when the user picks or closes it. Opening one closes
//! the one that was open (see [`Popup`](crate::state::Popup)).

use super::App;
use crate::components::search::Options as SearchOptions;

mod find;
mod finder;
mod palette;
mod quit;
mod search;

/// What a search box was last used for, so opening it again starts there.
#[derive(Debug, Default)]
pub(super) struct SavedQuery {
    query: String,
    options: SearchOptions,
}

impl SavedQuery {
    fn new(query: &str, options: SearchOptions) -> Self {
        Self {
            query: query.to_owned(),
            options,
        }
    }
}

/// The last project search and the last find in a file.
#[derive(Debug, Default)]
pub(super) struct Remembered {
    search: SavedQuery,
    find: SavedQuery,
}

impl App {
    /// The project: the explorer's root, else the working directory.
    pub(super) fn project_root(&self) -> std::path::PathBuf {
        match self.state.explorer().root_path() {
            Some(root) => root.to_path_buf(),
            None => std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        }
    }
}
