//! Code actions: `ctrl+.` asks the server what it offers at the cursor or the
//! selection (with the problems that touch it, so that a quick fix such as
//! "import `HashMap`" shows up), a box lists the answer, and the one picked is
//! done: its edit made, its command run.
//!
//! An edit is made by the applier of rename (`app/rename/apply.rs`), but only
//! in open documents, as undo steps, and with no check of the old text (an
//! import is an insertion, there is none). A file the edit names that is not
//! open is the weak spot: the server's positions for it come from what it read
//! from disk, and nothing says the disk did not change since. So such files are
//! opened as tabs first (the server is told with `didOpen`), and the server is
//! asked again, against the text it now has; the action picked is found in the
//! second answer by its kind, its title and the files it names, and the second
//! answer is the last: a third request is never made.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use super::App;
use super::rename::apply::{self, Check, EditError, plan, shown};
use super::rename::{count, moved_note, open_document_of};
use crate::components::popups::action_list::{ActionList, Command, Entry};
use crate::files::paths::canonical;
use crate::ids::DocumentId;
use crate::lsp::{
    CodeAction, ContextDiagnostic, LspPosition, PendingActionResolve, PendingActions,
    PendingCommand, ServerKey, WorkspaceEdit, to_lsp,
};

/// How long an answer to a question about actions is waited for.
const PATIENCE: Duration = Duration::from_secs(10);

/// How long a command is waited for.
const COMMAND_PATIENCE: Duration = Duration::from_secs(30);

/// What the code actions wait for.
#[derive(Default)]
pub(super) struct Acting {
    stage: Stage,
}

impl Acting {
    /// Whether nothing about actions is being asked, shown or done.
    pub(super) fn is_idle(&self) -> bool {
        matches!(self.stage, Stage::Idle)
    }
}

#[derive(Default)]
enum Stage {
    #[default]
    Idle,
    /// Waiting for the list of actions.
    Asking(Asking),
    /// The box with the actions is open.
    Choosing(Choosing),
    /// Waiting for the server to complete the action picked.
    Resolving(Resolving),
    /// Waiting for a command to run.
    Running(Running),
}

/// Where the question was asked.
#[derive(Clone)]
struct Context {
    key: ServerKey,
    uri: String,
    document: DocumentId,
    start: LspPosition,
    end: LspPosition,
    diagnostics: Vec<ContextDiagnostic>,
    /// The versions of the open documents when the server was asked.
    versions: HashMap<DocumentId, u64>,
}

/// The action picked before the files it names were opened, to be found again.
struct Picked {
    kind: Option<String>,
    title: String,
    /// The URIs of the files its edit names, in order.
    files: Vec<String>,
}

struct Asking {
    request: PendingActions,
    context: Context,
    /// Set when this is the second question, after opening files.
    picked: Option<Picked>,
    sent: Instant,
}

struct Choosing {
    context: Context,
    actions: Vec<CodeAction>,
}

struct Resolving {
    request: PendingActionResolve,
    context: Context,
    /// The second list, when this is the second round: no third question is asked.
    second: Option<Vec<CodeAction>>,
    sent: Instant,
}

struct Running {
    request: PendingCommand,
    sent: Instant,
}

impl App {
    /// `ctrl+.`: asks what can be done at the cursor or the selection.
    pub(super) fn code_action(&mut self) {
        let Some(document) = self.state.workspace().active_document_id() else {
            return;
        };
        let Some((key, uri)) = self.code_action_target(document) else {
            self.state
                .notify("no language server offers code actions for this file");
            return;
        };
        // The server has to know the text as it is before it is asked about it.
        self.lsp_sync();
        let Some(text) = self.document_snapshot(document) else {
            return;
        };
        let editor = self.state.editor();
        let selection = *editor.selections().primary();
        let (Ok(anchor), Ok(head)) = (
            editor.buffer().position_to_byte(selection.anchor()),
            editor.buffer().position_to_byte(selection.head()),
        ) else {
            return;
        };
        let bytes = anchor.min(head)..anchor.max(head);
        let diagnostics = self.context_diagnostics(document, bytes.clone(), &text);
        let context = Context {
            key,
            uri,
            document,
            start: to_lsp(&text, bytes.start),
            end: to_lsp(&text, bytes.end),
            diagnostics,
            versions: self.state.workspace().document_versions(),
        };
        self.ask_actions(context, None);
    }

