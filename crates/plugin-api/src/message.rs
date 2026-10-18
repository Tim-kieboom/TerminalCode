//! The messages: JSON, one object each, with a `type` field.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::MAX_MESSAGE;

/// Why a message could not be written or read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MessageError {
    #[error("a message of {0} bytes, more than the {MAX_MESSAGE} allowed")]
    TooBig(usize),
    #[error("not a message: {0}")]
    Invalid(String),
}

/// What the editor tells a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToPlugin {
    /// The plugin was loaded, or started again after it trapped: draw.
    Start,
    /// Something changed that the plugin may want to draw again.
    Refresh,
    /// One of the plugin's actions was run (from a key, the palette or a
    /// binding of the user's): `action` is the `id` of its [`ActionDecl`].
    Action { action: String },
    /// Text was typed (or pasted) into the plugin's input view (see
    /// [`WireView::Input`]). Only sent while that view has the keyboard.
    Text { text: String },
    /// A key that is not text was pressed in the plugin's input view: `key` is
    /// `backspace`, `delete`, `enter`, `tab`, `left`, `right`, `up`, `down`,
    /// `home`, `end`, `page_up` or `page_down`, or a single character when
    /// `ctrl` or `alt` was held with it.
    Key {
        key: String,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        shift: bool,
    },
    /// The user pressed `esc` in the input view: the editor takes the keyboard
    /// back. The plugin should stop showing the input.
    InputEnded,
    /// What git says changed: sent to a plugin with [`Capability::GitRead`] after
    /// it starts, and whenever the repository's state differs from before. It
    /// is a summary; the files are asked for with [`FromPlugin::GitEntries`].
    GitChanged { summary: GitSummary },
    /// The project is not in a git repository (any more), or `git` is not there:
    /// what the plugin has of it is out of date and should be dropped.
    GitUnavailable,
    /// The answer to [`FromPlugin::GitEntries`]: the page asked for. It always
    /// comes from the latest state of the repository, and `generation` says which
    /// that is: when it is not the generation of the pages the plugin already
    /// has, the plugin starts over.
    GitEntries {
        request: u64,
        generation: u64,
        section: GitSection,
        /// Where the page starts, and how many entries the section has in all.
        offset: usize,
        total: usize,
        entries: Vec<GitEntry>,
    },
    /// The request `request` was done.
    Done { request: u64 },
    /// The request `request` was not done.
    Refused { request: u64, reason: Refusal },
    /// A kind of message a later version of the editor sends.
    #[serde(other)]
    Unknown,
}

/// What a plugin tells the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum FromPlugin {
    /// Replace what the plugin's pane shows.
    SetView { view: WireView },
    /// Asks for a page of the files git lists in a section (needs
    /// [`Capability::GitRead`]). The answer is [`ToPlugin::GitEntries`] with the
    /// same `request`, or [`ToPlugin::Refused`]. A `limit` over [`MAX_PAGE`] is
    /// lowered to it.
    GitEntries {
        request: u64,
        section: GitSection,
        #[serde(default)]
        offset: usize,
        limit: usize,
    },
    /// Stages the file (needs [`Capability::GitWrite`]): `path` is from the root
    /// of the repository, as in [`GitEntry::path`]. The answer is
    /// [`ToPlugin::Done`] or [`ToPlugin::Refused`].
    GitStage { request: u64, path: String },
    /// Takes the file out of the staged changes (needs [`Capability::GitWrite`]).
    GitUnstage { request: u64, path: String },
    /// A kind of message a later version of the plugin API has.
    #[serde(other)]
    Unknown,
}

