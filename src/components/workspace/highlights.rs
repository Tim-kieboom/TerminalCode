//! Keeping the colors of the documents on screen up to date.

use super::Workspace;
use crate::components::editor::display_name;
use crate::ids::DocumentId;
use crate::syntax::{MAX_RANGES, Output, SyntaxWorker, merge_ranges};
use std::collections::HashMap;

impl Workspace {
    /// Brings the colors of every document that is on screen up to date, for
    /// the lines each pane shows and a screen of margin above and below, so a
    /// scroll does not show uncolored text. Run after the panes are laid out.
    ///
    /// Returns what went wrong for documents whose highlighting could not be
    /// set up, each once.
    pub(crate) fn refresh_highlights(&mut self, worker: Option<&SyntaxWorker>) -> Vec<String> {
        let Some(worker) = worker else {
            return Vec::new();
        };
        for key in self.forgotten.drain(..) {
            worker.forget(key);
        }
        let mut errors = Vec::new();
        let mut wanted: HashMap<DocumentId, Vec<std::ops::Range<usize>>> = HashMap::new();
        for pane in &self.panes {
            let Some(tab) = pane.tabs.get(pane.active) else {
                continue;
            };
            let buffer = &self.documents[&tab.document].buffer;
            let height = tab.view.viewport_height();
            let top = tab.view.scroll().top;
            let first = buffer.line_start_byte(top.saturating_sub(height));
            let last = buffer.line_start_byte(top + 2 * height);
            wanted.entry(tab.document).or_default().push(first..last);
        }
        for (id, ranges) in wanted {
            let ranges = merge_ranges(ranges, MAX_RANGES);
            if let Some(document) = self.documents.get_mut(&id) {
                document.syntax.update(&document.buffer, &ranges, worker);
                if let Some(error) = document.syntax.take_error() {
                    errors.push(format!("{}: {error}", display_name(&document.buffer)));
                }
            }
        }
        errors
    }

    /// Makes every document ask the syntax worker for its colors again, for
    /// when the worker is a new one (a new theme).
    pub(crate) fn restart_highlights(&mut self) {
        for document in self.documents.values_mut() {
            document.syntax.start_over();
        }
    }

    /// How the focused document's text has been parsed so far.
    #[cfg(test)]
    pub(crate) fn active_parses(&self) -> crate::syntax::document::Parses {
        self.active_tab()
            .map(|tab| self.documents[&tab.document].syntax.parses())
            .unwrap_or_default()
    }

    /// Whether any document on screen still waits for the syntax worker.
    #[cfg(test)]
    pub(crate) fn highlights_pending(&self) -> bool {
        self.documents
            .values()
            .any(|document| document.syntax.is_waiting())
    }

    /// Takes what the syntax worker found. Returns whether the screen has to
    /// be drawn again.
    pub(crate) fn accept_highlights(&mut self, output: Output) -> bool {
        self.documents
            .values_mut()
            .find(|document| document.syntax.key() == output.key)
            .is_some_and(|document| document.syntax.accept(output))
    }
}
