//! Plugins: the thread they run on, what they tell the app, and how keys, the
//! palette and the mouse reach them.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::App;
use crate::buffer::Position;
use crate::components::popups::menu::{Entry, Menu};
use crate::event::Event;
use crate::event::action::Action;
use crate::git::ActionError;
use crate::git::GitError;
use crate::plugin_api::{ActionDecl, Refusal, ToPlugin, WireMenuItem};
use crate::plugins::{Notice, Runtime, WriteOp};
use crate::ui::view::{ViewNode, button_at, input_column_at, rows_scroll};
use crate::ui::{ComponentKind, PluginViewId};

/// The plugin thread, once the app can hear from it, and the actions each
/// plugin declared.
#[derive(Debug, Default)]
pub(super) struct Plugins {
    pub(super) runtime: Option<Runtime>,
    pub(super) actions: HashMap<Box<str>, Vec<ActionDecl>>,
    pub(super) manager: super::plugin_manager::Manager,
    pub(super) store: super::plugin_list::Store,
    /// The fetch, pull or push that is running, if one is: only one runs at a time.
    pub(super) remote: Option<RemoteJob>,
    /// What each plugin with `files.text` was last told about the file in front.
    pub(super) pushed: HashMap<Box<str>, super::plugin_files::Pushed>,
    /// The size of its pane each plugin was last told.
    pub(super) sized: HashMap<Box<str>, (u16, u16)>,
    /// Plugins that asked to run commands in a folder that is not trusted, and whether the
    /// user was asked.
    pub(super) trust: super::plugin_trust::Ask,
    /// The plugin that was right-clicked, and where: the menu it answers with is shown there.
    pub(super) menu_for: Option<(Box<str>, ratatui::layout::Position)>,
}

/// A fetch, pull or push on its way.
#[derive(Debug)]
pub(super) struct RemoteJob {
    plugin: Box<str>,
    request: u64,
    what: &'static str,
    cancel: Arc<AtomicBool>,
}

impl App {
    /// Starts the thread plugins run on; what it says comes back as events.
    pub(super) fn start_plugin_runtime(&mut self) {
        let Some(events) = self.background.sender() else {
            return;
        };
        self.plugins.runtime = Some(Runtime::spawn(move |notice| {
            let _ = events.blocking_send(Event::Plugin(notice));
        }));
    }

    /// What the plugin thread said: a new view, or what happened to a plugin.
    pub(super) fn plugin_notice(&mut self, notice: Notice) {
        match notice {
            // A plugin without a pane has nothing to draw into.
            Notice::View { plugin, .. } if !self.plugins.manager.has_view(&plugin) => {}
            Notice::View { plugin, content } => {
                let id = PluginViewId::new(&*plugin);
                self.state.set_plugin_view(id.clone(), content);
                self.state.plugin_view_changed(&id);
            }
            Notice::Restarted { plugin, reason } => {
                // A fresh instance knows nothing: it is told about the file again.
                self.forget_pushed(&plugin);
                self.state
                    .notify(format!("plugin {plugin} restarted: {reason}"))
            }
            Notice::Disabled { plugin, reason } => {
                self.state.notify_error(format!(
                    "plugin {plugin} is off: it failed three times within a minute (last: {reason})"
                ));
                self.plugin_stopped_for_failing(&plugin, &reason);
            }
            Notice::Broken { plugin, reason } => self
                .state
                .notify_error(format!("plugin {plugin} cannot run: {reason}")),
            Notice::Rejected { plugin, reason } => self
                .state
                .notify_error(format!("plugin {plugin} said something wrong: {reason}")),
            Notice::Menu { plugin, items } => self.plugin_menu(&plugin, &items),
            Notice::NeedsTrust { plugin } => self.plugin_needs_trust(&plugin),
            Notice::Popup {
                plugin,
                title,
                text,
            } => self.plugin_text_popup(&plugin, title, text),
            Notice::Debug {
                plugin,
                request,
                program,
                args,
                env,
                cwd,
            } => self.plugin_debug(&plugin, request, (program, args, env), cwd),
            Notice::GitWrite {
                plugin,
                request,
                op,
            } => self.plugin_git_write(&plugin, request, op),
            Notice::OpenFile {
                plugin,
                request,
                path,
                line,
                column,
            } => self.plugin_open_file(&plugin, request, &path, line.zip(column.or(Some(1)))),
        }
        self.needs_redraw = true;
    }

