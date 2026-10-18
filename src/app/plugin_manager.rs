//! Which plugins the user has, which of them run, and the list that turns them
//! on and off. A plugin is a folder in the plugins folder (see
//! [`crate::plugins::manifest`]); the user decides about each, once, when they turn
//! it on and see what it asks to be allowed to do, and that decision is kept in
//! `state.json` (see [`crate::persist::saved_state`]).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use ratatui::layout::Position;

use super::App;
use crate::components::popups::confirm::Confirm;
use crate::components::popups::menu::{Entry, Menu};
use crate::event::action::Action;
use crate::persist::saved_state::PluginRecord;
use crate::plugin_api::Capability;
use crate::plugins::manifest::{self, Found, Host, Installed};
use crate::ui::PluginViewId;

/// The folder inside the config folder where plugins are.
const PLUGINS_FOLDER: &str = "plugins";

/// What is known of the plugins.
#[derive(Debug, Default)]
pub(super) struct Manager {
    /// What was in the plugins folder when it was last looked at.
    found: Vec<Found>,
    /// The plugins that are running.
    running: HashSet<Box<str>>,
    /// The plugins that were stopped because they failed too often, and why.
    stopped: HashMap<Box<str>, String>,
    /// Plugins whose user has to be asked before they run.
    waiting: VecDeque<Box<str>>,
}

impl Manager {
    fn installed(&self, id: &str) -> Option<&Installed> {
        self.found
            .iter()
            .filter_map(|found| found.as_ref().ok())
            .find(|installed| installed.id() == id)
    }

    fn broken(&self, id: &str) -> Option<String> {
        self.found.iter().find_map(|found| match found {
            Err((folder, error)) if folder == id => Some(error.to_string()),
            _ => None,
        })
    }
}

/// What a capability lets a plugin do, for the question to the user.
fn describe(capability: Capability) -> String {
    match capability {
        Capability::GitRead => {
            "read the state of the git repository: the branch and the changed files"
        }
        Capability::GitWrite => "change the git repository: stage and unstage files",
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
fn covers(record: &PluginRecord, installed: &Installed) -> bool {
    installed.capabilities.iter().all(|capability| {
        record
            .capabilities
            .iter()
            .any(|name| name == capability.name())
    })
}

impl App {
    fn plugins_dir(&self) -> Option<PathBuf> {
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
    fn start_plugin(&mut self, installed: &Installed) {
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
        if let Some(runtime) = &self.plugins.runtime {
            runtime.load(id, wasm, installed.capabilities.iter().copied());
        }
        self.plugins.manager.running.insert(id.into());
        self.plugins.manager.stopped.remove(id);
        self.apply_plugin_keys();
        self.needs_redraw = true;
    }

    /// Stops the plugin and takes its pane and actions away.
    fn stop_plugin(&mut self, id: &str) {
        if let Some(runtime) = &self.plugins.runtime {
            runtime.unload(id);
        }
        self.forget_plugin(id);
    }

    /// What of a plugin is in the editor: its actions, its pane, its keys.
    pub(super) fn forget_plugin(&mut self, id: &str) {
        self.plugins.actions.remove(id);
        self.plugins.manager.running.remove(id);
        self.state.remove_plugin_view(&PluginViewId::new(id));
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
            let allowed: Vec<String> = installed
                .capabilities
                .iter()
                .copied()
                .map(describe)
                .collect();
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
            },
        );
        self.start_plugin(&installed);
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

    /// Opens the list of the plugins in the plugins folder.
    pub(super) fn show_plugins(&mut self) {
        let Some(dir) = self.plugins_dir() else {
            self.state.notify("there is no folder for plugins here");
            return;
        };
        self.plugins.manager.found = manifest::scan(&dir, Host::THIS);
        if self.plugins.manager.found.is_empty() {
            self.state.notify(format!(
                "no plugins: put each in a folder of its own in {}",
                dir.display()
            ));
            return;
        }
        let entries: Vec<Entry> = self
            .plugins
            .manager
            .found
            .iter()
            .map(|found| match found {
                Ok(installed) => {
                    let id = installed.id();
                    let running = self.plugins.manager.running.contains(id);
                    let status = match (running, self.plugins.manager.stopped.get(id)) {
                        (true, _) => "on".to_owned(),
                        (false, Some(_)) => "off: it failed too often".to_owned(),
                        (false, None) => "off".to_owned(),
                    };
                    let mark = if running { "[x]" } else { "[ ]" };
                    Entry::Item {
                        label: format!(
                            "{mark} {} {}",
                            installed.manifest.display_name(),
                            installed.manifest.version
                        )
                        .into(),
                        action: Action::TogglePlugin(id.into()),
                        keys: Some(status),
                    }
                }
                Err((folder, _)) => Entry::Item {
                    label: format!("[!] {folder}").into(),
                    action: Action::TogglePlugin(folder.as_str().into()),
                    keys: Some("cannot run".to_owned()),
                },
            })
            .collect();
        let screen = self.state.screen();
        let menu = Menu::on_app(entries, Position::new(screen.x + 2, screen.y + 1))
            .starting_on_anchor_row();
        self.state.open_menu(menu);
        self.needs_redraw = true;
    }

    fn saved_plugin(&self, id: &str) -> Option<PluginRecord> {
        self.saved_state().plugin(id).cloned()
    }
}
