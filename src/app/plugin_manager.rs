//! Which plugins the user has, which of them run, and the list that turns them
//! on and off. A plugin is a folder in the plugins folder (see
//! [`crate::plugins::manifest`]); the user decides about each, once, when they turn
//! it on and see what it asks to be allowed to do, and that decision is kept in
//! `state.json` (see [`crate::persist::saved_state`]).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use super::App;
use crate::components::popups::confirm::Confirm;
use crate::persist::saved_state::PluginRecord;
use crate::plugin_api::{Capability, Thread};
use crate::plugins::manifest::{self, Found, Host, Installed};
use crate::ui::PluginViewId;

/// The folder inside the config folder where plugins are.
pub(super) const PLUGINS_FOLDER: &str = "plugins";

/// What is known of the plugins.
#[derive(Debug, Default)]
pub(super) struct Manager {
    /// What was in the plugins folder when it was last looked at.
    pub(super) found: Vec<Found>,
    /// The plugins that are running.
    running: HashSet<Box<str>>,
    /// The plugins that were stopped because they failed too often, and why.
    pub(super) stopped: HashMap<Box<str>, String>,
    /// Plugins whose user has to be asked before they run.
    pub(super) waiting: VecDeque<Box<str>>,
    /// The plugin that was chosen in the list when it closed to ask a question: the
    /// list comes back, with it chosen, when the questions are answered.
    pub(super) return_to: Option<Box<str>>,
}

impl Manager {
    pub(super) fn installed(&self, id: &str) -> Option<&Installed> {
        self.found
            .iter()
            .filter_map(|found| found.as_ref().ok())
            .find(|installed| installed.id() == id)
    }

    /// Whether the plugin has a pane: every plugin does unless its manifest says
    /// `view = false` (one without a manifest here, such as a test's, has one).
    pub(super) fn has_view(&self, id: &str) -> bool {
        self.installed(id)
            .is_none_or(|installed| installed.manifest.view)
    }

    /// The ids of the plugins that are running.
    pub(super) fn running_ids(&self) -> impl Iterator<Item = Box<str>> + '_ {
        self.running.iter().cloned()
    }

    /// Whether the plugin is running.
    pub(super) fn is_running(&self, id: &str) -> bool {
        self.running.contains(id)
    }

    pub(super) fn broken(&self, id: &str) -> Option<String> {
        self.found.iter().find_map(|found| match found {
            Err((folder, error)) if folder == id => Some(error.to_string()),
            _ => None,
        })
    }
}

/// What a capability lets a plugin do, for the question to the user.
pub(super) fn describe(capability: Capability, file_types: &[String], run: &[String]) -> String {
    if capability == Capability::FilesText {
        let kinds: Vec<String> = file_types.iter().map(|kind| format!(".{kind}")).collect();
        return match kinds.is_empty() {
            true => "receive the text of files (its manifest names no kind, so none)".to_owned(),
            false => format!(
                "see the text of the {} files you open, as saved on disk, and nothing else of them",
                kinds.join(", ")
            ),
        };
    }
    if capability == Capability::ProcessRun {
        let commands: Vec<String> = run.iter().map(|pattern| format!("`{pattern}`")).collect();
        return format!(
            "run commands, with no shell, in a project folder you trust: {} (this runs code of the project: its build scripts and tests)",
            commands.join(", ")
        );
    }
    match capability {
        Capability::GitRead => {
            "read the state of the git repository: the branch and the changed files"
        }
        Capability::GitWrite => "change the git repository: stage and unstage files",
        Capability::EditorOpen => "open files of the project in the editor",
        Capability::GitRemote => {
            "fetch from, pull from and push to the remotes of the git repository (uses the network)"
        }
        Capability::DebugStart => {
            "start programs under the debugger, in a project folder you trust (this runs code of the project)"
        }
        Capability::PluginCall => "ask other plugins to do things, and hear their answers",
        Capability::PluginServe => {
            "be asked by other plugins to do things, with everything this plugin is allowed to do"
        }
        _ => capability.name(),
    }
    .to_owned()
}

fn names(capabilities: &[Capability]) -> Vec<String> {
    capabilities
        .iter()
        .map(|capability| capability.name().to_owned())
        .collect()
}