    /// Hands the plugins the state of the repository (or that there is none).
    pub(super) fn publish_git_to_plugins(&self) {
        let Some(runtime) = &self.plugins.runtime else {
            return;
        };
        let generation = self.git_generation();
        runtime.publish_git(self.state.git_status().map(|status| (generation, status)));
    }

    /// A plugin with the capability asked to open a file of the repository (or of
    /// the project, when there is no repository): the path is checked like the
    /// path of any action, the file opens and the keyboard stays where it was, and
    /// the plugin is told how it went. With a place (line and column, from 1) the cursor
    /// goes there, held to what the file has, and the keyboard goes to the editor.
    fn plugin_open_file(&mut self, plugin: &str, request: u64, path: &str, at: Option<(u32, u32)>) {
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
        let root = match self.state.git_status() {
            Some(status) => status.root.clone(),
            None => self.project_root(),
        };
        let full = root.join(&relative);
        // `alt+,` comes back from where the click on a test took the user.
        if at.is_some()
            && full.is_file()
            && let Some(here) = self.here()
        {
            self.record_place(here);
        }
        let reply = match full.is_file() {
            false => refused(
                request,
                Refusal::Failed {
                    detail: format!("`{relative}` is not a file"),
                },
            ),
            true => match self.state.workspace_mut().open_path(&full) {
                Ok(_) => {
                    if let Some((line, column)) = at {
                        self.show_place(line, column);
                    }
                    ToPlugin::Done { request }
                }
                Err(error) => refused(
                    request,
                    Refusal::Failed {
                        detail: error.to_string(),
                    },
                ),
            },
        };
        self.send_to_plugin(plugin, reply);
        self.needs_redraw = true;
    }

    /// Puts the cursor of the file in front on `line` and `column` (from 1, held to what
    /// the file has) and gives the editor the keyboard.
    pub(super) fn show_place(&mut self, line: u32, column: u32) {
        let at = Position::new(
            (line as usize).saturating_sub(1),
            (column as usize).saturating_sub(1),
        );
        self.state.set_focus(super::state::Focus::Editor);
        self.state.edit(|editor| {
            let at = editor.buffer().clamp_position(at);
            editor.go_to(at);
        });
    }

