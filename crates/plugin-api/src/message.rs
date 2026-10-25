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
    /// The user clicked row `row` of the plugin's [`WireView::Rows`]: the index of
    /// the line in `lines`, as the plugin sent them.
    Click { row: usize },
    /// The user right-clicked row `row` of the plugin's [`WireView::Rows`] (the index of the line
    /// in `lines`, as for [`ToPlugin::Click`]). The plugin answers with [`FromPlugin::Menu`], the
    /// items of the menu to show at the pointer, or with nothing for no menu. Plugin API 1.10.
    ContextClick { row: usize },
    /// The size of the plugin's pane in cells, inside its frame: sent when the pane
    /// is on screen (and when its size changes), so that a plugin can wrap its text.
    /// A plugin that has not been told assumes 80 columns. Plugin API 1.5.
    Size { width: u16, height: u16 },
    /// The mouse wheel was turned over the plugin's pane: `lines` is how far, down when
    /// positive (a notch is 3). The plugin decides what to scroll. Plugin API 1.5.
    Scroll { lines: i16 },
    /// The user pressed the button with this `id` of a [`WireView::Buttons`] (a
    /// button that is not enabled is never reported). Plugin API 1.4.
    Press { button: String },
    /// The user clicked the line to type into ([`WireView::Input`]), at character
    /// `column` of its text (the end of the text when they clicked past it): the
    /// plugin usually makes the line active and puts the cursor there. Plugin API 1.4.
    InputClick { column: usize },
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
    /// The saved text of the file in front in the editor, for a plugin with
    /// [`Capability::FilesText`], when the file is one of the plugin's `file_types`:
    /// sent when the tab in front changes, after the file is saved, and when the file
    /// changes on disk. `path` is from the root of the repository (the project's folder
    /// when there is none) with `/`, as [`FromPlugin::OpenFile`] takes it (the whole
    /// path for a file outside it). `text` is what is on disk, not the buffer, and `modified`
    /// says that the buffer has changes that are not saved. At most
    /// [`crate::MAX_FILE_TEXT`] bytes; plugin API 1.5.
    FileText {
        path: String,
        text: String,
        #[serde(default)]
        modified: bool,
    },
    /// The file in front is not one the plugin gets (another kind, no file, not UTF-8):
    /// what it shows of the last [`ToPlugin::FileText`] is out of date. Plugin API 1.5.
    FileCleared,
    /// The file in front is one of the plugin's, but has more than
    /// [`crate::MAX_FILE_TEXT`] bytes (`bytes`): its text is not sent. Plugin API 1.5.
    FileTooLarge { path: String, bytes: u64 },
    /// Another plugin calls this one (needs [`Capability::PluginServe`]): `from` is
    /// the id of the caller, `request` the caller's number for the call, `method`
    /// what is asked and `payload` its arguments, text the two plugins agree on
    /// (usually JSON; the editor does not read it). Answer with
    /// [`FromPlugin::Reply`], once or in many parts, the last with `last` set.
    /// Plugin API 1.6.
    Call {
        from: String,
        request: u64,
        method: String,
        payload: String,
    },
    /// A part of the answer to [`FromPlugin::Call`] with the same `request`, from
    /// the plugin it was made to. After the part with `last` set no more come.
    /// Plugin API 1.6.
    Reply {
        from: String,
        request: u64,
        payload: String,
        #[serde(default)]
        last: bool,
    },
    /// The caller `from` stopped its call `request` ([`FromPlugin::CallCancel`]), or
    /// went away (it trapped or was turned off): stop working on it. Whatever the
    /// plugin still says about it is dropped. Plugin API 1.6.
    CallCancelled { from: String, request: u64 },
    /// Text a command that the plugin started with [`FromPlugin::Run`] wrote, in the
    /// order it wrote it on each `stream`; at most [`crate::MAX_RUN_CHUNK`] bytes, cut
    /// at a line break when there is one (bytes that are not UTF-8 are replaced). Plugin
    /// API 1.7.
    RunOutput {
        request: u64,
        stream: RunStream,
        text: String,
    },
    /// The command `request` ended: with the exit `code` (`None` when it was stopped by
    /// a signal, which is what a stop is), `cancelled` when [`FromPlugin::RunCancel`] or
    /// the user stopped it, and `truncated` when it wrote more than
    /// [`crate::MAX_RUN_OUTPUT`] and the rest of its output was dropped. The last
    /// message about the request. Plugin API 1.7.
    RunDone {
        request: u64,
        code: Option<i32>,
        #[serde(default)]
        cancelled: bool,
        #[serde(default)]
        truncated: bool,
    },
    /// Whether the user trusts the folder that is open, which decides if commands
    /// are run ([`Refusal::Untrusted`]): sent to a plugin with
    /// [`Capability::ProcessRun`] when it starts and whenever the answer changes. Plugin
    /// API 1.7.
    Trust { trusted: bool },
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
    /// Opens a file in the editor (needs [`Capability::EditorOpen`]): `path` is
    /// from the root of the repository, as in [`GitEntry::path`]. The answer is
    /// [`ToPlugin::Done`] or [`ToPlugin::Refused`]. With a `line` (counted from 1; and a
    /// `column`, also from 1) the cursor is put there, held to what the file has, and the
    /// keyboard goes to the editor; without one the file is only opened. `line` and
    /// `column` are plugin API 1.9.
    OpenFile {
        request: u64,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        column: Option<u32>,
    },
    /// Stages the file (needs [`Capability::GitWrite`]): `path` is from the root
    /// of the repository, as in [`GitEntry::path`]. The answer is
    /// [`ToPlugin::Done`] or [`ToPlugin::Refused`].
    GitStage { request: u64, path: String },
    /// Takes the file out of the staged changes (needs [`Capability::GitWrite`]).
    GitUnstage { request: u64, path: String },
    /// Stages every change in the working folder (needs [`Capability::GitWrite`]).
    /// Plugin API 1.4.
    GitStageAll { request: u64 },
    /// Takes everything out of the staged changes (needs [`Capability::GitWrite`]).
    /// Plugin API 1.4.
    GitUnstageAll { request: u64 },
    /// Commits what is staged with `message` (needs [`Capability::GitWrite`]). The
    /// answer is [`ToPlugin::Done`], or [`ToPlugin::Refused`] with
    /// [`Refusal::BadArgument`] for an empty message, [`Refusal::NothingStaged`] when
    /// nothing is staged, and [`Refusal::Failed`] with what git and its hooks printed.
    /// Plugin API 1.4.
    GitCommit { request: u64, message: String },
    /// Fetches from the remote (needs [`Capability::GitRemote`]). It runs until it
    /// ends, is stopped with [`FromPlugin::GitCancel`] or the user presses `esc`,
    /// or five minutes have passed; the answer is [`ToPlugin::Done`] or
    /// [`ToPlugin::Refused`]. Only one of fetch, pull and push runs at a time
    /// ([`Refusal::Busy`]). Plugin API 1.4.
    GitFetch { request: u64 },
    /// Brings the branch up to date with its upstream when that is only a move
    /// forward (`git pull --ff-only`); branches that diverged are refused with git's
    /// text ([`Refusal::Failed`]) and nothing changes. Needs [`Capability::GitRemote`].
    /// Plugin API 1.4.
    GitPull { request: u64 },
    /// Pushes the branch; one with no upstream is pushed with `-u` when the
    /// repository has exactly one remote. Needs [`Capability::GitRemote`]. Plugin
    /// API 1.4.
    GitPush { request: u64 },
    /// Stops the fetch, pull or push that is running (needs
    /// [`Capability::GitRemote`]); the stopped one is answered with
    /// [`Refusal::Cancelled`], and this with [`ToPlugin::Done`] whether one was
    /// running or not. Plugin API 1.4.
    GitCancel { request: u64 },
    /// Calls another plugin by its id (needs [`Capability::PluginCall`]); the other
    /// must have [`Capability::PluginServe`]. The answer comes as
    /// [`ToPlugin::Reply`] parts with the same `request`, or as [`ToPlugin::Refused`]
    /// ([`Refusal::NoSuchPlugin`] when there is no such plugin or it does not serve,
    /// [`Refusal::Gone`] when it trapped or was turned off before the last part,
    /// [`Refusal::Busy`] over [`crate::MAX_OPEN_CALLS`] calls at once, [`Refusal::BadArgument`]
    /// for a `request` that is open already or a call to itself). `payload` is text the
    /// two agree on. Plugin API 1.6.
    Call {
        request: u64,
        to: String,
        method: String,
        #[serde(default)]
        payload: String,
    },
    /// A part of the answer to the call `request` made by the plugin `to` (needs
    /// [`Capability::PluginServe`]); `last` ends the call. A part for a call that is
    /// not open (cancelled, ended, made by another plugin) is dropped. Plugin API 1.6.
    Reply {
        to: String,
        request: u64,
        payload: String,
        #[serde(default)]
        last: bool,
    },
    /// Stops the call `request` this plugin made: the other plugin is told
    /// ([`ToPlugin::CallCancelled`]) and anything it says after is dropped, so the
    /// caller never hears of a call it gave up. Calls that are not open are ignored.
    /// Plugin API 1.6.
    CallCancel { request: u64 },
    /// Runs a command (needs [`Capability::ProcessRun`]): `program` is a name looked up
    /// on `PATH` and `args` are its arguments, run with no shell in `cwd`, a folder of
    /// the project as a path from its root with `/` (the root when empty; no `..`).
    /// It must match a pattern of the manifest's `run` ([`Refusal::NotAllowed`]) and
    /// the folder must be trusted ([`Refusal::Untrusted`]); at most
    /// [`crate::MAX_OPEN_RUNS`] run at once ([`Refusal::Busy`]). What it writes comes as
    /// [`ToPlugin::RunOutput`], its end as [`ToPlugin::RunDone`]; a command that cannot
    /// be started is [`ToPlugin::Refused`] with [`Refusal::Failed`]. Its standard input
    /// is empty. A command still running when the plugin is turned off, replaced or
    /// trapped is killed with everything it started. Plugin API 1.7.
    Run {
        request: u64,
        program: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        cwd: String,
    },
    /// Kills the command `request` (needs [`Capability::ProcessRun`]); it ends with
    /// [`ToPlugin::RunDone`] saying `cancelled`. A command that is not running is
    /// ignored. Plugin API 1.7.
    RunCancel { request: u64 },
    /// The items of a menu to show at the pointer, as the answer to [`ToPlugin::ContextClick`]
    /// (a menu nobody asked for is not shown). Each item is one of the plugin's actions (its
    /// `id` in the manifest), shown with the title and the keys the manifest gives it, and
    /// choosing it runs the action, so the plugin is told [`ToPlugin::Action`] as for a key. An
    /// action the manifest does not have is left out, and so are the items past
    /// [`crate::MAX_MENU_ITEMS`]. Plugin API 1.10.
    Menu { items: Vec<WireMenuItem> },
    /// A kind of message a later version of the plugin API has.
    #[serde(other)]
    Unknown,
}

