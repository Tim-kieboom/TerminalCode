//! What the language servers said is wrong: taking in what they publish, stepping
//! between the problems of a file and the box with their text.

use crate::components::popups::diagnostic_popup::{DiagnosticPopup, PopupKey};
use crate::ids::DocumentId;
use crate::lsp::{Diagnostic, ServerKey, parse_published};

use super::App;

impl App {
    /// A server published the diagnostics of a document: puts them in the text
    /// the server was last told, from the version it looked at. Ones about a
    /// document that is not open are not wanted.
    pub(super) fn take_published_diagnostics(
        &mut self,
        key: &ServerKey,
        params: &serde_json::Value,
    ) {
        let Some(published) = parse_published(params) else {
            return;
        };
        let Some(document) = self.document_with_uri(key, &published.uri) else {
            return;
        };
        let ranges: Vec<_> = published
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.start, diagnostic.end))
            .collect();
        let Some(placed) =
            self.lsp
                .sync
                .ranges_in_told_text(key, document, published.version, &ranges)
        else {
            // About a text that is not kept any more: nothing it says can be placed.
            self.state.diagnostics_mut().clear(document);
            return;
        };
        let diagnostics = published
            .diagnostics
            .into_iter()
            .zip(placed)
            .filter_map(|(diagnostic, range)| {
                Some(Diagnostic {
                    range: range?,
                    severity: diagnostic.severity,
                    message: diagnostic.message,
                    source: diagnostic.source,
                    code: diagnostic.code,
                })
            })
            .collect();
        self.state.diagnostics_mut().set(document, diagnostics);
        self.needs_redraw = true;
    }

    /// The open document that the server `key` knows by `uri`.
    pub(super) fn document_with_uri(&self, key: &ServerKey, uri: &str) -> Option<DocumentId> {
        self.lsp.targets.iter().find_map(|(id, (_, target))| {
            let target = target.as_ref()?;
            (target.key == *key && target.uri == uri).then_some(*id)
        })
    }
}

impl App {
    /// `f8` and `shift+f8`: the cursor goes to the next or the previous
    /// diagnostic of the open file, round to the other end after the last.
    pub(super) fn step_diagnostic(&mut self, next: bool) {
        let Some(document) = self.state.workspace().active_document_id() else {
            return;
        };
        let head = self.state.editor().selections().primary().head();
        let Ok(byte) = self.state.editor().buffer().position_to_byte(head) else {
            return;
        };
        let diagnostics = self.state.diagnostics();
        let found = match next {
            true => diagnostics.next_after(document, byte),
            false => diagnostics.previous_before(document, byte),
        };
        let Some(start) = found.map(|diagnostic| diagnostic.range.start) else {
            self.state
                .notify("no diagnostics in this file (a language server reports them)");
            return;
        };
        self.mark_jump();
        match self.state.editor().buffer().byte_to_position(start) {
            Ok(position) => self.state.edit(|editor| editor.go_to(position)),
            Err(error) => self.state.notify_error(error.to_string()),
        }
        self.needs_redraw = true;
    }
}

impl App {
    /// `f7`: the whole text of the diagnostics on the cursor's line, in a box
    /// next to the cursor.
    pub(super) fn show_diagnostics(&mut self) {
        let Some(document) = self.state.workspace().active_document_id() else {
            return;
        };
        let editor = self.state.editor();
        let buffer = editor.buffer();
        let line = editor.selections().primary().head().line;
        let (start, next) = (
            buffer.line_start_byte(line),
            buffer.line_start_byte(line + 1),
        );
        let on_line: Vec<&Diagnostic> = self
            .state
            .diagnostics()
            .overlapping(document, start..next)
            .collect();
        if on_line.is_empty() {
            self.state.notify("no diagnostics on this line");
            return;
        }
        // Under the cursor; the middle of the screen when it is out of sight.
        let screen = self.state.screen();
        let anchor = editor
            .cursor_cell()
            .unwrap_or_else(|| ratatui::layout::Position::new(screen.width / 4, screen.height / 3));
        let popup = DiagnosticPopup::new(&on_line, anchor);
        self.state.open_diagnostic_popup(popup);
        self.needs_redraw = true;
    }

    /// A key while the box with the diagnostics is open: the arrow and page
    /// keys scroll it, any other key closes it (and does nothing else).
    pub(super) fn handle_diagnostic_popup_key(&mut self, key: crossterm::event::KeyEvent) {
        match PopupKey::from_key(key) {
            PopupKey::Ignore => {}
            PopupKey::Lines(lines) => {
                if let Some(popup) = self.state.diagnostic_popup() {
                    popup.scroll_lines(lines);
                }
            }
            PopupKey::Pages(pages) => {
                if let Some(popup) = self.state.diagnostic_popup() {
                    popup.scroll_pages(pages);
                }
            }
            PopupKey::Close => self.state.close_diagnostic_popup(),
        }
        self.needs_redraw = true;
    }
}
