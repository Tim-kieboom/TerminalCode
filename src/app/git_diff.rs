//! A tab that shows the file in front next to the file as it is in `HEAD`: asking git for the
//! old text, and what the tab does when `HEAD` moves. The tab itself is the workspace's
//! (`components/workspace/diff.rs`).

use std::path::PathBuf;

use super::App;
use super::plugins::refused;
use super::state::Focus;
use crate::event::Event;
use crate::git::{ActionError, GitError, HeadText};
use crate::ids::DocumentId;
use crate::plugin_api::{Refusal, ToPlugin};

impl App {
    /// "Git: Open Diff": the file of the active tab against its text in `HEAD`.
    pub(super) fn open_diff_of_active(&mut self) {
        if let Some(document) = self.state.workspace().active_document_id() {
            self.open_diff_of(document);
        }
    }

    /// The file of `document` against its text in `HEAD`.
    pub(super) fn open_diff_of(&mut self, document: DocumentId) {
        let path = self
            .state
            .workspace()
            .document_info(document)
            .and_then(|(path, _, _)| path.map(PathBuf::from));
        let Some(path) = path else {
            self.state
                .notify("a diff needs a file: save this one first");
            return;
        };
        if self.state.git_status().is_none() {
            self.state
                .notify("there is no git repository here to compare with");
            return;
        }
        let Some(relative) = self.state.git_marks().relative(&path) else {
            self.state
                .notify(format!("{} is not in the git repository", path.display()));
            return;
        };
        self.ask_head(Some(document), relative, true, None);
    }

    /// Asks, in the background, for the text of the file in `HEAD`; the answer comes as
    /// `Event::GitHead`.
    fn ask_head(
        &mut self,
        document: Option<DocumentId>,
        relative: PathBuf,
        open: bool,
        reply: Option<(Box<str>, u64)>,
    ) {
        let Some(events) = self.background.sender() else {
            return;
        };
        let Some(status) = self.state.git_status() else {
            return;
        };
        let (root, head) = (status.root.clone(), status.branch.oid.clone());
        let (git, _) = self.git_tool();
        std::thread::spawn(move || {
            let result = git.head_text(&root, &relative);
            let _ = events.blocking_send(Event::GitHead {
                document,
                relative,
                head,
                open,
                reply,
                result,
            });
        });
    }

    /// The text of the file in `HEAD` arrived: the diff is shown, or its tab takes it as the
    /// new left side. A plugin that asked for the diff is told how it went.
    pub(super) fn git_head_answer(
        &mut self,
        document: Option<DocumentId>,
        relative: PathBuf,
        head: Option<String>,
        open: bool,
        reply: Option<(Box<str>, u64)>,
        result: Result<HeadText, GitError>,
    ) {
        let outcome = self.show_head(document, relative, head, open, result);
        let told = match &outcome {
            Ok(()) => None,
            // The tab of a diff that follows `HEAD` is left as it was, and the user told.
            Err(reason) => Some(reason.clone()),
        };
        if let Some(reason) = told {
            self.state.notify(reason);
        }
        if let Some((plugin, request)) = reply {
            let message = match outcome {
                Ok(()) => ToPlugin::Done { request },
                Err(detail) => refused(request, Refusal::Failed { detail }),
            };
            self.send_to_plugin(&plugin, message);
        }
        self.needs_redraw = true;
    }

