//! Plugins: the thread they run on, what they tell the app, and how keys, the
//! palette and the mouse reach them.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::App;
use crate::event::Event;
use crate::event::action::Action;
use crate::git::ActionError;
use crate::git::GitError;
use crate::plugin_api::{ActionDecl, Refusal, ToPlugin};
use crate::plugins::{Notice, Runtime, WriteOp};
use crate::ui::{ComponentKind, PluginViewId};

/// The plugin thread, once the app can hear from it, and the actions each
/// plugin declared.
#[derive(Debug, Default)]
pub(super) struct Plugins {
    runtime: Option<Runtime>,
    actions: HashMap<Box<str>, Vec<ActionDecl>>,
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
            Notice::View { plugin, content } => {
                let id = PluginViewId::new(&*plugin);
                self.state.set_plugin_view(id.clone(), content);
                self.state.plugin_view_changed(&id);
            }
            Notice::Restarted { plugin, reason } => self
                .state
                .notify(format!("plugin {plugin} restarted: {reason}")),
            Notice::Disabled { plugin, reason } => self.state.notify_error(format!(
                "plugin {plugin} is off: it failed three times within a minute (last: {reason})"
            )),
            Notice::Broken { plugin, reason } => self
                .state
                .notify_error(format!("plugin {plugin} cannot run: {reason}")),
            Notice::Rejected { plugin, reason } => self
                .state
                .notify_error(format!("plugin {plugin} said something wrong: {reason}")),
            Notice::GitWrite {
                plugin,
                request,
                op,
            } => self.plugin_git_write(&plugin, request, op),
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

    /// A plugin with the capability asked to change the repository: it is done
    /// in the background, and the plugin hears how it went.
    fn plugin_git_write(&mut self, plugin: &str, request: u64, op: WriteOp) {
        let (git, root) = self.git_tool();
        let (Some(events), Some(root)) = (self.background.sender(), root) else {
            self.send_to_plugin(plugin, refused(request, Refusal::NoRepository));
            return;
        };
        let plugin: Box<str> = plugin.into();
        std::thread::spawn(move || {
            let result = match &op {
                WriteOp::Stage(path) => git.stage(&root, path),
                WriteOp::Unstage(path) => git.unstage(&root, path),
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
        let reply = match result {
            Ok(()) => ToPlugin::Done { request },
            Err(ActionError::BadArgument(detail)) => {
                refused(request, Refusal::BadArgument { detail })
            }
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

    fn send_to_plugin(&self, plugin: &str, message: ToPlugin) {
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
        self.focus_plugin_view(view.as_str());
        true
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
