//! Completion as you type: when to ask the server, what to do with its answer,
//! and the few keys that steer the box. The list itself, and what picking an
//! item writes, are [`Completion`]; the box on the screen is [`CompletionBox`].
//!
//! The server is asked from the loop, just before a frame and after the
//! servers were told about the text, so that it answers for the text on the
//! screen. The answer is looked for when the server makes a sound
//! ([`App::lsp_changed`]); it may be for text that has moved on a little, and
//! is moved forward through what was typed since.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ropey::Rope;

use super::App;
use super::state::{Focus, Popup};
use crate::components::completion_box::CompletionBox;
use crate::components::workspace::DocumentId;
use crate::lsp::{
    ByteEdit, Completion, PendingCompletion, Refresh, Trigger, byte_edits, change_between, to_lsp,
};

/// How long an answer is waited for before it is given up on.
const PATIENCE: Duration = Duration::from_secs(10);
/// How long picking a suggestion waits for the server to say what it needs imported.
const RESOLVE_PATIENCE: Duration = Duration::from_secs(2);

/// What the box of suggestions is waiting for.
#[derive(Default)]
pub(super) struct Asking {
    /// A key typed text since the last frame: only then is a word being typed.
    typed: bool,
    /// A request that was sent and has not been answered.
    pending: Option<Pending>,
}

impl std::fmt::Debug for Asking {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Asking")
            .field("typed", &self.typed)
            .field("pending", &self.pending.is_some())
            .finish()
    }
}

/// A request in flight, with the text it was made for.
struct Pending {
    request: PendingCompletion,
    document: DocumentId,
    /// The text the server was asked about, which its positions refer to.
    text: Rope,
    word_start: usize,
    cursor: usize,
    by_trigger: bool,
    sent: Instant,
}

/// What a frame decided to ask.
struct Wish {
    trigger: Trigger,
    word_start: usize,
    by_trigger: bool,
}

impl App {
    /// A key typed text: the next frame looks at whether a word is being typed.
    pub(super) fn note_typed(&mut self) {
        self.lsp.completion.typed = true;
    }

    /// What the loop does before each frame, after the servers were told about
    /// the text: moves the box along with what was typed, and asks the server
    /// when a word starts, when a trigger character was typed, or when the list
    /// on show is not enough any more.
    pub(crate) fn completion_step(&mut self) {
        let typed = std::mem::take(&mut self.lsp.completion.typed);
        let pending_too_old = self
            .lsp
            .completion
            .pending
            .as_ref()
            .is_some_and(|pending| pending.sent.elapsed() > PATIENCE);
        if pending_too_old {
            self.lsp.completion.pending = None;
        }

        let Some(document) = self.completion_document() else {
            self.close_completion();
            return;
        };
        let editor = self.state.editor();
        let text = editor.buffer().snapshot();
        let head = editor.selections().primary().head();
        let Ok(cursor) = editor.buffer().position_to_byte(head) else {
            self.close_completion();
            return;
        };

        let still_open = self.follow_text(document, &text, cursor);
        let wish = match still_open {
            true => self.wish_for_list_on_show(),
            false => typed
                .then(|| self.wish_for_typing(document, &text, cursor))
                .flatten(),
        };
        if let Some(wish) = wish {
            self.ask(document, text, cursor, wish);
        }
    }

    /// The document being typed in, when the editor has the keyboard.
    fn completion_document(&self) -> Option<DocumentId> {
        let free = self.state.focus() == Focus::Editor
            && matches!(self.state.popup(), Popup::None)
            && self.state.workspace().has_tabs();
        free.then(|| self.state.workspace().active_document_id())
            .flatten()
    }

    /// Moves the box through what changed in the text since it last looked, and
    /// closes it when the word is over or nothing matches. Whether it is open.
    fn follow_text(&mut self, document: DocumentId, text: &Rope, cursor: usize) -> bool {
        let keep = match self.state.completion_box_mut() {
            None => return false,
            Some(open) if open.document() != document => false,
            Some(open) => {
                if let Some(change) = change_between(open.text(), text) {
                    open.completion_mut().apply_change(&change.bytes);
                }
                let outcome = open.completion_mut().retarget(text, cursor);
                let completion = open.completion();
                // With nothing typed the box is only wanted right after a trigger character.
                let nothing_typed = completion.typed().is_empty() && !open.by_trigger();
                let stays = outcome == Refresh::Open && !completion.is_empty() && !nothing_typed;
                if stays {
                    open.set_text(text.clone());
                }
                stays
            }
        };
        if !keep {
            self.close_completion();
        }
        keep
    }