/// An item of a menu a plugin shows ([`FromPlugin::Menu`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum WireMenuItem {
    /// One of the plugin's actions, by its `id` in the manifest.
    Action { action: String },
    /// A rule between groups of items.
    Separator,
    /// A kind of item a later version has.
    #[serde(other)]
    Unknown,
}

impl WireMenuItem {
    pub fn action(action: impl Into<String>) -> Self {
        Self::Action {
            action: action.into(),
        }
    }
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
    /// Lines with a row that is selected: the editor draws them with the row
    /// highlighted and scrolls so that it is on screen, and tells the plugin of a
    /// click on a row ([`ToPlugin::Click`]). Each line may be made of spans that
    /// have styles of their own.
    Rows {
        #[serde(default)]
        lines: Vec<WireLine>,
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
        /// Whether the line is being typed into. Only a line inside a
        /// [`WireView::Column`] can be inactive (the other parts of the column then
        /// have the keys); a line that is the whole view always is. Plugin API 1.4.
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        active: bool,
    },
    /// A row of buttons. A click on an enabled one is [`ToPlugin::Press`]. Plugin
    /// API 1.4.
    Buttons {
        #[serde(default)]
        buttons: Vec<WireButton>,
    },
    /// Parts one under the other: a [`WireView::Buttons`] row and a
    /// [`WireView::Input`] line take a row of the pane each, a [`WireView::Lines`]
    /// as many as it has lines, and a [`WireView::Rows`] or [`WireView::List`]
    /// takes what is left (two of them share it). A column inside a column is not
    /// drawn. Plugin API 1.4.
    Column {
        #[serde(default)]
        parts: Vec<WireView>,
    },
    /// A kind of view a later version of the plugin API has.
    #[serde(other)]
    Unknown,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// A button of a [`WireView::Buttons`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WireButton {
    /// What [`ToPlugin::Press`] names it by.
    pub id: String,
    /// What it shows.
    #[serde(default)]
    pub label: String,
    /// Whether it can be pressed; a button that cannot is drawn dim.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

impl WireButton {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
        }
    }

    /// The same button, pressable or not.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
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
    /// The line as pieces of text with styles of their own; when there are any,
    /// they are the line and `text` is not drawn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<WireSpan>,
}