/// Whether what the user decided about a plugin covers everything it asks for now.
pub(super) fn covers(record: &PluginRecord, installed: &Installed) -> bool {
    let capabilities = installed.capabilities.iter().all(|capability| {
        record
            .capabilities
            .iter()
            .any(|name| name == capability.name())
    });
    // Seeing more kinds of file than the user was shown is a new thing to allow.
    let file_types = !installed.capabilities.contains(&Capability::FilesText)
        || installed
            .manifest
            .file_types
            .iter()
            .all(|kind| record.file_types.contains(&kind.to_ascii_lowercase()));
    // A thread of its own is a share of a processor: also something to allow.
    let thread = installed.manifest.thread != Thread::Own || record.own_thread;
    // Running a command the user was not shown is a new thing to allow.
    let run = installed
        .run
        .iter()
        .all(|pattern| record.run.iter().any(|shown| shown == pattern.as_str()));
    capabilities && file_types && thread && run
}

/// What a plugin asks that is not a capability, for the question: a thread of its own.
pub(super) fn extra_asks(installed: &Installed) -> Vec<String> {
    match installed.manifest.thread {
        Thread::Own => vec![
            "run on a thread of its own, and use a lot of processor time when it has a lot to do"
                .to_owned(),
        ],
        Thread::Shared => Vec::new(),
    }
}

/// The commands of `installed` as the user is shown them, as the manifest writes them.
pub(super) fn shown_run(installed: &Installed) -> Vec<String> {
    installed
        .run
        .iter()
        .map(|pattern| pattern.as_str().to_owned())
        .collect()
}

/// The file types of the manifest of `installed` as the user is shown them: lower case.
pub(super) fn shown_file_types(installed: &Installed) -> Vec<String> {
    match installed.capabilities.contains(&Capability::FilesText) {
        true => installed
            .manifest
            .file_types
            .iter()
            .map(|kind| kind.to_ascii_lowercase())
            .collect(),
        false => Vec::new(),
    }
}

impl App {
    pub(super) fn plugins_dir(&self) -> Option<PathBuf> {
        Some(self.config_dir.as_deref()?.join(PLUGINS_FOLDER))
    }

    /// Looks at the plugins folder and starts the plugins the user turned on, asking
    /// about the ones that want more than they were allowed.
    pub(super) fn load_installed_plugins(&mut self) {
        let Some(dir) = self.plugins_dir() else {
            return;
        };
        self.plugins.manager.found = manifest::scan(&dir, Host::THIS);
        let found = self.plugins.manager.found.clone();
        for entry in found {
            match entry {
                Ok(installed) => self.start_if_turned_on(installed),
                Err((folder, error)) => {
                    if self
                        .saved_plugin(&folder)
                        .is_some_and(|record| record.enabled)
                    {
                        self.state
                            .notify_error(format!("plugin {folder} cannot run: {error}"));
                    }
                }
            }
        }
        self.ask_about_next_plugin();
    }

    fn start_if_turned_on(&mut self, installed: Installed) {
        let Some(record) = self.saved_plugin(installed.id()) else {
            return;
        };
        if !record.enabled {
            return;
        }
        match covers(&record, &installed) {
            true => self.start_plugin(&installed),
            false => self
                .plugins
                .manager
                .waiting
                .push_back(installed.id().into()),
        }
    }

    /// Reads the module and starts it with what it declared.
    pub(super) fn start_plugin(&mut self, installed: &Installed) {
        let id = installed.id();
        let wasm = match installed.module() {
            Ok(wasm) => wasm,
            Err(error) => {
                self.state
                    .notify_error(format!("plugin {id} cannot run: {error}"));
                return;
            }
        };
        self.plugins
            .actions
            .insert(id.into(), installed.manifest.actions.clone());
        self.publish_workspace_to_plugins();
        if let Some(runtime) = &self.plugins.runtime {
            runtime.load_with(
                id,
                wasm,
                installed.capabilities.iter().copied(),
                installed.manifest.thread,
                installed.run.clone(),
            );
        }
        self.plugins.manager.running.insert(id.into());
        self.plugins.manager.stopped.remove(id);
        self.forget_pushed(id);
        self.apply_plugin_places();
        self.apply_plugin_keys();
        self.needs_redraw = true;
    }

    /// Stops the plugin and takes its pane and actions away.
    pub(super) fn stop_plugin(&mut self, id: &str) {
        if let Some(runtime) = &self.plugins.runtime {
            runtime.unload(id);
        }
        self.forget_plugin(id);
    }