    /// A plugin with the capability asked to change the repository: it is done
    /// in the background, and the plugin hears how it went.
    fn plugin_git_write(&mut self, plugin: &str, request: u64, op: WriteOp) {
        let (git, root) = self.git_tool();
        let (Some(events), Some(root)) = (self.background.sender(), root) else {
            self.send_to_plugin(plugin, refused(request, Refusal::NoRepository));
            return;
        };
        let plugin: Box<str> = plugin.into();
        if op == WriteOp::Cancel {
            self.cancel_remote();
            self.send_to_plugin(&plugin, ToPlugin::Done { request });
            return;
        }
        let remote = match op {
            WriteOp::Fetch => Some("fetch"),
            WriteOp::Pull => Some("pull"),
            WriteOp::Push => Some("push"),
            _ => None,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        if let Some(what) = remote {
            if self.plugins.remote.is_some() {
                self.send_to_plugin(&plugin, refused(request, Refusal::Busy));
                return;
            }
            self.plugins.remote = Some(RemoteJob {
                plugin: plugin.clone(),
                request,
                what,
                cancel: Arc::clone(&cancel),
            });
        }
        std::thread::spawn(move || {
            let result = match &op {
                WriteOp::Fetch => git.fetch(&root, &cancel),
                WriteOp::Pull => git.pull(&root, &cancel),
                WriteOp::Push => git.push(&root, &cancel),
                WriteOp::Cancel => Ok(()),
                WriteOp::Stage(path) => git.stage(&root, path),
                WriteOp::Unstage(path) => git.unstage(&root, path),
                WriteOp::StageAll => git.stage_all(&root),
                WriteOp::UnstageAll => git.unstage_all(&root),
                WriteOp::Commit(message) => git.commit(&root, message),
            };
            let _ = events.blocking_send(Event::PluginGit {
                plugin,
                request,
                result,
            });
        });
    }

    /// A change a plugin asked for was done, or not: the plugin is told, and
    /// what git shows is read again at once.
    pub(super) fn plugin_git_done(
        &mut self,
        plugin: &str,
        request: u64,
        result: Result<(), ActionError>,
    ) {
        if self
            .plugins
            .remote
            .as_ref()
            .is_some_and(|job| &*job.plugin == plugin && job.request == request)
        {
            self.plugins.remote = None;
        }
        // The plugin cannot show a message, and what git or a hook printed is what
        // the user needs: it is shown here, until it is dismissed.
        if let Err(error) = &result {
            self.tell_about_failure(error);
        }
        let reply = match result {
            Ok(()) => ToPlugin::Done { request },
            Err(ActionError::BadArgument(detail)) => {
                refused(request, Refusal::BadArgument { detail })
            }
            Err(ActionError::NothingStaged) => refused(request, Refusal::NothingStaged),
            Err(ActionError::NeedsAuthentication(detail)) => {
                refused(request, Refusal::AuthRequired { detail })
            }
            Err(ActionError::Git(GitError::Cancelled)) => refused(request, Refusal::Cancelled),
            Err(ActionError::Git(GitError::NotInstalled | GitError::NoRepository(_))) => {
                refused(request, Refusal::NoRepository)
            }
            Err(ActionError::Git(error)) => refused(
                request,
                Refusal::Failed {
                    detail: error.to_string(),
                },
            ),
        };
        self.send_to_plugin(plugin, reply);
        self.git_changed_now();
    }

    /// Shows why a change to the repository failed (not a stop, and not a refusal
    /// of an argument, which the plugin shows itself).
    fn tell_about_failure(&mut self, error: &ActionError) {
        let message = match error {
            ActionError::NeedsAuthentication(said) => {
                format!("git: {said}; {}", crate::git::AUTH_HINT)
            }
            ActionError::Git(GitError::Cancelled)
            | ActionError::Git(GitError::NotInstalled | GitError::NoRepository(_))
            | ActionError::BadArgument(_)
            | ActionError::NothingStaged => return,
            ActionError::Git(error) => error.to_string(),
        };
        self.state.notify_error(message);
    }

    /// Stops the fetch, pull or push that is running; it ends within a moment and
    /// its plugin hears that it was stopped. Returns whether one was running.
    pub(super) fn cancel_remote(&mut self) -> bool {
        let Some(job) = &self.plugins.remote else {
            return false;
        };
        job.cancel.store(true, Ordering::Relaxed);
        self.state.notify(format!("stopping git {}", job.what));
        true
    }

    /// `esc` in the pane of the plugin that started a fetch, pull or push stops it
    /// (and goes no further: the pane keeps the keyboard).
    pub(super) fn esc_stops_remote(&mut self) -> bool {
        let Some(job) = &self.plugins.remote else {
            return false;
        };
        let in_its_pane = self
            .state
            .focused_plugin()
            .is_some_and(|view| view.as_str() == &*job.plugin);
        in_its_pane && self.cancel_remote()
    }

    pub(super) fn send_to_plugin(&self, plugin: &str, message: ToPlugin) {
        if let Some(runtime) = &self.plugins.runtime {
            runtime.send(plugin, message);
        }
    }

    /// Every action of every plugin as the action to run and the title the
    /// palette shows, `plugin: Title`.
    pub(super) fn plugin_palette_actions(&self) -> Vec<(Action, String)> {
        let mut found: Vec<_> = self
            .plugins
            .actions
            .iter()
            .flat_map(|(plugin, actions)| {
                actions.iter().map(move |action| {
                    let title = match action.title.is_empty() {
                        true => &action.id,
                        false => &action.title,
                    };
                    (
                        Action::Plugin(format!("{plugin}:{}", action.id).into()),
                        format!("{plugin}: {title}"),
                    )
                })
            })
            .collect();
        // The plugins the user put in a popup are opened from the palette too.
        for (id, record) in self.saved_state().plugins() {
            if record.place == Some(crate::persist::saved_state::Place::Popup)
                && self.plugins.manager.is_running(id)
            {
                found.push((
                    Action::OpenPluginPopup(id.into()),
                    format!("plugins: open {id}"),
                ));
            }
        }
        found.sort_by(|a, b| a.1.cmp(&b.1));
        found
    }

    /// The keys a plugin declared for an action, to show beside it in the palette.
    pub(super) fn plugin_declared_keys(&self, action: &Action) -> Option<String> {
        let Action::Plugin(full) = action else {
            return None;
        };
        let (plugin, id) = full.split_once(':')?;
        let declared = self.plugins.actions.get(plugin)?;
        declared.iter().find(|decl| decl.id == id)?.keys.clone()
    }

    /// Runs an action of a plugin, written `plugin:action`: the plugin is told.
    pub(super) fn run_plugin_action(&mut self, full: &str) {
        let known = full.split_once(':').filter(|(plugin, id)| {
            self.plugins
                .actions
                .get(*plugin)
                .is_some_and(|actions| actions.iter().any(|action| action.id == *id))
        });
        match known {
            Some((plugin, id)) => self.send_to_plugin(plugin, ToPlugin::action(id)),
            None => self.state.notify(format!("no plugin action named {full}")),
        }
    }

    /// Shows the plugin's view if it is hidden and gives it the keyboard, with the
    /// keys the plugin declared.
    pub(super) fn focus_plugin_view(&mut self, id: &str) {
        let view = PluginViewId::new(id);
        if self.state.plugin_view(&view).is_none() {
            self.state.notify(format!("no plugin view named {id}"));
            return;
        }
        if !self.state.is_showing(&ComponentKind::Plugin(view.clone())) {
            self.state.toggle_plugin_view(&view);
        }
        self.state.focus_plugin(view);
        self.apply_plugin_keys();
    }

    /// Puts in the keymap the keys of the plugin that has the keyboard (and takes
    /// out the ones of the plugin before it).
    pub(super) fn apply_plugin_keys(&mut self) {
        let bindings: Vec<(String, Action)> = self
            .state
            .focused_plugin()
            .and_then(|view| {
                let plugin = view.as_str();
                let declared = self.plugins.actions.get(plugin)?;
                Some(
                    declared
                        .iter()
                        .filter_map(|decl| {
                            let keys = decl.keys.clone()?;
                            let action = Action::Plugin(format!("{plugin}:{}", decl.id).into());
                            Some((keys, action))
                        })
                        .collect(),
                )
            })
            .unwrap_or_default();
        if let Some(error) = self.keyboard.set_plugin_defaults(&bindings).first() {
            self.state
                .notify_error(format!("a key of a plugin cannot be used: {error}"));
        }
    }

    /// A key while a plugin shows a line to type into: `esc` ends the typing and
    /// the plugin is told; every other key goes to the plugin.
    pub(super) fn handle_plugin_input_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let Some(plugin) = self
            .state
            .focused_plugin()
            .map(|view| view.as_str().to_owned())
        else {
            return;
        };
        self.needs_redraw = true;
        if key.code == KeyCode::Esc {
            self.state.end_plugin_input();
            self.send_to_plugin(&plugin, ToPlugin::InputEnded);
            return;
        }
        if let Some(message) = key_message(key) {
            self.send_to_plugin(&plugin, message);
        }
    }