/// What a plugin's pane shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum WireView {
    /// Lines of text, each in the style of a theme slot.
    Lines {
        #[serde(default)]
        lines: Vec<WireLine>,
    },
    /// A list with one row highlighted.
    List {
        #[serde(default)]
        items: Vec<String>,
        #[serde(default)]
        selected: Option<usize>,
    },
    /// A line of text the user types into: the prompt, then the text the plugin
    /// holds, with the cursor at character `cursor` of it. While a pane shows
    /// this and has the keyboard, every key goes to the plugin as [`ToPlugin::Text`]
    /// or [`ToPlugin::Key`] (except `esc`, which the editor keeps for itself and
    /// answers with [`ToPlugin::InputEnded`]), and the plugin sends a new view for
    /// every change.
    Input {
        #[serde(default)]
        prompt: String,
        #[serde(default)]
        value: String,
        #[serde(default)]
        cursor: usize,
    },
    /// A kind of view a later version of the plugin API has.
    #[serde(other)]
    Unknown,
}

/// One line of text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WireLine {
    #[serde(default)]
    pub text: String,
    /// The theme slot that styles it; a slot the theme does not have is plain text.
    #[serde(default)]
    pub slot: String,
}

impl WireLine {
    pub fn new(text: impl Into<String>, slot: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            slot: slot.into(),
        }
    }
}

impl WireView {
    pub fn lines(lines: impl IntoIterator<Item = WireLine>) -> Self {
        Self::Lines {
            lines: lines.into_iter().collect(),
        }
    }

    pub fn list(items: impl IntoIterator<Item = String>, selected: Option<usize>) -> Self {
        Self::List {
            items: items.into_iter().collect(),
            selected,
        }
    }
}

impl ToPlugin {
    pub fn action(action: impl Into<String>) -> Self {
        Self::Action {
            action: action.into(),
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn key(key: impl Into<String>, ctrl: bool, alt: bool, shift: bool) -> Self {
        Self::Key {
            key: key.into(),
            ctrl,
            alt,
            shift,
        }
    }
}

/// An action a plugin has: it shows in the command palette and can have keys.
/// The manifest lists them; the editor sends [`ToPlugin::Action`] when one runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ActionDecl {
    /// What the plugin knows it by; no `:` in it.
    pub id: String,
    /// What the palette shows after the plugin's name.
    #[serde(default)]
    pub title: String,
    /// Keys that run it while the plugin's pane has the keyboard, such as `s` or
    /// `ctrl+k ctrl+s`, which the user can change in their keymap.
    #[serde(default)]
    pub keys: Option<String>,
}

impl ActionDecl {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            keys: None,
        }
    }

    /// The same action with default keys.
    #[must_use]
    pub fn with_keys(mut self, keys: impl Into<String>) -> Self {
        self.keys = Some(keys.into());
        self
    }
}

impl WireView {
    pub fn input(prompt: impl Into<String>, value: impl Into<String>, cursor: usize) -> Self {
        Self::Input {
            prompt: prompt.into(),
            value: value.into(),
            cursor,
        }
    }
}

impl FromPlugin {
    pub fn set_view(view: WireView) -> Self {
        Self::SetView { view }
    }
}

/// What a plugin may ask the editor for. The manifest lists what the plugin
/// uses and the editor refuses the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Capability {
    /// Read what git says of the project: the branch, the files that changed.
    #[serde(rename = "git.read")]
    GitRead,
    /// Change the repository: stage and unstage files.
    #[serde(rename = "git.write")]
    GitWrite,
    /// A capability a later version of the plugin API has.
    #[serde(other)]
    Unknown,
}

impl Capability {
    /// Every capability this version has.
    pub const ALL: [Self; 2] = [Self::GitRead, Self::GitWrite];

    /// The name a manifest writes it by.
    pub const fn name(self) -> &'static str {
        match self {
            Self::GitRead => "git.read",
            Self::GitWrite => "git.write",
            Self::Unknown => "unknown",
        }
    }

    /// The capability a manifest names, or `None` for a name this version does not
    /// have (a plugin built for a later version, or a misspelling).
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.name() == name)
    }
}

