//! Formatting documents with their language server: before a save when the
//! server's `format_on_save` setting is on, and "Language: Format Document" without one.
//!
//! A save that formats is asynchronous: the document is asked for formatting
//! edits, and written when they are applied or when the wait ends. The wait
//! never blocks the save for long: a server that is not ready is waited for at
//! most [`READY_PATIENCE`] from the save, an answer at most [`FORMAT_PATIENCE`],
//! and a document that changed meanwhile (the edits are about a text that is gone)
//! is saved as it is. Whatever goes wrong, the file is saved unformatted and a
//! message says why. Quitting waits for the saves; quitting again skips the
//! formatting.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::App;
use super::editing::{deleted_warning, overwrite_warning};
use super::lsp::find_target;
use super::rename::apply::{Check, plan};
use super::rename::open_document_of;
use crate::components::editor::IndentKind;
use crate::components::workspace::SaveOutcome;
use crate::ids::DocumentId;
use crate::lsp::uri::file_uri;
use crate::lsp::{
    FileEdits, LspPosition, PendingFormat, RenameEdit, ServerKey, WorkspaceEdit, from_lsp, to_lsp,
};

/// How long an answer to a formatting request is waited for.
const FORMAT_PATIENCE: Duration = Duration::from_secs(2);

/// How long a server that is still starting is waited for, from the save.
const READY_PATIENCE: Duration = Duration::from_secs(10);

/// What the formatting waits for.
pub(super) struct Formatting {
    /// How long a server that is starting is waited for; tests make it short.
    ready_patience: Duration,
    /// How long an answer is waited for; tests make it short.
    format_patience: Duration,
    pending: Vec<Pending>,
    /// Quit when the saves are done: a quit was asked for while they ran.
    quit_after: bool,
    /// The servers that were said not to format, so that it is said once.
    told: HashSet<ServerKey>,
}

impl App {
    /// Whether a save that formats first is still waiting for a language server.
    pub(super) fn formatting_pending(&self) -> bool {
        !self.lsp.formatting.pending.is_empty()
    }
}

impl Default for Formatting {
    fn default() -> Self {
        Self {
            ready_patience: READY_PATIENCE,
            format_patience: FORMAT_PATIENCE,
            pending: Vec::new(),
            quit_after: false,
            told: HashSet::new(),
        }
    }
}

struct Pending {
    document: DocumentId,
    key: ServerKey,
    uri: String,
    /// When it was asked for: the wait for a server counts from here.
    started: Instant,
    then: Then,
    /// The part to format, when it is not the whole document.
    range: Option<(LspPosition, LspPosition)>,
    state: State,
}

/// What happens after the formatting.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Then {
    Save,
    Nothing,
}

enum State {
    /// The server is not running yet.
    Waiting,
    /// Asked, with the version of the text that was sent.
    Asked {
        request: PendingFormat,
        version: u64,
        sent: Instant,
    },
}

/// How the formatting of a document ended.
enum Done {
    /// The document was closed meanwhile.
    Gone,
    /// The edits, to the text of this version.
    Formatted(Vec<RenameEdit>, u64),
    /// No formatting: the reason, for a message.
    Unformatted(String),
}

impl App {
    /// Whether any save or format is waiting for a server.
    pub(super) fn formatting_busy(&self) -> bool {
        !self.lsp.formatting.pending.is_empty()
    }

    /// When the wait of the first pending format runs out.
    pub(crate) fn formatting_deadline(&self) -> Option<Instant> {
        let formatting = &self.lsp.formatting;
        formatting
            .pending
            .iter()
            .map(|item| match &item.state {
                State::Waiting => item.started + formatting.ready_patience,
                State::Asked { sent, .. } => *sent + formatting.format_patience,
            })
            .min()
    }

    /// Starts saving `document` through its formatter, when the setting is on and
    /// its file has a server that formats (or is starting). `false` when the
    /// caller is to save as usual. The save is done when the formatting ends.
    pub(super) fn begin_format_save(&mut self, document: DocumentId) -> bool {
        if self.is_format_pending(document) {
            return true;
        }
        let has_file = self
            .state
            .workspace()
            .document_info(document)
            .is_some_and(|(path, _, _)| path.is_some());
        let Some((key, uri)) = self.format_target(document).filter(|_| has_file) else {
            return false;
        };
        // The setting is the server's: `format_on_save` in its `language_servers` entry.
        let wanted = self
            .settings
            .language_servers
            .get(&key.name)
            .is_some_and(|server| server.format_on_save);
        if !wanted {
            return false;
        }
        let Some(manager) = self.lsp.manager.as_ref() else {
            return false;
        };
        let (running, formats, starting) = (
            manager.is_running(&key),
            manager.offers_formatting(&key),
            manager.is_starting(&key),
        );
        if running && !formats {
            if self.lsp.formatting.told.insert(key) {
                self.state.notify(
                    "this language server does not format; its files are saved as they are",
                );
            }
            return false;
        }
        if !running && !starting {
            return false;
        }
        self.queue_format(document, key, uri, Then::Save, None);
        true
    }