    /// Puts the text of `HEAD` in the tab: shows the diff (`open`) or gives the tab of it a
    /// new left side. The reason when it cannot be done.
    fn show_head(
        &mut self,
        document: Option<DocumentId>,
        relative: PathBuf,
        head: Option<String>,
        open: bool,
        result: Result<HeadText, GitError>,
    ) -> Result<(), String> {
        if document.is_some_and(|document| !self.state.workspace().has_document(document)) {
            return Err("the file was closed before its text in HEAD arrived".to_owned());
        }
        let name = relative.display().to_string();
        let text = match result {
            Ok(HeadText::Text(text)) => Some(text),
            Ok(HeadText::NotInHead) => None,
            Ok(HeadText::NotText) => {
                return Err(format!("{name} is not text: there is nothing to compare"));
            }
            Ok(HeadText::TooLarge) => return Err(crate::components::workspace::too_large(&name)),
            Err(error) => return Err(format!("{name}: {error}")),
        };
        let workspace = self.state.workspace_mut();
        match (document, open) {
            (Some(document), true) => {
                workspace.open_diff(document, relative, head, text.as_deref())?;
                self.state.set_focus(Focus::Editor);
            }
            (Some(document), false) => {
                workspace.set_diff_base(document, head, text.as_deref())?;
            }
            (None, true) => {
                let Some(text) = text else {
                    return Err(format!("{name} is neither on the disk nor in HEAD"));
                };
                workspace.open_deleted_diff(relative, head, &text)?;
                self.state.set_focus(Focus::Editor);
            }
            (None, false) => {}
        }
        Ok(())
    }

    /// A plugin asked to show a file of the repository next to its text in `HEAD`: the path
    /// is checked like the path of any action, the file opens (a file that is gone from the
    /// disk has no tab but the diff's) and the plugin is told when the tab is shown.
    pub(super) fn plugin_open_diff(&mut self, plugin: &str, request: u64, path: &str) {
        let relative = match crate::git::repository_path(path) {
            Ok(relative) => relative,
            Err(ActionError::BadArgument(detail)) => {
                self.send_to_plugin(plugin, refused(request, Refusal::BadArgument { detail }));
                return;
            }
            Err(error) => {
                let detail = error.to_string();
                self.send_to_plugin(plugin, refused(request, Refusal::Failed { detail }));
                return;
            }
        };
        let Some(root) = self.state.git_status().map(|status| status.root.clone()) else {
            self.send_to_plugin(plugin, refused(request, Refusal::NoRepository));
            return;
        };
        let full = root.join(&relative);
        let reply = Some((Box::<str>::from(plugin), request));
        if full.is_file() {
            if let Err(error) = self.state.workspace_mut().open_path(&full) {
                let detail = error.to_string();
                self.send_to_plugin(plugin, refused(request, Refusal::Failed { detail }));
                return;
            }
            let document = self.state.workspace().active_document_id();
            self.ask_head(document, PathBuf::from(&relative), true, reply);
        } else if !full.exists() {
            self.ask_head(None, PathBuf::from(&relative), true, reply);
        } else {
            let detail = format!("`{relative}` is not a file");
            self.send_to_plugin(plugin, refused(request, Refusal::Failed { detail }));
        }
    }

    /// After a refresh: the diff tabs whose left side is from another commit than `HEAD` is
    /// at now ask for their text again. Once for each commit, so a failure is not asked
    /// again at every refresh.
    pub(super) fn diff_heads_step(&mut self) {
        let Some(status) = self.state.git_status() else {
            return;
        };
        let oid = status.branch.oid.clone();
        if self.git.diff_heads_asked.as_ref() == Some(&oid) {
            return;
        }
        let stale: Vec<_> = self
            .state
            .workspace()
            .diffs()
            .into_iter()
            .filter(|diff| diff.head != oid)
            .collect();
        if stale.is_empty() {
            return;
        }
        self.git.diff_heads_asked = Some(oid);
        for diff in stale {
            self.ask_head(Some(diff.document), diff.relative, false, None);
        }
    }

    /// The next (or previous) change of the diff in front.
    pub(super) fn jump_in_diff(&mut self, forward: bool) {
        match self.state.workspace_mut().diff_jump(forward) {
            None => self.state.notify("this tab is not a diff"),
            Some(false) => self.state.notify(match forward {
                true => "no more changes below",
                false => "no more changes above",
            }),
            Some(true) => {}
        }
    }
}
