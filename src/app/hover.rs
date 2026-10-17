//! What a language server says about the thing under the mouse pointer, or at
//! the cursor: the pointer has to rest on a word for a moment, then the server
//! is asked and its answer shown in a small box ([`HoverBox`]) that goes away
//! when the pointer moves to another word, a key is pressed, the text changes
//! or anything else takes the screen.

use std::ops::Range;
use std::time::{Duration, Instant};

use ratatui::layout::Position;
use ropey::Rope;

use super::App;
use super::state::Popup;
use crate::components::hover_box::HoverBox;
use crate::components::popups::diagnostic_popup::Entry;
use crate::event::action::Action;
use crate::ids::DocumentId;
use crate::keymap::KeyChord;
use crate::lsp::{PendingHover, to_lsp};

/// How long the pointer has to rest on a word before the server is asked.
const REST: Duration = Duration::from_millis(400);
/// How long an answer is waited for before it is given up on.
const PATIENCE: Duration = Duration::from_secs(10);

/// What the hover waits for.
#[derive(Default)]
pub(super) struct Hovering {
    /// The word the pointer rests on, and since when.
    candidate: Option<Candidate>,
    /// A request that was sent and has not been answered.
    pending: Option<Pending>,
}

impl std::fmt::Debug for Hovering {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hovering")
            .field("candidate", &self.candidate.is_some())
            .field("pending", &self.pending.is_some())
            .finish()
    }
}

struct Candidate {
    document: DocumentId,
    /// What the pointer rests on: the word, or the range of the diagnostic when
    /// it is not on a word. The wait goes on while it stays the same.
    word: Range<usize>,
    /// The byte under the pointer, which is what the server is asked about.
    byte: usize,
    /// The cell the box hangs from.
    cell: Position,
    since: Instant,
    asked: bool,
}

struct Pending {
    request: PendingHover,
    document: DocumentId,
    /// The word asked about, when it was the pointer that asked.
    word: Option<Range<usize>>,
    /// The diagnostics there, which the box shows before what the server says.
    problems: Vec<Entry>,
    anchor: Position,
    /// Asked for with `f6`: the box stays until a key.
    pinned: bool,
    /// The version of the text the server was asked about.
    version: u64,
    sent: Instant,
}

impl App {
    /// The pointer moved to a cell: a new word starts the wait, the same word
    /// goes on with it, anything else clears the hover.
    pub(super) fn hover_pointer_moved(&mut self, column: u16, row: u16) {
        // A box pinned with `f6` stays whatever the pointer does; one from the
        // pointer stays while the pointer is on the box, to be scrolled.
        if let Some(shown) = self.state.hover_box() {
            if shown.is_pinned() || shown.scroll().contains(column, row) {
                return;
            }
        }
        let free = matches!(self.state.popup(), Popup::None)
            && self.state.completion_box().is_none()
            && self.state.diagnostic_popup().is_none();
        let target = free
            .then(|| self.state.workspace().text_at_cell(column, row))
            .flatten();
        let Some((document, byte, text)) = target else {
            self.clear_hover();
            return;
        };
        // A word, or a spot a diagnostic covers (a `;`, a bracket).
        let word = word_around(&text, byte).or_else(|| self.problem_range_at(document, byte));
        let Some(word) = word else {
            self.clear_hover();
            return;
        };
        let same = self
            .lsp
            .hovering
            .candidate
            .as_ref()
            .is_some_and(|candidate| candidate.document == document && candidate.word == word);
        if same {
            return;
        }
        self.clear_hover();
        self.lsp.hovering.candidate = Some(Candidate {
            document,
            word,
            byte,
            cell: Position::new(column, row),
            since: Instant::now(),
            asked: false,
        });
    }

    /// When the pointer has rested on its word long enough to ask the server.
    pub(crate) fn hover_deadline(&self) -> Option<Instant> {
        let candidate = self.lsp.hovering.candidate.as_ref()?;
        (!candidate.asked).then(|| candidate.since + REST)
    }

    /// The pointer has rested: asks the server about its word.
    pub(crate) fn hover_due(&mut self) {
        let Some(candidate) = self.lsp.hovering.candidate.as_mut() else {
            return;
        };
        if candidate.asked || candidate.since.elapsed() < REST {
            return;
        }
        candidate.asked = true;
        let (document, byte, cell) = (candidate.document, candidate.byte, candidate.cell);
        let word = Some(candidate.word.clone());
        self.ask_hover(document, byte, cell, word, false);
    }

    /// The diagnostics whose range holds `byte`, worst first.
    fn problems_at(&self, document: DocumentId, byte: usize) -> Vec<Entry> {
        let found: Vec<_> = self
            .state
            .diagnostics()
            .overlapping(document, byte..byte + 1)
            .collect();
        Entry::of_all(&found)
    }

