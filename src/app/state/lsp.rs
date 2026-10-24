//! What the language servers said, kept for drawing: status, diagnostics, boxes.

use super::AppState;
use crate::components::completion_box::CompletionBox;
use crate::components::hover_box::HoverBox;

impl AppState {
    pub(crate) fn lsp_status(&self) -> Option<&str> {
        self.lsp_status.as_deref()
    }

    pub(crate) fn set_lsp_status(&mut self, status: Option<String>) {
        self.lsp_status = status;
    }

    /// Whether a format, or a save that formats first, is waiting for a server.
    pub(crate) fn formatting(&self) -> bool {
        self.formatting
    }

    pub(crate) fn set_formatting(&mut self, formatting: bool) {
        self.formatting = formatting;
    }

    pub(crate) fn debug(&self) -> &crate::components::debug::model::DebugData {
        &self.debug
    }

    pub(crate) fn debug_mut(&mut self) -> &mut crate::components::debug::model::DebugData {
        &mut self.debug
    }

    pub(crate) fn breakpoints(&self) -> &crate::breakpoints::Breakpoints {
        &self.breakpoints
    }

    pub(crate) fn breakpoints_mut(&mut self) -> &mut crate::breakpoints::Breakpoints {
        &mut self.breakpoints
    }

    /// The name of the run configuration the Run key starts, once it is known.
    pub(crate) fn run_label(&self) -> Option<&str> {
        self.run_label.as_deref()
    }

    pub(crate) fn set_run_label(&mut self, label: Option<String>) {
        self.run_label = label;
    }

    pub(crate) fn diagnostics(&self) -> &crate::lsp::Diagnostics {
        &self.lsp_diagnostics
    }

    pub(crate) fn diagnostics_mut(&mut self) -> &mut crate::lsp::Diagnostics {
        &mut self.lsp_diagnostics
    }

    /// The line with a mark in the gutter for the actions there, in `document`.
    pub(crate) fn action_mark_in(&self, document: crate::ids::DocumentId) -> Option<usize> {
        self.action_mark
            .and_then(|(marked, line)| (marked == document).then_some(line))
    }

    pub(crate) fn set_action_mark(&mut self, mark: Option<(crate::ids::DocumentId, usize)>) {
        self.action_mark = mark;
    }

    pub(crate) fn action_mark(&self) -> Option<(crate::ids::DocumentId, usize)> {
        self.action_mark
    }

    pub(crate) fn hover_box(&self) -> Option<&HoverBox> {
        self.hover_box.as_ref()
    }

    pub(crate) fn hover_box_mut(&mut self) -> Option<&mut HoverBox> {
        self.hover_box.as_mut()
    }

    pub(crate) fn set_hover_box(&mut self, hover_box: Option<HoverBox>) {
        self.hover_box = hover_box;
    }

    pub(crate) fn debug_hover(&self) -> Option<&crate::components::debug::DebugHoverBox> {
        self.debug_hover.as_ref()
    }

    pub(crate) fn debug_hover_mut(
        &mut self,
    ) -> Option<&mut crate::components::debug::DebugHoverBox> {
        self.debug_hover.as_mut()
    }

    pub(crate) fn set_debug_hover(
        &mut self,
        hover: Option<crate::components::debug::DebugHoverBox>,
    ) {
        self.debug_hover = hover;
    }

    pub(crate) fn completion_box(&self) -> Option<&CompletionBox> {
        self.completion_box.as_ref()
    }

    pub(crate) fn completion_box_mut(&mut self) -> Option<&mut CompletionBox> {
        self.completion_box.as_mut()
    }

    pub(crate) fn set_completion_box(&mut self, completion_box: Option<CompletionBox>) {
        self.completion_box = completion_box;
    }
}
