//! Rename symbol: `f2` in the editor. The server is asked whether there is a
//! symbol at the cursor (`prepareRename`, which also gives the name to start
//! from), a prompt takes the new name, and the server's answer, the edits to
//! every place the symbol is used, is applied to every file it names by
//! [`apply`]: all or nothing, only inside the project, only text edits.

use std::collections::HashMap;
use std::ops::Range;
use std::time::{Duration, Instant};

use ropey::Rope;

use super::App;
use super::definition::word_at;
use crate::components::popups::name_prompt::NamePrompt;
use crate::files::entries::EntryError;
use crate::ids::DocumentId;
use crate::lsp::{
    LspPosition, PendingPrepare, PendingRename, Prepared, ServerKey, WorkspaceEdit, from_lsp,
    to_lsp,
};
use apply::{OpenDocument, RenameError, plan, remap};

mod apply;

/// How long the answer to `prepareRename` is waited for.
const PREPARE_PATIENCE: Duration = Duration::from_secs(10);

/// How long the answer to `rename` is waited for: a server may have much to
/// look through.
const RENAME_PATIENCE: Duration = Duration::from_secs(60);

/// What a rename waits for.
#[derive(Default)]
pub(super) struct Renaming {
    stage: Stage,
}

#[derive(Default)]
enum Stage {
    #[default]
    Idle,
    /// Waiting for the server to say whether there is something to rename.
    Preparing(Preparing),
    /// The prompt for the new name is open.
    Naming(Naming),
    /// Waiting for the edit.
    Renaming(InFlight),
}

/// Where the question was asked.
struct Asked {
    key: ServerKey,
    uri: String,
    document: DocumentId,
    /// A byte of the word at the cursor.
    byte: usize,
    position: LspPosition,
    /// The version of the document's text the server was told.
    version: u64,
}

struct Preparing {
    request: PendingPrepare,
    asked: Asked,
    sent: Instant,
}

struct Naming {
    asked: Asked,
    old: String,
}

struct InFlight {
    request: PendingRename,
    old: String,
    new: String,
    /// The versions of the open documents when the server was asked.
    versions: HashMap<DocumentId, u64>,
    sent: Instant,
}

impl App {
    /// `f2`: asks for a new name for the symbol at the cursor.
    pub(super) fn rename_symbol(&mut self) {
        let Some((document, byte)) = self.word_near_cursor() else {
            self.state.notify("no word at the cursor to rename");
            return;
        };
        let Some((key, uri)) = self.rename_target(document) else {
            self.state
                .notify("no language server renames symbols for this file");
            return;
        };
        // The server has to know the text as it is before it is asked about it.
        self.lsp_sync();
        let (Some(text), Some(version)) = (
            self.document_snapshot(document),
            self.state.workspace().document_version(document),
        ) else {
            return;
        };
        let asked = Asked {
            key,
            uri,
            document,
            byte,
            position: to_lsp(&text, byte),
            version,
        };
        let Some(manager) = self.lsp.manager.as_ref() else {
            return;
        };
        if !manager.offers_prepare_rename(&asked.key) {
            // Nothing to ask first: the word is the name.
            let range = word_range(&text, byte);
            self.ask_for_name(asked, &text, range, None);
            return;
        }
        match manager.request_prepare_rename(&asked.key, &asked.uri, asked.position) {
            Some(request) => {
                self.lsp.renaming.stage = Stage::Preparing(Preparing {
                    request,
                    asked,
                    sent: Instant::now(),
                });
            }
            None => self
                .state
                .notify("the language server did not take the request"),
        }
    }

    /// The server that serves `document` and how it names it, when it runs and renames.
    fn rename_target(&self, document: DocumentId) -> Option<(ServerKey, String)> {
        let (_, target) = self.lsp.targets.get(&document)?;
        let target = target.as_ref()?;
        let offered = self.lsp.manager.as_ref()?.offers_rename(&target.key);
        offered.then(|| (target.key.clone(), target.uri.clone()))
    }

    fn document_snapshot(&self, document: DocumentId) -> Option<Rope> {
        let (_, _, buffer) = self.state.workspace().document_info(document)?;
        Some(buffer.snapshot())
    }

    /// The answers that have come: the server's word on the symbol, or its edit.
    pub(super) fn rename_answer(&mut self) {
        match std::mem::take(&mut self.lsp.renaming.stage) {
            Stage::Preparing(mut preparing) => match preparing.request.try_take() {
                Some(result) => self.prepared(preparing.asked, result),
                None => self.lsp.renaming.stage = Stage::Preparing(preparing),
            },
            Stage::Renaming(mut flight) => match flight.request.try_take() {
                Some(result) => self.edit_came(flight, result),
                None => self.lsp.renaming.stage = Stage::Renaming(flight),
            },
            other => self.lsp.renaming.stage = other,
        }
        self.needs_redraw = true;
    }

    /// Gives up on a request that was never answered.
    pub(crate) fn rename_forget_stale(&mut self) {
        let (waited, patience, what) = match &self.lsp.renaming.stage {
            Stage::Preparing(preparing) => (
                preparing.sent.elapsed(),
                PREPARE_PATIENCE,
                "whether there is something to rename there",
            ),
            Stage::Renaming(flight) => (flight.sent.elapsed(), RENAME_PATIENCE, "the rename"),
            _ => return,
        };
        if waited > patience {
            self.lsp.renaming.stage = Stage::Idle;
            self.state
                .notify(format!("the language server did not answer {what}"));
        }
    }