    /// "Language: Format Document": formats the open document, without saving it.
    pub(super) fn format_document(&mut self) {
        let Some(document) = self.state.workspace().active_document_id() else {
            return;
        };
        let Some((key, uri)) = self.format_target(document) else {
            self.state.notify("no language server formats this file");
            return;
        };
        let Some(manager) = self.lsp.manager.as_ref() else {
            return;
        };
        if manager.is_running(&key) && !manager.offers_formatting(&key) {
            self.state.notify("this language server does not format");
            return;
        }
        if !manager.is_running(&key) && !manager.is_starting(&key) {
            self.state
                .notify("the language server for this file is not running");
            return;
        }
        if !self.is_format_pending(document) {
            self.queue_format(document, key, uri, Then::Nothing, None);
        }
    }

    /// "Language: Format Selection": formats the lines the selection touches,
    /// without saving.
    pub(super) fn format_selection(&mut self) {
        let Some(document) = self.state.workspace().active_document_id() else {
            return;
        };
        let editor = self.state.editor();
        let selection = *editor.selections().primary();
        let (Ok(anchor), Ok(head)) = (
            editor.buffer().position_to_byte(selection.anchor()),
            editor.buffer().position_to_byte(selection.head()),
        ) else {
            return;
        };
        if anchor == head {
            self.state.notify("select the text to format first");
            return;
        }
        let Some(text) = self.document_snapshot(document) else {
            return;
        };
        let range = whole_lines(&text, anchor.min(head), anchor.max(head));
        let Some((key, uri)) = self.format_target(document) else {
            self.state.notify("no language server formats this file");
            return;
        };
        let Some(manager) = self.lsp.manager.as_ref() else {
            return;
        };
        if manager.is_running(&key) && !manager.offers_range_formatting(&key) {
            self.state
                .notify("this language server cannot format a selection");
            return;
        }
        if !manager.is_running(&key) && !manager.is_starting(&key) {
            self.state
                .notify("the language server for this file is not running");
            return;
        }
        if !self.is_format_pending(document) {
            self.queue_format(document, key, uri, Then::Nothing, Some(range));
        }
    }

    /// `esc` while a quit waits for its saves: the quit is given up and the saves
    /// go on. Whether there was one.
    pub(super) fn cancel_waiting_quit(&mut self) -> bool {
        if !self.lsp.formatting.quit_after {
            return false;
        }
        self.lsp.formatting.quit_after = false;
        self.state
            .notify("quit cancelled; the saves that are running go on");
        true
    }

    /// The quit prompt was closed: nothing is to quit when the saves are done.
    pub(super) fn forget_quit_after_saves(&mut self) {
        self.lsp.formatting.quit_after = false;
    }

    fn is_format_pending(&self, document: DocumentId) -> bool {
        self.lsp
            .formatting
            .pending
            .iter()
            .any(|item| item.document == document)
    }

    /// The server that serves `document` and how it names it. Found from the
    /// file when the sync has not looked at it yet, which it does not while no
    /// server runs: a save can come while the server is still starting.
    fn format_target(&self, document: DocumentId) -> Option<(ServerKey, String)> {
        if let Some((_, Some(target))) = self.lsp.targets.get(&document) {
            return Some((target.key.clone(), target.uri.clone()));
        }
        let (path, _, _) = self.state.workspace().document_info(document)?;
        let target = find_target(&self.settings.language_servers, path?, &self.project_root())?;
        Some((target.key, target.uri))
    }

    fn queue_format(
        &mut self,
        document: DocumentId,
        key: ServerKey,
        uri: String,
        then: Then,
        range: Option<(LspPosition, LspPosition)>,
    ) {
        self.lsp.formatting.pending.push(Pending {
            document,
            key,
            uri,
            started: Instant::now(),
            then,
            range,
            state: State::Waiting,
        });
        self.formatting_step();
        let waiting = self
            .lsp
            .formatting
            .pending
            .iter()
            .any(|item| item.document == document && matches!(item.state, State::Waiting));
        if waiting {
            self.state
                .notify("waiting for the language server to format first");
        }
    }