    /// The problems that touch `bytes` of `document`, as the server is told them.
    pub(super) fn context_diagnostics(
        &self,
        document: DocumentId,
        bytes: std::ops::Range<usize>,
        text: &ropey::Rope,
    ) -> Vec<ContextDiagnostic> {
        self.state
            .diagnostics()
            .overlapping(document, bytes)
            .map(|diagnostic| ContextDiagnostic {
                start: to_lsp(text, diagnostic.range.start),
                end: to_lsp(text, diagnostic.range.end),
                severity: diagnostic.severity,
                message: diagnostic.message.clone(),
                source: diagnostic.source.clone(),
                code: diagnostic.code.clone(),
            })
            .collect()
    }

    /// The server that serves `document` and how it names it, when it runs and
    /// lists code actions.
    pub(super) fn code_action_target(&self, document: DocumentId) -> Option<(ServerKey, String)> {
        let (_, target) = self.lsp.targets.get(&document)?;
        let target = target.as_ref()?;
        let offered = self.lsp.manager.as_ref()?.offers_code_actions(&target.key);
        offered.then(|| (target.key.clone(), target.uri.clone()))
    }

    fn ask_actions(&mut self, context: Context, picked: Option<Picked>) {
        let request = self.lsp.manager.as_ref().and_then(|manager| {
            manager.request_code_actions(
                &context.key,
                &context.uri,
                context.start,
                context.end,
                &context.diagnostics,
            )
        });
        match request {
            Some(request) => {
                self.lsp.acting.stage = Stage::Asking(Asking {
                    request,
                    context,
                    picked,
                    sent: Instant::now(),
                });
            }
            None => self
                .state
                .notify("the language server did not take the request"),
        }
    }

    /// The answers that have come: the list of actions, a completed action, a
    /// command that ran.
    pub(super) fn code_actions_answer(&mut self) {
        match std::mem::take(&mut self.lsp.acting.stage) {
            Stage::Asking(mut asking) => match asking.request.try_take() {
                Some(result) => self.actions_came(asking.context, asking.picked, result),
                None => self.lsp.acting.stage = Stage::Asking(asking),
            },
            Stage::Resolving(mut resolving) => match resolving.request.try_take() {
                Some(result) => self.action_resolved(resolving.context, resolving.second, result),
                None => self.lsp.acting.stage = Stage::Resolving(resolving),
            },
            Stage::Running(mut running) => match running.request.try_take() {
                Some(Err(error)) => self.state.notify_error(error.to_string()),
                Some(Ok(())) => {}
                None => self.lsp.acting.stage = Stage::Running(running),
            },
            other => self.lsp.acting.stage = other,
        }
        self.needs_redraw = true;
    }

    /// Gives up on a request that was never answered.
    pub(crate) fn code_actions_forget_stale(&mut self) {
        let (waited, patience, what) = match &self.lsp.acting.stage {
            Stage::Asking(asking) => (asking.sent.elapsed(), PATIENCE, "what can be done there"),
            Stage::Resolving(resolving) => {
                (resolving.sent.elapsed(), PATIENCE, "what the action does")
            }
            Stage::Running(running) => (running.sent.elapsed(), COMMAND_PATIENCE, "the command"),
            _ => return,
        };
        if waited > patience {
            self.lsp.acting.stage = Stage::Idle;
            self.state
                .notify(format!("the language server did not answer {what}"));
        }
    }

    fn actions_came(
        &mut self,
        context: Context,
        picked: Option<Picked>,
        result: Result<Vec<CodeAction>, crate::lsp::LspError>,
    ) {
        let actions: Vec<CodeAction> = match result {
            // An action the server says cannot be done now is listed too, dim, with the reason.
            Ok(actions) => actions,
            Err(error) => {
                self.state.notify_error(error.to_string());
                return;
            }
        };
        match picked {
            None if actions.is_empty() => self.state.notify("no code actions here"),
            None => self.show_actions(context, actions),
            Some(picked) => self.after_second_answer(context, actions, &picked),
        }
    }

    /// Opens the box with `actions`.
    fn show_actions(&mut self, context: Context, actions: Vec<CodeAction>) {
        let entries = actions
            .iter()
            .map(|action| Entry {
                title: action.title.clone(),
                kind: action.kind.clone().unwrap_or_default(),
                disabled: action.disabled.clone(),
            })
            .collect();
        self.state.open_action_list(ActionList::new(entries));
        self.lsp.acting.stage = Stage::Choosing(Choosing { context, actions });
    }