    /// What to ask when the list on show is not enough for the word now.
    fn wish_for_list_on_show(&self) -> Option<Wish> {
        let open = self.state.completion_box()?;
        let completion = open.completion();
        if !completion.wants_new_list() || self.lsp.completion.pending.is_some() {
            return None;
        }
        let trigger = match completion.is_incomplete() {
            true => Trigger::Incomplete,
            false => Trigger::Typing,
        };
        Some(Wish {
            trigger,
            word_start: completion.word_start(),
            by_trigger: open.by_trigger(),
        })
    }

    /// What to ask after text was typed with no box open: a trigger character
    /// the server named starts a list of everything that may follow it; a letter
    /// (or digit or underscore) starts one for the word it ends, unless the word
    /// is a number.
    fn wish_for_typing(&self, document: DocumentId, text: &Rope, cursor: usize) -> Option<Wish> {
        let (_, _, provider) = self.completion_target(document)?;
        let before = text.byte_to_char(cursor).checked_sub(1)?;
        let typed = text.get_char(before)?;
        if provider.triggers_on(typed) {
            return Some(Wish {
                trigger: Trigger::Character(typed),
                word_start: cursor,
                by_trigger: true,
            });
        }
        if !is_word_char(typed) || self.lsp.completion.pending.is_some() {
            return None;
        }
        let start = word_start(text, cursor);
        let first = text.get_char(text.byte_to_char(start))?;
        (!first.is_ascii_digit()).then_some(Wish {
            trigger: Trigger::Typing,
            word_start: start,
            by_trigger: false,
        })
    }

    /// Sends the request, forgetting one that has not been answered.
    fn ask(&mut self, document: DocumentId, text: Rope, cursor: usize, wish: Wish) {
        let Some((key, uri, _)) = self.completion_target(document) else {
            return;
        };
        let position = to_lsp(&text, cursor);
        let Some(request) = self.request_completion(&key, &uri, position, wish.trigger) else {
            return;
        };
        self.lsp.completion.pending = Some(Pending {
            request,
            document,
            text,
            word_start: wish.word_start,
            cursor,
            by_trigger: wish.by_trigger,
            sent: Instant::now(),
        });
    }

    /// Looks for the answer to the request in flight (the server made a sound).
    /// An answer is shown when the word it was for is still being typed,
    /// moved forward through what was typed since; otherwise it is dropped.
    pub(super) fn completion_answer(&mut self) {
        let Some(pending) = self.lsp.completion.pending.as_mut() else {
            return;
        };
        let Some(result) = pending.request.try_take() else {
            return;
        };
        let Some(pending) = self.lsp.completion.pending.take() else {
            return;
        };
        // An error is no box: not a message either, since it happens as you type.
        let Ok(list) = result else {
            return;
        };
        let Some(document) = self.completion_document() else {
            return;
        };
        if document != pending.document {
            return;
        }

        let editor = self.state.editor();
        let now = editor.buffer().snapshot();
        let head = editor.selections().primary().head();
        let Ok(cursor) = editor.buffer().position_to_byte(head) else {
            return;
        };
        let mut completion =
            Completion::new(list, &pending.text, pending.word_start, pending.cursor);
        if let Some(change) = change_between(&pending.text, &now) {
            completion.apply_change(&change.bytes);
        }
        let nothing_typed = completion.typed().is_empty() && !pending.by_trigger;
        // The word may be over by now (the cursor moved, a space was typed).
        let outcome = completion.retarget(&now, cursor);
        if outcome == Refresh::Closed || completion.is_empty() || nothing_typed {
            self.close_completion();
            return;
        }
        if let Some(open) = self.state.completion_box() {
            completion.keep_highlight_of(open.completion());
        }
        let open = CompletionBox::new(completion, document, now, pending.by_trigger);
        self.state.set_completion_box(Some(open));
        self.needs_redraw = true;
    }

    /// Takes the box away.
    pub(super) fn close_completion(&mut self) {
        if self.state.completion_box().is_some() {
            self.state.set_completion_box(None);
            self.needs_redraw = true;
        }
    }