/// A piece of a line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WireSpan {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub slot: String,
}

impl WireSpan {
    pub fn new(text: impl Into<String>, slot: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            slot: slot.into(),
        }
    }
}

impl WireLine {
    pub fn new(text: impl Into<String>, slot: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            slot: slot.into(),
            spans: Vec::new(),
        }
    }

    /// A line made of pieces with styles of their own.
    pub fn of_spans(spans: impl IntoIterator<Item = WireSpan>) -> Self {
        Self {
            text: String::new(),
            slot: String::new(),
            spans: spans.into_iter().collect(),
        }
    }
}

impl WireView {
    pub fn lines(lines: impl IntoIterator<Item = WireLine>) -> Self {
        Self::Lines {
            lines: lines.into_iter().collect(),
        }
    }

    pub fn rows(lines: impl IntoIterator<Item = WireLine>, selected: Option<usize>) -> Self {
        Self::Rows {
            lines: lines.into_iter().collect(),
            selected,
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
            active: true,
        }
    }

    /// A line to type into that is not being typed into (inside a
    /// [`WireView::Column`]).
    pub fn idle_input(prompt: impl Into<String>, value: impl Into<String>, cursor: usize) -> Self {
        Self::Input {
            prompt: prompt.into(),
            value: value.into(),
            cursor,
            active: false,
        }
    }