    /// What a click at the cell is on in the plugin's view: the part of it under
    /// the pointer, as the message that tells the plugin. `None` when it is on
    /// nothing that takes a click (text, a button that cannot be pressed).
    fn click_message(&self, view: &PluginViewId, column: u16, row: u16) -> Option<ToPlugin> {
        let content = self.state.plugin_view(view)?.content();
        let body = self.state.plugin_body(view)?;
        let at = ratatui::layout::Position::new(column, row);
        let (area, part) = content
            .leaves(body)
            .into_iter()
            .find(|(area, _)| area.contains(at))?;
        match part {
            ViewNode::Rows { rows, selected } => {
                let top = rows_scroll(*selected, rows.len(), usize::from(area.height));
                let clicked = top + usize::from(row - area.y);
                (clicked < rows.len()).then_some(ToPlugin::Click { row: clicked })
            }
            ViewNode::Buttons(buttons) => {
                let at = button_at(buttons, area.width, row - area.y, column - area.x)?;
                let button = buttons.get(at)?;
                button.enabled.then(|| ToPlugin::Press {
                    button: button.id.to_string(),
                })
            }
            ViewNode::Input { prompt, value, .. } => Some(ToPlugin::InputClick {
                column: input_column_at(prompt, value, column - area.x),
            }),
            _ => None,
        }
    }