    /// The answer to the second question: finds the action that was picked and
    /// goes on with it. When it cannot be told which is meant, the list is
    /// shown again; nothing is guessed.
    fn after_second_answer(&mut self, context: Context, actions: Vec<CodeAction>, picked: &Picked) {
        let same_action: Vec<&CodeAction> = actions
            .iter()
            .filter(|action| action.kind == picked.kind && action.title == picked.title)
            .collect();
        let chosen = match same_action.as_slice() {
            [only] => Some((*only).clone()),
            several => {
                let by_files: Vec<_> = several
                    .iter()
                    .filter(|action| {
                        action.edit.as_ref().map(edit_files).as_ref() == Some(&picked.files)
                    })
                    .collect();
                match by_files.as_slice() {
                    [only] => Some((**only).clone()),
                    _ => None,
                }
            }
        };
        match chosen {
            Some(action) => self.start_action(context, action, Some(actions)),
            None => {
                self.state
                    .notify("the actions changed after opening the files; pick again");
                if !actions.is_empty() {
                    self.show_actions(context, actions);
                }
            }
        }
    }

    /// A key while the box of actions is open.
    pub(super) fn handle_actions_key(&mut self, key: KeyEvent) {
        let Some(command) = Command::from_key(key) else {
            return;
        };
        match command {
            Command::Previous => self.with_action_list(|list| list.step(-1)),
            Command::Next => self.with_action_list(|list| list.step(1)),
            Command::PageUp => self.with_action_list(|list| list.page(-1)),
            Command::PageDown => self.with_action_list(|list| list.page(1)),
            Command::Go => self.pick_selected_action(),
            Command::Pick(number) => {
                let found = self
                    .state
                    .action_list_mut()
                    .is_some_and(|list| list.select_number(number));
                if found {
                    self.pick_selected_action();
                }
            }
            Command::Cancel => {
                self.state.take_action_list();
                self.lsp.acting.stage = Stage::Idle;
            }
        }
        self.needs_redraw = true;
    }

    fn with_action_list(&mut self, f: impl FnOnce(&mut ActionList)) {
        if let Some(list) = self.state.action_list_mut() {
            f(list);
        }
    }

