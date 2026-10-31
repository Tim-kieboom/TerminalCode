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
//! file_types = ["md"]          # with "files.text": the files whose text it gets
//! run = ["cargo nextest *"]    # with "process.run": the commands it may run
//! view = false                 # no pane (a plugin that works for other plugins)
//! thread = "own"               # a thread of its own, for heavy work (optional)
//!
//! [[action]]
//! id = "stage"
//! title = "Stage File"
//! keys = "s"
//! ```
//!
//! A plugin may instead be only a language (plugin API 1.12): no `wasm`, no pane, no code,
//! and the grammar is its module (what the installer downloads and checks):
//!
//! ```toml
//! id = "sol-syntax"
//! name = "Sol"
//! version = "0.0.1"
//! abi = "1.12"
//!
//! [language]
//! name = "Sol"
//! grammar = "sol.wasm"          # a tree-sitter grammar built for WebAssembly
//! extensions = ["sol", "solc"]
//! comment = "//"                # how a line is commented (optional)
//! highlights = """
//! (line_comment) @comment
//! """
//! ```
//!
//! This is only the shape of the file; reading it and judging it (does the
//! editor have that version of the API, is the id good) is the editor's.

use serde::{Deserialize, Serialize};

use crate::ActionDecl;

/// Which thread the editor runs a plugin on.
///
/// Unlike the message enums this one has no `Unknown`, on purpose: the thread decides how
/// much of the machine a plugin may take and what the user is asked, so a manifest that
/// names a thread this version does not know is refused rather than run on the wrong one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Thread {
    /// The thread all plugins share: a plugin that takes long stalls the others, so
    /// each message it gets has a small budget.
    #[default]
    Shared,
    /// A thread of its own: a plugin that takes long stalls only itself, so what it is
    /// sent that is big gets a budget that grows with the size (a document to parse).
    /// The user is told when they turn the plugin on. Plugin API 1.5.
    Own,
}

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
    /// The module, a path in the plugin's folder. Left out by a plugin that is only a
    /// [`LanguageDecl`] (plugin API 1.12), whose module is its grammar.
    #[serde(default)]
    pub wasm: String,
    /// The language the plugin adds to the editor: its grammar and how to color it. Plugin
    /// API 1.12.
    #[serde(default)]
    pub language: Option<LanguageDecl>,
    /// What it needs to be allowed to do, by name (see [`crate::Capability`]).
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// The kinds of file whose text it receives with [`crate::Capability::FilesText`],
    /// as extensions without the dot (`["md", "markdown"]`). The most it can get: the
    /// user may narrow it. Without any, it receives no file.
    #[serde(default)]
    pub file_types: Vec<String>,
    /// With [`crate::Capability::ProcessRun`]: the commands it may run, as patterns
    /// (see [`crate::RunPattern`]), such as `cargo nextest *`. Without any, it can run
    /// nothing; the user is shown every one of them when asked to allow the plugin.
    #[serde(default)]
    pub run: Vec<String>,
    /// Whether it has a pane. A plugin that only does work for other plugins (a language
    /// adapter) has none: with `view = false` it is never asked where its pane goes, has no
    /// place and no tab, and what it draws is dropped. Plugin API 1.8; before it, every plugin
    /// has a pane.
    #[serde(default = "has_view_by_default")]
    pub view: bool,
    /// The thread it runs on; the shared one when left out. A plugin that does heavy work
    /// on what it is sent (parsing a document) asks for `"own"`.
    #[serde(default)]
    pub thread: Thread,
    /// The actions it has, for the palette and for keys.
    #[serde(default, rename = "action")]
    pub actions: Vec<ActionDecl>,
}

/// A language a plugin adds: files with its extensions are highlighted by its tree-sitter
/// grammar, which is a module of WebAssembly (`tree-sitter build --wasm`, the same file a Zed
/// extension has) that runs inside the editor's sandbox, with the capture names of
/// `highlights` turned into colors by the theme. Plugin API 1.12.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LanguageDecl {
    /// What a person calls the language.
    pub name: String,
    /// The grammar, a path in the plugin's folder.
    pub grammar: String,
    /// The name the grammar was generated with, which names its entry in the module
    /// (`tree_sitter_<name>`); the language's name in lower case when left out.
    #[serde(default)]
    pub grammar_name: String,
    /// The files it is for, as extensions without the dot. The editor's own languages come first.
    pub extensions: Vec<String>,
    /// How a line is commented, such as `//`; without one `ctrl+/` has nothing to put.
    #[serde(default)]
    pub comment: Option<String>,
    /// The tree-sitter query that names the parts to color (`@keyword`, `@string`, ...), as
    /// text.
    pub highlights: String,
}

impl LanguageDecl {
    /// The name the module's entry has: `grammar_name`, or the language's name in lower case with
    /// anything but letters and digits as `_`.
    pub fn entry_name(&self) -> String {
        match self.grammar_name.is_empty() {
            false => self.grammar_name.clone(),
            true => self
                .name
                .chars()
                .map(|c| match c.is_ascii_alphanumeric() {
                    true => c.to_ascii_lowercase(),
                    false => '_',
                })
                .collect(),
        }
    }
}

fn has_view_by_default() -> bool {
    true
}

impl Manifest {
    /// The file the plugin is installed with and that its hash is of: its module, or the grammar
    /// of a plugin that is only a language.
    pub fn module_file(&self) -> &str {
        match (self.wasm.is_empty(), &self.language) {
            (true, Some(language)) => &language.grammar,
            _ => &self.wasm,
        }
    }

    /// What a person calls the plugin.
    pub fn display_name(&self) -> &str {
        match self.name.is_empty() {
            true => &self.id,
            false => &self.name,
        }
    }
}