    /// Pasted text into a plugin's line to type into: it goes to the plugin.
    /// Returns whether it did.
    pub(super) fn paste_into_plugin(&mut self, text: &str) -> bool {
        if !self.state.plugin_input() {
            return false;
        }
        if let Some(plugin) = self
            .state
            .focused_plugin()
            .map(|view| view.as_str().to_owned())
        {
            self.send_to_plugin(&plugin, ToPlugin::text(text));
        }
        true
    }

    /// A click on a plugin's pane gives it the keyboard. Returns whether the
    /// click was on one.
    pub(super) fn click_in_plugin_pane(&mut self, column: u16, row: u16) -> bool {
        let Some(view) = self.state.plugin_at(column, row) else {
            return false;
        };
        // What was clicked is worked out before the focus moves: the plugin's reply
        // to the click is what changes the view.
        let message = self.click_message(&view, column, row);
        self.focus_plugin_view(view.as_str());
        if let Some(message) = message {
            self.send_to_plugin(view.as_str(), message);
        }
        true
    }

    /// A right click on a plugin's rows: the plugin is told which row, and answers with the
    /// menu to show there. Returns whether the click was on rows of a plugin.
    pub(super) fn right_click_in_plugin_pane(&mut self, column: u16, row: u16) -> bool {
        let Some(view) = self.state.plugin_at(column, row) else {
            return false;
        };
        let Some(clicked) = self.rows_row_at(&view, column, row) else {
            return false;
        };
        let at = ratatui::layout::Position::new(column, row);
        self.plugins.menu_for = Some((view.as_str().into(), at));
        self.send_to_plugin(view.as_str(), ToPlugin::ContextClick { row: clicked });
        true
    }

    /// The row of the plugin's rows that the cell is on, counted as in a click on it.
    fn rows_row_at(&self, view: &PluginViewId, column: u16, row: u16) -> Option<usize> {
        let content = self.state.plugin_view(view)?.content();
        let body = self.state.plugin_body(view)?;
        let at = ratatui::layout::Position::new(column, row);
        let (area, part) = content
            .leaves(body)
            .into_iter()
            .find(|(area, _)| area.contains(at))?;
        let ViewNode::Rows { rows, selected } = part else {
            return None;
        };
        let top = rows_scroll(*selected, rows.len(), usize::from(area.height));
        let clicked = top + usize::from(row - area.y);
        (clicked < rows.len()).then_some(clicked)
    }