    /// A key while the box is open. `Tab` writes the highlighted suggestion,
    /// `Up` and `Down` move the highlight, `Esc` closes the box; `Enter` closes
    /// it and goes on to be a newline. Every other key is typed or does what it
    /// does as if there were no box. Whether the key was used up.
    pub(super) fn handle_completion_key(&mut self, key: KeyEvent) -> bool {
        let Some(open) = self.state.completion_box() else {
            return false;
        };
        let plain = key.modifiers == KeyModifiers::NONE && key.kind != KeyEventKind::Release;
        if self.state.focus() != Focus::Editor || self.keyboard.is_pending() || !plain {
            return false;
        }
        if self.state.workspace().active_document_id() != Some(open.document()) {
            self.close_completion();
            return false;
        }
        match key.code {
            KeyCode::Tab => self.accept_completion(),
            KeyCode::Down => self.move_completion(true),
            KeyCode::Up => self.move_completion(false),
            KeyCode::Esc => self.close_completion(),
            // The page keys scroll the documentation when it has more than it shows.
            KeyCode::PageUp if self.page_docs(-1) => {}
            KeyCode::PageDown if self.page_docs(1) => {}
            KeyCode::Enter => {
                self.close_completion();
                return false;
            }
            _ => return false,
        }
        self.needs_redraw = true;
        true
    }

    /// Scrolls the documentation next to the suggestions by `pages`. Whether there
    /// was any that scrolls.
    fn page_docs(&mut self, pages: isize) -> bool {
        let Some(open) = self.state.completion_box() else {
            return false;
        };
        let scroll = open.docs_scroll();
        if !scroll.is_drawn() || !scroll.is_scrollable() {
            return false;
        }
        scroll.pages(pages);
        true
    }

    fn move_completion(&mut self, next: bool) {
        if let Some(open) = self.state.completion_box_mut() {
            match next {
                true => open.completion_mut().select_next(),
                false => open.completion_mut().select_previous(),
            }
        }
    }

    /// The edits that go with the highlighted suggestion and that the server only
    /// works out when it is picked: the `use` line of an import. The text goes to the
    /// server first, so that what it answers fits the text as it is; a server
    /// that does not answer in time leaves the word to be written without it.
    fn resolved_edits(&mut self) -> Vec<ByteEdit> {
        let Some(document) = self.state.workspace().active_document_id() else {
            return Vec::new();
        };
        let Some((key, _, provider)) = self.completion_target(document) else {
            return Vec::new();
        };
        let item = self
            .state
            .completion_box()
            .and_then(|open| open.completion().selected_item().cloned());
        // rust-analyzer says it cannot resolve (`resolveProvider: false`) and
        // does, for the items it stamps with `data`; so an item with data is
        // resolved whatever the server says.
        let Some(item) = item.filter(|item| provider.resolve || item.raw.get("data").is_some())
        else {
            return Vec::new();
        };
        self.lsp_sync();
        let Some(request) = self.resolve_completion(&key, &item) else {
            return Vec::new();
        };
        match request.wait(RESOLVE_PATIENCE) {
            Ok(edits) => byte_edits(&edits, &self.state.editor().buffer().snapshot()),
            Err(_) => {
                self.state
                    .notify("the language server did not say what to import; the name is written without it");
                Vec::new()
            }
        }
    }

    /// Writes the highlighted suggestion, and the import that goes with it, as one undo step.
    fn accept_completion(&mut self) {
        let editor = self.state.editor();
        let head = editor.selections().primary().head();
        let Ok(cursor) = editor.buffer().position_to_byte(head) else {
            return;
        };
        let resolved = self.resolved_edits();
        let accepted = self
            .state
            .completion_box()
            .and_then(|open| open.completion().accept_with(cursor, resolved));
        self.close_completion();
        // What was asked for before is about a word that is written now.
        self.lsp.completion.pending = None;
        let Some(accepted) = accepted else {
            return;
        };
        let edits: Vec<_> = accepted
            .edits
            .iter()
            .map(|edit| (edit.range.clone(), edit.text.as_str()))
            .collect();
        let result = self
            .state
            .edit(|editor| editor.replace_ranges(&edits, accepted.anchor, accepted.cursor));
        if let Err(error) = result {
            self.state.notify_error(error.to_string());
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Where the word that ends at `cursor` starts.
fn word_start(text: &Rope, cursor: usize) -> usize {
    let mut start = text.byte_to_char(cursor);
    while start > 0 && text.get_char(start - 1).is_some_and(is_word_char) {
        start -= 1;
    }
    text.char_to_byte(start)
}