    /// The range of the worst diagnostic that holds `byte`, if any.
    fn problem_range_at(&self, document: DocumentId, byte: usize) -> Option<Range<usize>> {
        self.state
            .diagnostics()
            .overlapping(document, byte..byte + 1)
            .min_by_key(|diagnostic| (diagnostic.severity, diagnostic.range.start))
            .map(|diagnostic| diagnostic.range.clone())
    }

    /// Sends the request for the thing at `byte` of `document`; the box will
    /// hang from `anchor`. The diagnostics there are in the box too, and when no
    /// server can be asked they are all it has.
    fn ask_hover(
        &mut self,
        document: DocumentId,
        byte: usize,
        anchor: Position,
        word: Option<Range<usize>>,
        pinned: bool,
    ) {
        let problems = self.problems_at(document, byte);
        let Some(version) = self.state.workspace().document_version(document) else {
            return;
        };
        let Some((key, uri)) = self.hover_target(document) else {
            if !problems.is_empty() {
                let shown =
                    HoverBox::new(String::new(), problems, anchor, document, version, pinned);
                self.state.set_hover_box(Some(shown));
                self.needs_redraw = true;
            }
            return;
        };
        // The server has to know the text as it is before it is asked about it.
        self.lsp_sync();
        let Some(text) = self.text_of(document) else {
            return;
        };
        let position = to_lsp(&text, byte);
        let Some(request) = self.request_hover(&key, &uri, position) else {
            return;
        };
        self.lsp.hovering.pending = Some(Pending {
            request,
            document,
            word,
            problems,
            anchor,
            pinned,
            version,
            sent: Instant::now(),
        });
    }

    /// The text of the document in front when it is `document`.
    fn text_of(&self, document: DocumentId) -> Option<Rope> {
        (self.state.workspace().active_document_id() == Some(document))
            .then(|| self.state.editor().buffer().snapshot())
    }

    /// The answer to the request in flight, if it has come (the server made a
    /// sound): shown when it is still about what the pointer rests on, or what
    /// the cursor is at, and the text is as it was.
    pub(super) fn hover_answer(&mut self) {
        let Some(pending) = self.lsp.hovering.pending.as_mut() else {
            return;
        };
        let Some(result) = pending.request.try_take() else {
            return;
        };
        let Some(pending) = self.lsp.hovering.pending.take() else {
            return;
        };
        // What the server says, or nothing: the diagnostics are shown either way.
        let text = result.ok().flatten().unwrap_or_default();
        if text.is_empty() && pending.problems.is_empty() {
            return;
        }
        let version = self.state.workspace().document_version(pending.document);
        let still_there = version == Some(pending.version)
            && matches!(self.state.popup(), Popup::None)
            && self.state.completion_box().is_none()
            && match &pending.word {
                Some(word) => self
                    .lsp
                    .hovering
                    .candidate
                    .as_ref()
                    .is_some_and(|candidate| {
                        candidate.document == pending.document && candidate.word == *word
                    }),
                None => true,
            };
        if !still_there {
            return;
        }
        let shown = HoverBox::new(
            text,
            pending.problems,
            pending.anchor,
            pending.document,
            pending.version,
            pending.pinned,
        );
        self.state.set_hover_box(Some(shown));
        self.needs_redraw = true;
    }

    /// What the loop does before each frame: takes the box away when the text
    /// changed, another document is in front, or a popup is open.
    pub(crate) fn hover_step(&mut self) {
        let Some(shown) = self.state.hover_box() else {
            return;
        };
        let same = self.state.workspace().active_document_id() == Some(shown.document())
            && self.state.workspace().document_version(shown.document()) == Some(shown.version())
            && matches!(self.state.popup(), Popup::None);
        if !same {
            self.close_hover();
        }
    }

    /// Takes the box and the wait away. Whether a box was there.
    pub(super) fn close_hover(&mut self) -> bool {
        self.lsp.hovering.pending = None;
        self.lsp.hovering.candidate = None;
        let had = self.state.hover_box().is_some();
        if had {
            self.state.set_hover_box(None);
            self.needs_redraw = true;
        }
        had
    }

    /// The pointer left the text or went to another word.
    fn clear_hover(&mut self) {
        self.close_hover();
    }

    /// `f6`: what the server says about the word at the cursor, in a box under it.
    /// `f6`: keeps the box that is there (what the pointer brought) until a key;
    /// without one, asks about the word at the cursor and keeps the answer; on a
    /// box that is kept already it takes it away.
    pub(super) fn show_hover(&mut self) {
        if let Some(shown) = self.state.hover_box_mut() {
            // The key that pinned it also lets it go.
            if shown.is_pinned() {
                self.close_hover();
                return;
            }
            shown.pin();
            self.lsp.hovering.candidate = None;
            self.needs_redraw = true;
            return;
        }
        self.ask_hover_at_cursor();
    }

