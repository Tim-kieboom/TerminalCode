//! An edit a language server asks the editor to make (`workspace/applyEdit`): a command that
//! works by asking for an edit, such as the ones some code actions run, changes things only if
//! this answers. The edit goes through the applier of rename and code actions, as an undo step
//! in open documents; the server is told whether it was made, and why not when it was not.

use super::App;
use super::rename::apply::{Check, EditError, plan};
use super::rename::{count, moved_note, open_document_of};
use crate::lsp::ApplyEditRequest;

impl App {
    /// Answers the edits servers asked for since the last time.
    pub(super) fn answer_apply_edits(&mut self) {
        let requests = self
            .lsp
            .manager
            .as_mut()
            .map(|manager| manager.take_apply_edits())
            .unwrap_or_default();
        for request in requests {
            let outcome = self.make_server_edit(&request);
            let (applied, reason) = match &outcome {
                Ok(message) => {
                    if !message.is_empty() {
                        self.state.notify(message.clone());
                    }
                    (true, None)
                }
                Err(error) => {
                    self.state.notify_error(error.to_string());
                    (false, Some(error.to_string()))
                }
            };
            if let Some(manager) = self.lsp.manager.as_ref() {
                manager.respond_apply_edit(&request.key, request.id, applied, reason);
            }
            self.needs_redraw = true;
        }
    }

    /// Makes the edit: checked against the open documents as they are now (the server's own
    /// request says they are what it looked at), then the moves and the text edits. Says what was
    /// done.
    fn make_server_edit(&mut self, request: &ApplyEditRequest) -> Result<String, EditError> {
        let root = self.project_root();
        let versions = self.state.workspace().document_versions();
        let workspace = self.state.workspace();
        let plan = plan(&request.edit, Check::Action, &root, &versions, |path| {
            open_document_of(workspace, path)
        })?;
        if plan.files() == 0 && plan.moves().is_empty() {
            return Ok(String::new());
        }
        self.move_and_edit(&plan)?;
        let label = request
            .label
            .clone()
            .unwrap_or_else(|| "the language server's edit".to_owned());
        Ok(format!(
            "{label}: {} in {}{}",
            count(plan.places(), "place"),
            count(plan.files(), "file"),
            moved_note(&plan),
        ))
    }
}
