//! Go to definition: `f12` on the word at the cursor, `ctrl+click` on a word.
//! The server is asked where the thing is defined; one place is gone to, the
//! file opened if it is not (a file outside the project read-only, and served
//! by the server that pointed at it, so that hover and go to definition keep
//! working in the source of the standard library); the place the cursor leaves
//! goes in the jump list.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ropey::Rope;

use super::App;
use crate::components::workspace::DocumentId;
use crate::lsp::uri::uri_to_path;
use crate::lsp::{Location, PendingDefinition, ServerKey, from_lsp, to_lsp};
use crate::paths::canonical;

/// How long an answer is waited for before it is given up on.
const PATIENCE: Duration = Duration::from_secs(10);

/// What go to definition waits for.
#[derive(Default)]
pub(super) struct Defining {
    pending: Option<Pending>,
}

impl std::fmt::Debug for Defining {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Defining")
            .field("pending", &self.pending.is_some())
            .finish()
    }
}

struct Pending {
    request: PendingDefinition,
    /// The server that was asked: the files it points at stay with it.
    key: ServerKey,
    sent: Instant,
}

impl App {
    /// `f12`: where the word at the cursor is defined (or the word just before it).
    pub(super) fn go_to_definition(&mut self) {
        let Some(document) = self.state.workspace().active_document_id() else {
            return;
        };
        let editor = self.state.editor();
        let head = editor.selections().primary().head();
        let Ok(cursor) = editor.buffer().position_to_byte(head) else {
            return;
        };
        let text = editor.buffer().snapshot();
        let on = word_at(&text, cursor).then_some(cursor).or_else(|| {
            cursor
                .checked_sub(1)
                .filter(|&before| word_at(&text, before))
        });
        let Some(byte) = on else {
            self.state.notify("no word at the cursor to look up");
            return;
        };
        self.ask_definition(document, byte);
    }

    /// `ctrl+click`: where the word under the pointer is defined. The cursor
    /// stays where it is until the answer takes it there.
    pub(super) fn go_to_definition_at(&mut self, column: u16, row: u16) {
        let target = self.state.workspace().text_at_cell(column, row);
        let Some((document, byte, text)) = target else {
            return;
        };
        if !word_at(&text, byte) {
            return;
        }
        self.ask_definition(document, byte);
    }

    fn ask_definition(&mut self, document: DocumentId, byte: usize) {
        let Some((key, uri)) = self.definition_target(document) else {
            self.state
                .notify("no language server answers for this file");
            return;
        };
        // The server has to know the text as it is before it is asked about it.
        self.lsp_sync();
        let text = self.document_text(document);
        let Some(text) = text else {
            return;
        };
        let position = to_lsp(&text, byte);
        match self.request_definition(&key, &uri, position) {
            Some(request) => {
                self.lsp.defining.pending = Some(Pending {
                    request,
                    key,
                    sent: Instant::now(),
                });
            }
            None => self
                .state
                .notify("the language server did not take the request"),
        }
    }

    /// The text of `document` when it is the one in front.
    fn document_text(&self, document: DocumentId) -> Option<Rope> {
        (self.state.workspace().active_document_id() == Some(document))
            .then(|| self.state.editor().buffer().snapshot())
    }

    /// The answer to the request in flight, if it has come (the server made a sound).
    pub(super) fn definition_answer(&mut self) {
        let Some(pending) = self.lsp.defining.pending.as_mut() else {
            return;
        };
        let Some(result) = pending.request.try_take() else {
            return;
        };
        let Some(pending) = self.lsp.defining.pending.take() else {
            return;
        };
        let found = match result {
            Ok(found) => found,
            Err(error) => {
                self.state.notify_error(error.to_string());
                return;
            }
        };
        match found.as_slice() {
            [] => self.state.notify("no definition found"),
            [only] => self.go_to_location(only, &pending.key),
            several => self.choose_definition(several, &pending.key),
        }
        self.needs_redraw = true;
    }

    /// Gives up on a request that was never answered.
    pub(crate) fn definition_forget_stale(&mut self) {
        let stale = self
            .lsp
            .defining
            .pending
            .as_ref()
            .is_some_and(|pending| pending.sent.elapsed() > PATIENCE);
        if stale {
            self.lsp.defining.pending = None;
            self.state
                .notify("the language server did not say where it is defined");
        }
    }

    /// Several places: until there is a list to choose from, the first.
    fn choose_definition(&mut self, found: &[Location], key: &ServerKey) {
        self.go_to_location(&found[0], key);
    }

    /// Goes to a place the server named, opening its file when it is not open.
    /// A file outside the project is opened read-only and stays with `key`, the
    /// server that pointed at it. The place the cursor leaves is recorded.
    pub(super) fn go_to_location(&mut self, location: &Location, key: &ServerKey) {
        let Some(path) = uri_to_path(&location.uri) else {
            self.state
                .notify_error(format!("cannot open {}", location.uri));
            return;
        };
        let outside = !self.is_in_project(&path);
        let was_open = self.state.workspace().document_of_path(&path).is_some();
        if outside {
            // Before the file is shown, so that the first sync already knows its server.
            self.borrow_file(&path, key.clone(), location.uri.clone());
        }
        if let Some(here) = self.here() {
            self.record_place(here);
        }
        if let Err(error) = self.state.workspace_mut().open_path(&path) {
            self.state.notify_error(error.to_string());
            return;
        }
        if outside && !was_open {
            if let Some(document) = self.state.workspace().active_document_id() {
                self.state.workspace_mut().lock_document(document);
            }
            self.state
                .notify("opened read-only: it is outside the project");
        }
        let text = self.state.editor().buffer().snapshot();
        let byte = from_lsp(&text, location.start);
        match self.state.editor().buffer().byte_to_position(byte) {
            Ok(position) => self.state.edit(|editor| editor.go_to(position)),
            Err(error) => self.state.notify_error(error.to_string()),
        }
        self.state.set_focus(super::state::Focus::Editor);
        self.needs_redraw = true;
    }

    /// Whether `path` is under the project folder.
    fn is_in_project(&self, path: &Path) -> bool {
        let root: PathBuf = canonical(&self.project_root());
        canonical(path).starts_with(root)
    }
}

/// Whether the character at `byte` is part of a word.
fn word_at(text: &Rope, byte: usize) -> bool {
    if byte >= text.len_bytes() {
        return false;
    }
    text.get_char(text.byte_to_char(byte))
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
}
