//! The mark in the gutter for a line the language server has code actions for. When the
//! cursor has rested on a line for a moment, the server is asked what can be done there
//! (the same question as `ctrl+.`, for the whole line); if it offers something that can be
//! done, the line gets a mark. Moving the cursor or editing the text takes the mark away
//! and starts the wait again.

use std::time::{Duration, Instant};

use super::App;
use crate::ids::DocumentId;
use crate::lsp::{LspPosition, PendingActions, to_lsp};

/// How long the cursor rests on a line before the server is asked.
const REST: Duration = Duration::from_millis(400);

/// Where the cursor is, and the text it is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Spot {
    document: DocumentId,
    line: usize,
    version: u64,
}

struct Waiting {
    spot: Spot,
    since: Instant,
    asked: bool,
}

struct Asked {
    spot: Spot,
    request: PendingActions,
}

/// What the mark waits for.
#[derive(Default)]
pub(super) struct Lightbulb {
    waiting: Option<Waiting>,
    asked: Option<Asked>,
}

impl App {
    /// Whether no question about a line is on its way.
    #[cfg(test)]
    pub(crate) fn lightbulb_quiet(&self) -> bool {
        self.lsp.lightbulb.asked.is_none()
    }

    /// Where the cursor is now, when a server could be asked about it.
    fn lightbulb_spot(&self) -> Option<Spot> {
        if self.modal_open() || !self.lsp.acting.is_idle() {
            return None;
        }
        let document = self.state.workspace().active_document_id()?;
        let version = *self.state.workspace().document_versions().get(&document)?;
        self.code_action_target(document)?;
        let line = self.state.editor().selections().primary().head().line;
        Some(Spot {
            document,
            line,
            version,
        })
    }

    /// Looks at where the cursor is, once for each frame: a move takes the mark away and the
    /// wait starts again.
    pub(crate) fn lightbulb_watch(&mut self) {
        let spot = self.lightbulb_spot();
        let same = match (&self.lsp.lightbulb.waiting, spot) {
            (Some(waiting), Some(spot)) => waiting.spot == spot,
            _ => false,
        };
        if same {
            return;
        }
        self.lsp.lightbulb.asked = None;
        if self.state.action_mark().is_some() {
            self.state.set_action_mark(None);
        }
        self.lsp.lightbulb.waiting = spot.map(|spot| Waiting {
            spot,
            since: Instant::now(),
            asked: false,
        });
    }

    /// When the cursor has rested long enough to ask, if it is waiting.
    pub(crate) fn lightbulb_deadline(&self) -> Option<Instant> {
        let waiting = self.lsp.lightbulb.waiting.as_ref()?;
        (!waiting.asked).then(|| waiting.since + REST)
    }

    /// The cursor has rested: asks what can be done on its line.
    pub(crate) fn lightbulb_due(&mut self) {
        let Some(waiting) = self.lsp.lightbulb.waiting.as_mut() else {
            return;
        };
        if waiting.asked || waiting.since.elapsed() < REST {
            return;
        }
        waiting.asked = true;
        let spot = waiting.spot;
        let Some((key, uri)) = self.code_action_target(spot.document) else {
            return;
        };
        // The server has to know the text as it is before it is asked about it.
        self.lsp_sync();
        let Some(text) = self.document_snapshot(spot.document) else {
            return;
        };
        let buffer = self.state.editor().buffer();
        let start = buffer.line_start_byte(spot.line);
        let Ok(content) = buffer.line_content(spot.line) else {
            return;
        };
        let end = start + content.len();
        let diagnostics = self.context_diagnostics(spot.document, start..end, &text);
        let request = self.lsp.manager.as_ref().and_then(|manager| {
            let (from, to): (LspPosition, LspPosition) = (to_lsp(&text, start), to_lsp(&text, end));
            manager.request_code_actions(&key, &uri, from, to, &diagnostics)
        });
        self.lsp.lightbulb.asked = request.map(|request| Asked { spot, request });
    }

    /// The server's answer about the line: marks it when something can be done there.
    pub(super) fn lightbulb_answer(&mut self) {
        let Some(mut asked) = self.lsp.lightbulb.asked.take() else {
            return;
        };
        let Some(result) = asked.request.try_take() else {
            self.lsp.lightbulb.asked = Some(asked);
            return;
        };
        if self.lightbulb_spot() != Some(asked.spot) {
            return;
        }
        let offered =
            result.is_ok_and(|actions| actions.iter().any(|action| action.disabled.is_none()));
        if offered {
            self.state
                .set_action_mark(Some((asked.spot.document, asked.spot.line)));
        }
    }
}
