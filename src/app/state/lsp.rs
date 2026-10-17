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

    pub(crate) fn diagnostics(&self) -> &crate::lsp::Diagnostics {
        &self.lsp_diagnostics
    }

    pub(crate) fn diagnostics_mut(&mut self) -> &mut crate::lsp::Diagnostics {
        &mut self.lsp_diagnostics
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