    /// What of a plugin is in the editor: its actions, its pane, its keys.
    pub(super) fn forget_plugin(&mut self, id: &str) {
        self.plugins.actions.remove(id);
        self.plugins.manager.running.remove(id);
        if self
            .state
            .plugin_popup()
            .is_some_and(|open| open.as_str() == id)
        {
            self.state.close_plugin_popup();
        }
        self.state.remove_plugin_view(&PluginViewId::new(id));
        self.apply_plugin_places();
        self.apply_plugin_keys();
        self.needs_redraw = true;
    }

    /// The plugin was stopped because it failed too often.
    pub(super) fn plugin_stopped_for_failing(&mut self, id: &str, reason: &str) {
        self.forget_plugin(id);
        self.plugins
            .manager
            .stopped
            .insert(id.into(), reason.to_owned());
    }

    /// Asks about the first plugin that waits for an answer, when no other question
    /// is open.
    pub(super) fn ask_about_next_plugin(&mut self) {
        if !matches!(self.state.popup(), super::state::Popup::None) {
            return;
        }
        while let Some(id) = self.plugins.manager.waiting.pop_front() {
            let Some(installed) = self.plugins.manager.installed(&id) else {
                continue;
            };
            let kinds = shown_file_types(installed);
            let run = shown_run(installed);
            let mut allowed: Vec<String> = installed
                .capabilities
                .iter()
                .map(|capability| describe(*capability, &kinds, &run))
                .collect();
            allowed.extend(extra_asks(installed));
            let confirm = Confirm::enable_plugin(
                &id,
                installed.manifest.display_name(),
                &installed.manifest.version,
                &allowed,
            );
            self.state.open_confirm(confirm);
            self.needs_redraw = true;
            return;
        }
        // What the plugins waiting for an answer were asked comes first.
        self.ask_about_trust();
    }

    /// The user said yes: what they were shown is remembered and the plugin runs.
    pub(super) fn allow_plugin(&mut self, id: &str) {
        let Some(installed) = self.plugins.manager.installed(id).cloned() else {
            return;
        };
        self.remember_plugin(
            id,
            PluginRecord {
                enabled: true,
                capabilities: names(&installed.capabilities),
                file_types: shown_file_types(&installed),
                own_thread: installed.manifest.thread == Thread::Own,
                run: shown_run(&installed),
                place: self.saved_plugin(id).and_then(|record| record.place),
            },
        );
        self.start_plugin(&installed);
        // Where it goes is the next question, unless it has no pane, the layout file says
        // or other plugins still wait for their answer (they are asked first).
        if !installed.manifest.view {
            return;
        }
        let unplaced = self
            .saved_plugin(id)
            .and_then(|record| record.place)
            .is_none();
        if unplaced && !self.layout_places(id) && self.plugins.manager.waiting.is_empty() {
            self.ask_where(id);
        }
    }

    /// The user said no: the plugin stays off, and what it was allowed before stays.
    pub(super) fn refuse_plugin(&mut self, id: &str) {
        let mut record = self.saved_plugin(id).unwrap_or_default();
        record.enabled = false;
        self.remember_plugin(id, record);
        self.state.notify(format!(
            "plugin {id} stays off (Plugins: Manage turns it on)"
        ));
    }

    /// Turns the plugin on or off.
    pub(super) fn toggle_plugin(&mut self, id: &str) {
        if let Some(problem) = self.plugins.manager.broken(id) {
            self.state
                .notify_error(format!("plugin {id} cannot run: {problem}"));
            return;
        }
        if self.plugins.manager.running.contains(id) {
            let mut record = self.saved_plugin(id).unwrap_or_default();
            record.enabled = false;
            self.remember_plugin(id, record);
            self.stop_plugin(id);
            self.state.notify(format!("plugin {id} is off"));
            return;
        }
        let Some(installed) = self.plugins.manager.installed(id).cloned() else {
            self.state.notify(format!("no plugin named {id}"));
            return;
        };
        match self.saved_plugin(id) {
            // Asked before, about all it asks for now: no need to ask again.
            Some(record) if covers(&record, &installed) => {
                self.remember_plugin(
                    id,
                    PluginRecord {
                        run: Vec::new(),
                        enabled: true,
                        ..record
                    },
                );
                self.start_plugin(&installed);
            }
            _ => {
                self.plugins.manager.waiting.push_front(id.into());
                self.ask_about_next_plugin();
            }
        }
    }

    pub(super) fn saved_plugin(&self, id: &str) -> Option<PluginRecord> {
        self.saved_state().plugin(id).cloned()
    }

    pub(super) fn saved_plugin_record(&self, id: &str) -> Option<PluginRecord> {
        self.saved_plugin(id)
    }
}