/// The files git lists, in the groups a Git pane shows. A file changed both in
/// the index and since is in two of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GitSection {
    /// Changes that are staged.
    Staged,
    /// Changes to files git knows that are not staged.
    Changes,
    /// Files git does not know.
    Untracked,
    /// Files a merge left with conflicts.
    Conflicts,
    #[serde(other)]
    Unknown,
}

/// What happened to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GitKind {
    Modified,
    TypeChanged,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Conflicted,
    #[serde(other)]
    Unknown,
}

/// One file in a [`GitSection`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GitEntry {
    /// From the root of the repository, with `/` between the parts.
    pub path: String,
    /// Where a renamed or copied file came from.
    #[serde(default)]
    pub original: Option<String>,
    pub kind: GitKind,
}

impl GitEntry {
    pub fn new(path: impl Into<String>, kind: GitKind) -> Self {
        Self {
            path: path.into(),
            original: None,
            kind,
        }
    }

    #[must_use]
    pub fn renamed_from(mut self, original: impl Into<String>) -> Self {
        self.original = Some(original.into());
        self
    }
}

/// How the repository is, in a few numbers: what [`ToPlugin::GitChanged`] says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GitSummary {
    /// Counts the states of the repository that differed from the one before; the
    /// pages of [`ToPlugin::GitEntries`] say which of them they are from.
    pub generation: u64,
    /// The branch checked out; `None` when `HEAD` is detached.
    #[serde(default)]
    pub branch: Option<String>,
    /// The commit `HEAD` is at; `None` before the first commit.
    #[serde(default)]
    pub commit: Option<String>,
    /// The branch it follows, such as `origin/main`.
    #[serde(default)]
    pub upstream: Option<String>,
    #[serde(default)]
    pub ahead: u32,
    #[serde(default)]
    pub behind: u32,
    /// How many files each [`GitSection`] has.
    #[serde(default)]
    pub staged: usize,
    #[serde(default)]
    pub changes: usize,
    #[serde(default)]
    pub untracked: usize,
    #[serde(default)]
    pub conflicts: usize,
}

impl GitSummary {
    /// A summary of generation `generation` with nothing in it.
    pub fn empty(generation: u64) -> Self {
        Self {
            generation,
            branch: None,
            commit: None,
            upstream: None,
            ahead: 0,
            behind: 0,
            staged: 0,
            changes: 0,
            untracked: 0,
            conflicts: 0,
        }
    }
}

/// Why a request was not done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Refusal {
    /// The plugin's manifest does not list the capability the request needs.
    Denied { capability: Capability },
    /// The project is not in a git repository, or `git` is not there.
    NoRepository,
    /// An argument that cannot be right: a path outside the repository, for one.
    BadArgument {
        #[serde(default)]
        detail: String,
    },
    /// `git` itself said no; `detail` is what it said.
    Failed {
        #[serde(default)]
        detail: String,
    },
    #[serde(other)]
    Unknown,
}

macro_rules! bytes {
    ($message:ty) => {
        impl $message {
            /// The message as the bytes to send.
            pub fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
                let bytes = serde_json::to_vec(self)
                    .map_err(|error| MessageError::Invalid(error.to_string()))?;
                match bytes.len() > MAX_MESSAGE {
                    true => Err(MessageError::TooBig(bytes.len())),
                    false => Ok(bytes),
                }
            }

            /// Reads a message that was sent. A kind of message that is not known
            /// is `Unknown`, not an error; one that is not JSON, or has no `type`,
            /// is an error.
            pub fn from_bytes(bytes: &[u8]) -> Result<Self, MessageError> {
                if bytes.len() > MAX_MESSAGE {
                    return Err(MessageError::TooBig(bytes.len()));
                }
                serde_json::from_slice(bytes)
                    .map_err(|error| MessageError::Invalid(error.to_string()))
            }
        }
    };
}

bytes!(ToPlugin);
bytes!(FromPlugin);