    /// Asks about the word at the cursor, for a box under the cursor that stays.
    fn ask_hover_at_cursor(&mut self) {
        let Some(document) = self.state.workspace().active_document_id() else {
            return;
        };
        let editor = self.state.editor();
        let head = editor.selections().primary().head();
        let Ok(cursor) = editor.buffer().position_to_byte(head) else {
            return;
        };
        let text = editor.buffer().snapshot();
        // On a word, or just after one (the cursor stays at the end of what was typed),
        // or in the range of a diagnostic.
        let on = word_around(&text, cursor)
            .map(|_| cursor)
            .or_else(|| {
                cursor
                    .checked_sub(1)
                    .filter(|&before| word_around(&text, before).is_some())
            })
            .or_else(|| self.problem_range_at(document, cursor).map(|_| cursor));
        let Some(byte) = on else {
            self.state
                .notify("no word or problem at the cursor to ask about");
            return;
        };
        let screen = self.state.screen();
        let anchor = editor
            .cursor_cell()
            .unwrap_or_else(|| Position::new(screen.width / 4, screen.height / 3));
        self.close_hover();
        self.ask_hover(document, byte, anchor, None, true);
        if self.lsp.hovering.pending.is_none() && self.state.hover_box().is_none() {
            self.state
                .notify("no language server answers for this file");
        }
    }

    /// Gives up on a request that was never answered.
    pub(crate) fn hover_forget_stale(&mut self) {
        let stale = self
            .lsp
            .hovering
            .pending
            .as_ref()
            .is_some_and(|pending| pending.sent.elapsed() > PATIENCE);
        if stale {
            self.lsp.hovering.pending = None;
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The bytes of the word that holds the character at `byte`; `None` when that
/// character is not part of a word.
fn word_around(text: &Rope, byte: usize) -> Option<Range<usize>> {
    if byte >= text.len_bytes() {
        return None;
    }
    let at = text.byte_to_char(byte);
    if !is_word_char(text.get_char(at)?) {
        return None;
    }
    let mut start = at;
    while start > 0 && text.get_char(start - 1).is_some_and(is_word_char) {
        start -= 1;
    }
    let mut end = at + 1;
    while text.get_char(end).is_some_and(is_word_char) {
        end += 1;
    }
    Some(text.char_to_byte(start)..text.char_to_byte(end))
}

impl App {
    /// The wheel over a box of text that scrolls (what the server says about a
    /// word, or the documentation next to the suggestions) scrolls that box
    /// instead of the editor. Whether it did.
    pub(super) fn scroll_docs_under(&mut self, column: u16, row: u16, down: bool) -> bool {
        if !matches!(self.state.popup(), Popup::None) {
            return false;
        }
        let lines = if down { WHEEL_LINES } else { -WHEEL_LINES };
        let scroll = match (self.state.hover_box(), self.state.completion_box()) {
            (Some(shown), _) if shown.scroll().contains(column, row) => shown.scroll(),
            (_, Some(open)) if open.docs_scroll().contains(column, row) => open.docs_scroll(),
            _ => return false,
        };
        scroll.lines(lines);
        self.needs_redraw = true;
        true
    }

    /// A key while a box with what the server said about a word is up. One that
    /// was pinned with `f6` scrolls with the arrow and page keys; `Esc` takes
    /// either away and does nothing else; any other key takes it away and does its
    /// work. Whether the key was used up.
    pub(super) fn handle_hover_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

        let Some(shown) = self.state.hover_box() else {
            return false;
        };
        if key.kind == KeyEventKind::Release {
            return false;
        }
        // The key for the box is for show_hover to deal with: it pins the box that
        // is up, and takes a pinned one away.
        let for_the_box = KeyChord::from_event(key).is_some_and(|chord| {
            self.keyboard.action_for(self.contexts(), chord) == Some(&Action::ShowHover)
        });
        if for_the_box {
            return false;
        }
        if shown.is_pinned() && key.modifiers == KeyModifiers::NONE {
            let scroll = shown.scroll();
            let scrolled = match key.code {
                KeyCode::Up => Some(scroll.lines(-1)),
                KeyCode::Down => Some(scroll.lines(1)),
                KeyCode::PageUp => Some(scroll.pages(-1)),
                KeyCode::PageDown => Some(scroll.pages(1)),
                _ => None,
            };
            if scrolled.is_some() {
                self.needs_redraw = true;
                return true;
            }
        }
        self.close_hover();
        key.code == KeyCode::Esc
    }
}

/// How many lines the wheel moves a box by for each notch.
const WHEEL_LINES: isize = 3;