    pub fn buttons(buttons: impl IntoIterator<Item = WireButton>) -> Self {
        Self::Buttons {
            buttons: buttons.into_iter().collect(),
        }
    }

    pub fn column(parts: impl IntoIterator<Item = WireView>) -> Self {
        Self::Column {
            parts: parts.into_iter().collect(),
        }
    }
}

impl FromPlugin {
    pub fn set_view(view: WireView) -> Self {
        Self::SetView { view }
    }
}

/// Which output of a command text came from. Plugin API 1.7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RunStream {
    Stdout,
    Stderr,
    /// An output a newer editor has that this version does not know; read it as
    /// output of no stream in particular.
    #[serde(other)]
    Unknown,
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
    /// Open files of the project in the editor.
    #[serde(rename = "editor.open")]
    EditorOpen,
    /// Fetch from, pull from and push to the remotes of the repository: it reaches
    /// the network. Plugin API 1.4.
    #[serde(rename = "git.remote")]
    GitRemote,
    /// Receive the saved text of the files in front in the editor that are of the
    /// kinds the manifest names (`file_types`). Plugin API 1.5.
    #[serde(rename = "files.text")]
    FilesText,
    /// Call other plugins that serve ([`FromPlugin::Call`]). Plugin API 1.6.
    #[serde(rename = "plugin.call")]
    PluginCall,
    /// Be called by other plugins ([`ToPlugin::Call`]) and answer them. Plugin API 1.6.
    #[serde(rename = "plugin.serve")]
    PluginServe,
    /// Run the commands the manifest's `run` allows, in a folder the user trusts.
    /// Plugin API 1.7.
    #[serde(rename = "process.run")]
    ProcessRun,
    /// A capability a later version of the plugin API has.
    #[serde(other)]
    Unknown,
}

impl Capability {
    /// Every capability this version has.
    pub const ALL: [Self; 8] = [
        Self::GitRead,
        Self::GitWrite,
        Self::EditorOpen,
        Self::GitRemote,
        Self::FilesText,
        Self::PluginCall,
        Self::PluginServe,
        Self::ProcessRun,
    ];

    /// The name a manifest writes it by.
    pub const fn name(self) -> &'static str {
        match self {
            Self::GitRead => "git.read",
            Self::GitWrite => "git.write",
            Self::EditorOpen => "editor.open",
            Self::GitRemote => "git.remote",
            Self::FilesText => "files.text",
            Self::PluginCall => "plugin.call",
            Self::PluginServe => "plugin.serve",
            Self::ProcessRun => "process.run",
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
    /// A commit was asked for and nothing is staged. Plugin API 1.4.
    NothingStaged,
    /// A fetch, pull or push is running already. Plugin API 1.4.
    Busy,
    /// The user (or the plugin) stopped the command. Plugin API 1.4.
    Cancelled,
    /// The remote wants a password, a passphrase or a key the editor cannot give:
    /// what git said. The user can authenticate once in the terminal pane, or set up
    /// a credential helper or an ssh agent. Plugin API 1.4.
    AuthRequired {
        #[serde(default)]
        detail: String,
    },
    /// A call ([`FromPlugin::Call`]) to a plugin that is not there, or does not
    /// serve. Plugin API 1.6.
    NoSuchPlugin,
    /// The plugin a call was made to trapped or was turned off before it ended the
    /// call: what it said is not the whole answer. Plugin API 1.6.
    Gone,
    /// A command that no pattern of the manifest's `run` allows. Plugin API 1.7.
    NotAllowed {
        #[serde(default)]
        detail: String,
    },
    /// The folder that is open is not trusted (yet): the user is asked, and the plugin
    /// is told by [`ToPlugin::Trust`]. Plugin API 1.7.
    Untrusted,
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