    fn prepared(&mut self, asked: Asked, result: Result<Option<Prepared>, crate::lsp::LspError>) {
        let found = match result {
            Ok(Some(found)) => found,
            Ok(None) => {
                self.state.notify("there is nothing to rename here");
                return;
            }
            Err(error) => {
                self.state.notify_error(error.to_string());
                return;
            }
        };
        let text = self.document_snapshot(asked.document);
        let current = self.state.workspace().document_version(asked.document);
        let Some(text) = text.filter(|_| current == Some(asked.version)) else {
            self.state
                .notify("the file changed while the name was looked up; try again");
            return;
        };
        let range = match found.range {
            Some((start, end)) => from_lsp(&text, start)..from_lsp(&text, end),
            None => word_range(&text, asked.byte),
        };
        self.ask_for_name(asked, &text, range, found.placeholder);
    }

    /// Opens the prompt for the new name of the symbol at `range` of `text`.
    fn ask_for_name(
        &mut self,
        asked: Asked,
        text: &Rope,
        range: Range<usize>,
        placeholder: Option<String>,
    ) {
        let old = text.byte_slice(range).to_string();
        if old.is_empty() {
            self.state.notify("there is nothing to rename here");
            return;
        }
        let start = placeholder.unwrap_or_else(|| old.clone());
        self.state
            .open_name_prompt(NamePrompt::rename_symbol(old.clone(), start));
        self.lsp.renaming.stage = Stage::Naming(Naming { asked, old });
    }

    /// The prompt was answered with `new`: asks the server for the edit.
    pub(super) fn submit_rename(&mut self, new: &str) -> Result<(), EntryError> {
        let new = new.trim();
        if new.is_empty() {
            return Err(EntryError::Empty);
        }
        let Stage::Naming(naming) = std::mem::take(&mut self.lsp.renaming.stage) else {
            self.state.notify("the rename was cancelled");
            return Ok(());
        };
        if new == naming.old {
            self.state.notify("the name is unchanged");
            return Ok(());
        }
        let asked = naming.asked;
        self.lsp_sync();
        let current = self.state.workspace().document_version(asked.document);
        if current != Some(asked.version) {
            self.state
                .notify("the file changed while the name was typed; try again");
            return Ok(());
        }
        let versions = self.state.workspace().document_versions();
        let request = self.lsp.manager.as_ref().and_then(|manager| {
            manager.request_rename(&asked.key, &asked.uri, asked.position, new)
        });
        match request {
            Some(request) => {
                self.lsp.renaming.stage = Stage::Renaming(InFlight {
                    request,
                    old: naming.old,
                    new: new.to_owned(),
                    versions,
                    sent: Instant::now(),
                });
            }
            None => self
                .state
                .notify("the language server did not take the request"),
        }
        Ok(())
    }

    fn edit_came(&mut self, flight: InFlight, result: Result<WorkspaceEdit, crate::lsp::LspError>) {
        let edit = match result {
            Ok(edit) => edit,
            Err(error) => {
                self.state.notify_error(error.to_string());
                return;
            }
        };
        if edit.files.is_empty() && edit.operations.is_empty() {
            self.state
                .notify("the language server found nothing to rename");
            return;
        }
        match self.apply_rename(&edit, &flight) {
            Ok(message) => self.state.notify(message),
            Err(error) => self.state.notify_error(error.to_string()),
        }
    }

    /// Makes the edit: the files that are not open are written, then the open
    /// documents are edited. Says what was done.
    fn apply_rename(
        &mut self,
        edit: &WorkspaceEdit,
        flight: &InFlight,
    ) -> Result<String, RenameError> {
        let root = self.project_root();
        let workspace = self.state.workspace();
        let mut plan = plan(edit, &flight.old, &root, &flight.versions, |path| {
            let id = workspace.document_of_path(path)?;
            let (_, _, buffer) = workspace.document_info(id)?;
            Some(OpenDocument {
                id,
                version: buffer.version(),
                text: buffer.snapshot(),
                read_only: buffer.is_read_only(),
            })
        })?;
        plan.write_closed()?;

        for open in plan.open() {
            let done = self
                .state
                .workspace_mut()
                .edit_document(open.document, |editor| {
                    let buffer = editor.buffer();
                    let selection = *editor.selections().primary();
                    let head = buffer.position_to_byte(selection.head())?;
                    let anchor = buffer.position_to_byte(selection.anchor())?;
                    let replacements: Vec<(Range<usize>, &str)> = open
                        .replacements
                        .iter()
                        .map(|(range, new)| (range.clone(), new.as_str()))
                        .collect();
                    editor.replace_ranges(
                        &replacements,
                        remap(anchor, &open.replacements),
                        remap(head, &open.replacements),
                    )
                });
            // An open document that went away is no loss: it was not edited.
            if let Some(Err(error)) = done {
                self.state.notify_error(error.to_string());
            }
        }
        self.needs_redraw = true;

        let open = plan.open().len();
        let unsaved = match open {
            0 => String::new(),
            1 => "; 1 open file is not saved".to_owned(),
            many => format!("; {many} open files are not saved"),
        };
        Ok(format!(
            "renamed {} to {}: {} in {}{unsaved}",
            flight.old,
            flight.new,
            count(plan.places(), "place"),
            count(plan.files(), "file"),
        ))
    }
}

fn count(number: usize, noun: &str) -> String {
    match number {
        1 => format!("1 {noun}"),
        _ => format!("{number} {noun}s"),
    }
}

/// The bytes of the word around `byte`.
fn word_range(text: &Rope, byte: usize) -> Range<usize> {
    let mut start = byte;
    while start > 0 && word_at(text, previous_char(text, start)) {
        start = previous_char(text, start);
    }
    let mut end = byte;
    while word_at(text, end) {
        end += text.char(text.byte_to_char(end)).len_utf8();
    }
    start..end
}

/// The byte where the character before `byte` starts.
fn previous_char(text: &Rope, byte: usize) -> usize {
    text.char_to_byte(text.byte_to_char(byte).saturating_sub(1))
}
