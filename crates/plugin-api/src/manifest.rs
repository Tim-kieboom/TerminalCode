//! The manifest: the file that says what a plugin is and what it needs. It is
//! `manifest.toml` in the plugin's folder, next to its module:
//!
//! ```toml
//! id = "git-viewer"            # the folder name; letters, digits, `-` and `_`
//! name = "Git Viewer"
//! version = "0.1.0"
//! abi = "1.2"                  # the plugin API it was built for
//! requires = "0.7.0"           # the oldest TerminalCode it works with (optional)
//! wasm = "git_viewer.wasm"     # the module, in the same folder
//! capabilities = ["git.read", "git.write"]
//!
//! [[action]]
//! id = "stage"
//! title = "Stage File"
//! keys = "s"
//! ```
//!
//! This is only the shape of the file; reading it and judging it (does the
//! editor have that version of the API, is the id good) is the editor's.

use serde::{Deserialize, Serialize};

use crate::ActionDecl;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Manifest {
    /// What the plugin is called in the editor: its pane, its actions (`id:action`)
    /// and its folder.
    pub id: String,
    /// What a person calls it; the id when left out.
    #[serde(default)]
    pub name: String,
    pub version: String,
    /// The version of the plugin API it was built for, as `major.minor`.
    pub abi: String,
    /// The oldest version of the editor it works with, as `major.minor.patch`.
    #[serde(default)]
    pub requires: Option<String>,
    /// The module, a path in the plugin's folder.
    pub wasm: String,
    /// What it needs to be allowed to do, by name (see [`crate::Capability`]).
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// The actions it has, for the palette and for keys.
    #[serde(default, rename = "action")]
    pub actions: Vec<ActionDecl>,
}

impl Manifest {
    /// What a person calls the plugin.
    pub fn display_name(&self) -> &str {
        match self.name.is_empty() {
            true => &self.id,
            false => &self.name,
        }
    }
}