    /// Moves every pending format on: sends what can be sent, and finishes what
    /// has an answer, a reason to give up, or ran out of time.
    pub(crate) fn formatting_step(&mut self) {
        if self.lsp.formatting.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.lsp.formatting.pending);
        let mut kept = Vec::new();
        let mut finished = Vec::new();
        for mut item in pending {
            match self.format_progress(&mut item) {
                None => kept.push(item),
                Some(done) => finished.push((item.document, item.then, done)),
            }
        }
        self.lsp.formatting.pending = kept;
        for (document, then, done) in finished {
            self.format_done(document, then, done);
        }
        self.formatting_settled();
        let busy = self.formatting_busy();
        if self.state.formatting() != busy {
            self.state.set_formatting(busy);
        }
        self.needs_redraw = true;
    }

    /// One step for one document: `Some` when it has ended.
    fn format_progress(&mut self, item: &mut Pending) -> Option<Done> {
        let current = self.state.workspace().document_version(item.document);
        let Some(current) = current else {
            return Some(Done::Gone);
        };
        match std::mem::replace(&mut item.state, State::Waiting) {
            State::Waiting => {
                let manager = self.lsp.manager.as_ref();
                let running = manager.is_some_and(|manager| manager.is_running(&item.key));
                if running {
                    let formats =
                        manager.is_some_and(|manager| manager.offers_formatting(&item.key));
                    if !formats {
                        return Some(Done::Unformatted(
                            "the language server does not format".to_owned(),
                        ));
                    }
                } else if !manager.is_some_and(|manager| manager.is_starting(&item.key)) {
                    return Some(Done::Unformatted(
                        "the language server is not running".to_owned(),
                    ));
                } else if item.started.elapsed() >= self.lsp.formatting.ready_patience {
                    return Some(Done::Unformatted(
                        "the language server was not ready".to_owned(),
                    ));
                } else {
                    return None;
                }
                // The server has to know the text as it is before it is asked about it.
                self.lsp_sync();
                let tab_size = u32::try_from(self.settings.tab_size.get()).unwrap_or(4);
                let spaces = self.settings.indent_style == IndentKind::Spaces;
                let request = self
                    .lsp
                    .manager
                    .as_ref()
                    .and_then(|manager| match item.range {
                        Some((start, end)) => manager.request_range_formatting(
                            &item.key, &item.uri, start, end, tab_size, spaces,
                        ),
                        None => manager.request_formatting(&item.key, &item.uri, tab_size, spaces),
                    });
                match request {
                    Some(request) => {
                        item.state = State::Asked {
                            request,
                            version: current,
                            sent: Instant::now(),
                        };
                        None
                    }
                    None => Some(Done::Unformatted(
                        "the language server did not take the request".to_owned(),
                    )),
                }
            }
            State::Asked {
                mut request,
                version,
                sent,
            } => {
                if current != version {
                    return Some(Done::Unformatted(
                        "the file changed while formatting".to_owned(),
                    ));
                }
                match request.try_take() {
                    Some(Ok(edits)) => Some(Done::Formatted(edits, version)),
                    Some(Err(error)) => Some(Done::Unformatted(error.to_string())),
                    None if sent.elapsed() >= self.lsp.formatting.format_patience => Some(
                        Done::Unformatted("the language server did not answer in time".to_owned()),
                    ),
                    None => {
                        item.state = State::Asked {
                            request,
                            version,
                            sent,
                        };
                        None
                    }
                }
            }
        }
    }

    /// A document's formatting ended: applies the edits, then saves if that was
    /// asked for.
    fn format_done(&mut self, document: DocumentId, then: Then, done: Done) {
        let (changed, note) = match done {
            Done::Gone => return,
            Done::Unformatted(reason) => (false, Some(reason)),
            Done::Formatted(edits, version) => match self.apply_format(document, &edits, version) {
                Ok(changed) => (changed, None),
                Err(reason) => (false, Some(reason)),
            },
        };
        match then {
            Then::Save => self.finish_save(document, note),
            Then::Nothing => {
                let name = self.document_name(document);
                self.state.notify(match (note, changed) {
                    (Some(reason), _) => format!("could not format {name}: {reason}"),
                    (None, true) => format!("formatted {name}"),
                    (None, false) => format!("{name} is already formatted"),
                });
            }
        }
    }

    fn document_name(&self, document: DocumentId) -> String {
        self.state
            .workspace()
            .document_info(document)
            .map(|(_, name, _)| name)
            .unwrap_or_default()
    }

    /// Makes the formatting edits in the document as one undo step. Whether
    /// anything changed; an edit that would not change the text is not made,
    /// so a document that is formatted already is left alone.
    fn apply_format(
        &mut self,
        document: DocumentId,
        edits: &[RenameEdit],
        version: u64,
    ) -> Result<bool, String> {
        let text = self
            .document_snapshot(document)
            .ok_or_else(|| "the file is gone".to_owned())?;
        let path = self
            .state
            .workspace()
            .document_info(document)
            .and_then(|(path, _, _)| path.map(std::path::Path::to_path_buf))
            .ok_or_else(|| "the file has no name".to_owned())?;
        let uri = file_uri(&path).ok_or_else(|| "the file has no name".to_owned())?;
        let changing: Vec<RenameEdit> = edits
            .iter()
            .filter(|edit| {
                let (start, end) = (from_lsp(&text, edit.start), from_lsp(&text, edit.end));
                // A broken range is kept, for the check to refuse.
                end < start || text.byte_slice(start..end) != edit.new_text.as_str()
            })
            .cloned()
            .collect();
        if changing.is_empty() {
            return Ok(false);
        }
        let edit = WorkspaceEdit {
            files: vec![FileEdits {
                uri,
                edits: changing,
            }],
            renames: Vec::new(),
            operations: Vec::new(),
        };
        let root = self.project_root();
        let versions = HashMap::from([(document, version)]);
        let workspace = self.state.workspace();
        let plan = plan(&edit, Check::Format, &root, &versions, |path| {
            open_document_of(workspace, path)
        })
        .map_err(|error| error.to_string())?;
        self.edit_open_documents(&plan);
        Ok(true)
    }

    /// Writes `document`, and says how it went; `note` is why it was not formatted.
    fn finish_save(&mut self, document: DocumentId, note: Option<String>) {
        let name = self.document_name(document);
        match self.state.workspace_mut().save_document(document) {
            Ok(SaveOutcome::Saved) => self.state.notify(match note {
                Some(reason) => format!("saved {name} without formatting: {reason}"),
                None => format!("saved {name}"),
            }),
            Ok(SaveOutcome::NeedsConfirmation(name)) => {
                self.state.notify_error(overwrite_warning(&name));
            }
            Ok(SaveOutcome::FileDeleted(name)) => {
                self.state.notify_error(deleted_warning(&name));
            }
            Err(error) => {
                let active = self.state.workspace().active_document_id() == Some(document);
                // Not allowed to write there: the password prompt of a save as root.
                if !(active && self.offer_root_save(&error)) {
                    self.state.notify_error(format!("{name}: {error}"));
                }
            }
        }
    }

    /// Quit was asked for and the saves are done: goes on with it.
    fn formatting_settled(&mut self) {
        if self.lsp.formatting.pending.is_empty() && self.lsp.formatting.quit_after {
            self.lsp.formatting.quit_after = false;
            self.quit();
        }
    }

    /// Quit, asked for while saves wait for their formatting. The first time it
    /// waits for them; the second skips the formatting and saves as it is.
    /// Whether to go on with the quit now.
    pub(super) fn quit_waits_for_formatting(&mut self) -> bool {
        if !self.formatting_busy() {
            return false;
        }
        if self.lsp.formatting.quit_after {
            self.lsp.formatting.quit_after = false;
            let pending = std::mem::take(&mut self.lsp.formatting.pending);
            for item in pending {
                let done = Done::Unformatted("skipped".to_owned());
                self.format_done(item.document, item.then, done);
            }
            return false;
        }
        self.lsp.formatting.quit_after = true;
        self.state
            .notify("formatting before saving; ctrl+q again skips the formatting and quits");
        true
    }

    /// A save for the quit prompt, which quits afterwards: taken over when the
    /// document is formatted first.
    pub(super) fn begin_format_save_for_quit(&mut self, document: DocumentId) -> bool {
        if !self.begin_format_save(document) {
            return false;
        }
        self.lsp.formatting.quit_after = true;
        true
    }
}

/// The range of the whole lines from the byte `start` to the byte `end` of
/// `text`: from the start of the first line to the start of the line after the
/// last (or to where `end` is, when that is the start of a line already).
fn whole_lines(text: &ropey::Rope, start: usize, end: usize) -> (LspPosition, LspPosition) {
    let first = text.byte_to_line(start.min(text.len_bytes()));
    let from = text.line_to_byte(first);
    let last = text.byte_to_line(end.min(text.len_bytes()));
    let to = match text.line_to_byte(last) == end {
        true => end,
        false => text
            .try_line_to_byte(last + 1)
            .unwrap_or_else(|_| text.len_bytes()),
    };
    (to_lsp(text, from), to_lsp(text, to))
}

#[cfg(test)]
impl App {
    /// Makes the waits short, so that a test does not take seconds.
    pub(crate) fn set_format_patience(&mut self, ready: Duration, answer: Duration) {
        self.lsp.formatting.ready_patience = ready;
        self.lsp.formatting.format_patience = answer;
    }
}

#[cfg(test)]
pub(crate) fn whole_lines_for_tests(
    text: &ropey::Rope,
    start: usize,
    end: usize,
) -> (LspPosition, LspPosition) {
    whole_lines(text, start, end)
}