    /// The mouse while the box of actions is open: a click on a row does it, one
    /// outside closes the box, the wheel moves the selection. Whether the box
    /// was open, in which case nothing else gets the event.
    pub(super) fn handle_actions_mouse(&mut self, event: MouseEvent) -> bool {
        let Some(list) = self.state.action_list_mut() else {
            return false;
        };
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if list.select_at(event.column, event.row) {
                    self.pick_selected_action();
                } else if !list.contains(event.column, event.row) {
                    self.state.take_action_list();
                    self.lsp.acting.stage = Stage::Idle;
                }
            }
            MouseEventKind::ScrollDown => list.step(1),
            MouseEventKind::ScrollUp => list.step(-1),
            _ => {}
        }
        true
    }

    /// Closes the box and does the action that was selected.
    fn pick_selected_action(&mut self) {
        // One that cannot be done says why, and the box stays for another pick.
        if let Some(reason) = self
            .state
            .action_list_mut()
            .and_then(|list| list.selected_disabled().map(str::to_owned))
        {
            self.state.notify(format!("cannot do that now: {reason}"));
            return;
        }
        let Some(list) = self.state.take_action_list() else {
            return;
        };
        let Stage::Choosing(choosing) = std::mem::take(&mut self.lsp.acting.stage) else {
            return;
        };
        let Some(action) = choosing.actions.get(list.selected()).cloned() else {
            return;
        };
        self.start_action(choosing.context, action, None);
    }

    /// An action was picked. One that has neither edit nor command waits for the
    /// server to complete it.
    fn start_action(
        &mut self,
        context: Context,
        action: CodeAction,
        second: Option<Vec<CodeAction>>,
    ) {
        if action.edit.is_some() || action.command.is_some() {
            self.perform(context, action, second);
            return;
        }
        let manager = self.lsp.manager.as_ref();
        let request = manager
            .filter(|manager| manager.offers_code_action_resolve(&context.key))
            .and_then(|manager| manager.request_resolve_code_action(&context.key, &action));
        match request {
            Some(request) => {
                self.lsp.acting.stage = Stage::Resolving(Resolving {
                    request,
                    context,
                    second,
                    sent: Instant::now(),
                });
            }
            None => self.state.notify("this action has nothing to do"),
        }
    }

    fn action_resolved(
        &mut self,
        context: Context,
        second: Option<Vec<CodeAction>>,
        result: Result<CodeAction, crate::lsp::LspError>,
    ) {
        match result {
            Ok(action) if action.edit.is_some() || action.command.is_some() => {
                self.perform(context, action, second);
            }
            Ok(_) => self.state.notify("this action has nothing to do"),
            Err(error) => self.state.notify_error(error.to_string()),
        }
    }

    /// Does a completed action: its edit, then its command.
    fn perform(&mut self, context: Context, action: CodeAction, second: Option<Vec<CodeAction>>) {
        if let Some(edit) = action.edit.as_ref().filter(|edit| !is_empty(edit)) {
            let closed = self.closed_files_of(edit);
            if !closed.is_empty() {
                self.open_then_ask_again(context, &action, &closed, second);
                return;
            }
            match self.apply_action(&context, &action) {
                Ok(message) => self.state.notify(message),
                Err(error) => {
                    self.state.notify_error(error.to_string());
                    return;
                }
            }
        } else if action.command.is_none() {
            self.state.notify("this action changes nothing");
        }
        if let Some(command) = &action.command {
            self.run_command(&context, command);
        }
    }

    /// The files of the project the edit names that are not open.
    fn closed_files_of(&self, edit: &WorkspaceEdit) -> Vec<PathBuf> {
        let root = canonical(&self.project_root());
        // Where the files are now: an edit may name one by where it moves to.
        apply::files_now(edit, &root)
            .into_iter()
            .filter(|path| {
                canonical(path).starts_with(&root)
                    && path.is_file()
                    && self.state.workspace().document_of_path(path).is_none()
            })
            .collect()
    }

    /// Opens `closed` as tabs, so that the server reads them (`didOpen`) and the
    /// edit is made on the text it has, and asks again. At the second round the
    /// list is shown again instead.
    fn open_then_ask_again(
        &mut self,
        mut context: Context,
        action: &CodeAction,
        closed: &[PathBuf],
        second: Option<Vec<CodeAction>>,
    ) {
        let root = canonical(&self.project_root());
        let mut names = Vec::with_capacity(closed.len());
        for path in closed {
            if let Err(error) = self.state.workspace_mut().open_path(path) {
                self.state.notify_error(error.to_string());
                break;
            }
            let path = canonical(path);
            names.push(shown(&root, &path));
        }
        // The tab the user was in stays in front.
        self.state.workspace_mut().show_document(context.document);
        self.state
            .notify(format!("opened {} for this action", names.join(", ")));
        if names.len() < closed.len() {
            return;
        }
        // The server is told about the new documents before it is asked.
        self.lsp_sync();
        let versions = self.state.workspace().document_versions();
        match second {
            Some(actions) => {
                for (document, version) in versions {
                    context.versions.entry(document).or_insert(version);
                }
                self.state
                    .notify("this action names more files; opened them, pick again");
                self.show_actions(context, actions);
            }
            None => {
                let picked = Picked {
                    kind: action.kind.clone(),
                    title: action.title.clone(),
                    files: action.edit.as_ref().map(edit_files).unwrap_or_default(),
                };
                context.versions = versions;
                self.ask_actions(context, Some(picked));
            }
        }
    }

    /// Makes the edit of `action` in the open documents. Says what was done.
    fn apply_action(
        &mut self,
        context: &Context,
        action: &CodeAction,
    ) -> Result<String, EditError> {
        let Some(edit) = &action.edit else {
            return Ok(String::new());
        };
        let root = self.project_root();
        let workspace = self.state.workspace();
        let plan = plan(edit, Check::Action, &root, &context.versions, |path| {
            open_document_of(workspace, path)
        })?;
        self.move_and_edit(&plan)?;
        self.needs_redraw = true;
        Ok(format!(
            "{}: {} in {}{}",
            action.title,
            count(plan.places(), "place"),
            count(plan.files(), "file"),
            moved_note(&plan),
        ))
    }

    /// Asks the server to run a command the action carries.
    fn run_command(&mut self, context: &Context, command: &crate::lsp::Command) {
        let Some(manager) = self.lsp.manager.as_ref() else {
            return;
        };
        if !manager.offers_execute_command(&context.key) {
            self.state
                .notify("the language server does not run commands");
            return;
        }
        match manager.request_execute_command(&context.key, command) {
            Some(request) => {
                self.lsp.acting.stage = Stage::Running(Running {
                    request,
                    sent: Instant::now(),
                });
            }
            None => self
                .state
                .notify("the language server did not take the request"),
        }
    }
}

/// The URIs of the files an edit names, in order.
fn edit_files(edit: &WorkspaceEdit) -> Vec<String> {
    edit.files.iter().map(|file| file.uri.clone()).collect()
}

fn is_empty(edit: &WorkspaceEdit) -> bool {
    edit.files.is_empty() && edit.operations.is_empty()
}