    /// The menu a plugin answered a right click with, shown where the pointer was: each item is
    /// an action of the plugin, with the title and keys its manifest gives, and the editor's
    /// own menu does the rest. What nobody asked for, and actions the plugin does not have, are
    /// left out.
    fn plugin_menu(&mut self, plugin: &str, items: &[WireMenuItem]) {
        let Some((asked, at)) = self.plugins.menu_for.clone() else {
            return;
        };
        if &*asked != plugin {
            return;
        }
        self.plugins.menu_for = None;
        if !matches!(
            self.state.popup(),
            super::state::Popup::None | super::state::Popup::PluginView(_)
        ) {
            return;
        }
        let mut entries: Vec<Entry> = Vec::new();
        for item in items {
            match item {
                WireMenuItem::Action { action } => {
                    let Some(entry) = self.plugin_menu_entry(plugin, action) else {
                        continue;
                    };
                    entries.push(entry);
                }

                // A rule is between items: not first, not twice.
                WireMenuItem::Separator
                    if !matches!(entries.last(), None | Some(Entry::Separator)) =>
                {
                    entries.push(Entry::Separator);
                }
                _ => {}
            }
        }
        while matches!(entries.last(), Some(Entry::Separator)) {
            entries.pop();
        }
        if entries.is_empty() {
            return;
        }
        self.state.open_menu(Menu::on_app(entries, at));
        self.needs_redraw = true;
    }

    /// The item of a menu for the action `id` of the plugin, if its manifest has it.
    fn plugin_menu_entry(&self, plugin: &str, id: &str) -> Option<Entry> {
        let declared = self.plugins.actions.get(plugin)?;
        let decl = declared.iter().find(|decl| decl.id == id)?;
        let action = Action::Plugin(format!("{plugin}:{id}").into());
        let label = match decl.title.is_empty() {
            true => decl.id.clone(),
            false => decl.title.clone(),
        };
        Some(Entry::Item {
            label: label.into(),
            keys: self.plugin_declared_keys(&action),
            action,
        })
    }

    /// Loads a plugin from its module and starts it, with the actions it declares.
    #[cfg(test)]
    pub(crate) fn load_plugin(
        &mut self,
        plugin: &str,
        wasm: Vec<u8>,
        actions: Vec<ActionDecl>,
        grants: &[crate::plugin_api::Capability],
    ) {
        self.plugins.actions.insert(plugin.into(), actions);
        if let Some(runtime) = &self.plugins.runtime {
            runtime.load(plugin, wasm, grants.iter().copied());
        }
        self.apply_plugin_keys();
    }

    /// Tells a plugin that something changed.
    #[cfg(test)]
    pub(crate) fn refresh_plugin(&self, plugin: &str) {
        self.send_to_plugin(plugin, ToPlugin::Refresh);
    }
}

fn refused(request: u64, reason: Refusal) -> ToPlugin {
    ToPlugin::Refused { request, reason }
}

/// What a key is to a plugin's line to type into: text when it is a character
/// typed without `ctrl` or `alt`, a key otherwise; `None` for a key the
/// protocol has no name for.
fn key_message(key: KeyEvent) -> Option<ToPlugin> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let named = |name: &str, shift: bool| Some(ToPlugin::key(name, ctrl, alt, shift));
    match key.code {
        KeyCode::Char(typed) if ctrl || alt => {
            Some(ToPlugin::key(typed.to_string(), ctrl, alt, shift))
        }
        KeyCode::Char(typed) => Some(ToPlugin::text(typed)),
        KeyCode::Backspace => named("backspace", shift),
        KeyCode::Delete => named("delete", shift),
        KeyCode::Enter => named("enter", shift),
        KeyCode::Tab => named("tab", shift),
        KeyCode::BackTab => named("tab", true),
        KeyCode::Left => named("left", shift),
        KeyCode::Right => named("right", shift),
        KeyCode::Up => named("up", shift),
        KeyCode::Down => named("down", shift),
        KeyCode::Home => named("home", shift),
        KeyCode::End => named("end", shift),
        KeyCode::PageUp => named("page_up", shift),
        KeyCode::PageDown => named("page_down", shift),
        _ => None,
    }
}
