//! The plugin manager popup: the plugins of the index and the ones in the plugins
//! folder in one list, and what the user does with them: install, update, remove,
//! turn on or off, place. Downloading runs on a thread of its own and tells the app
//! by events; nothing is written in the plugins folder until the module is checked.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::KeyEvent;
use ratatui::layout::Position;

use super::App;
use crate::components::popups::confirm::Confirm;
use crate::components::popups::menu::{Entry, Menu};
use crate::components::popups::plugin_list::{Command, Mark, Note, PluginList, Row};
use crate::components::popups::plugin_readme::{self, Body, Readme};
use crate::event::Event;
use crate::event::action::Action;
use crate::persist::saved_state::Place;
use crate::plugins::index::{Availability, Entry as IndexEntry, Index};
use crate::plugins::install::{self, InstallError, JobEvent, Policy, Stage};
use crate::plugins::manifest::{self, Host, Installed, is_newer};
use crate::ui::layout::RIGHT_BAR_MIN_WIDTH;

/// Where the index is fetched from and what may be downloaded.
#[derive(Debug, Clone)]
pub(super) struct Source {
    /// `None`: nothing is fetched, the index is what the release came with.
    index_url: Option<String>,
    policy: Policy,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            // Tests never reach for the network unless they say so.
            index_url: (!cfg!(test)).then(|| install::INDEX_URL.to_owned()),
            policy: Policy::GITHUB,
        }
    }
}

#[cfg(test)]
impl Source {
    pub(super) fn local(index_url: String, policy: Policy) -> Self {
        Self {
            index_url: Some(index_url),
            policy,
        }
    }
}

/// The entry that is a newer version of `installed`, from the source it came from,
/// when it can be installed here: a plugin of your own has none.
fn update_for<'a>(installed: &Installed, entry: Option<&'a IndexEntry>) -> Option<&'a IndexEntry> {
    entry
        .filter(|_| install::marker(&installed.dir).is_some())
        .filter(|entry| is_newer(&entry.version, &installed.manifest.version))
        .filter(|entry| entry.source == installed_source(installed))
        .filter(|entry| entry.availability(Host::THIS) == Availability::Ready)
}

/// The source a plugin of the manager came from; `None` is the official index.
fn installed_source(installed: &Installed) -> Option<String> {
    install::marker(&installed.dir).and_then(|marker| marker.entry.source)
}

/// What the plugin is, under the list; a source's plugin says so first.
fn described(entry: &IndexEntry) -> String {
    match entry.source_label() {
        Some(label) => format!("from {label}, not official: {}", entry.description),
        None => entry.description.clone(),
    }
}

/// An install that is running.
#[derive(Debug)]
pub(super) struct Job {
    id: Box<str>,
    cancel: Arc<AtomicBool>,
}

/// What is known of the index and of the install in progress.
#[derive(Debug, Default)]
pub(super) struct Store {
    /// The index shown: the cached one or the one of the release at first, the
    /// fetched one when it comes.
    index: Index,
    /// What the index is, when that is worth saying: it could not be fetched, or it is being.
    note: Option<String>,
    job: Option<Job>,
    source: Source,
    /// The plugins of the official index.
    official: Index,
    /// The extra sources fetched so far, in the order of `plugin_sources`.
    extra: Vec<(String, Index)>,
    /// The `plugin_sources` the last fetch asked, for ordering what comes back.
    asked: Vec<String>,
    /// What went wrong with each source, by label.
    problems: Vec<String>,
    /// The plugin names a source lists that another index already has.
    left_out: Vec<String>,
    /// The READMEs fetched, by address.
    readmes: HashMap<String, String>,
    /// The updates of "update all" that are still to come, in order.
    queue: VecDeque<IndexEntry>,
}

impl Store {
    /// Makes `index` what is shown: the official plugins, then those of the sources.
    fn rebuild(&mut self) {
        let asked = self.asked.clone();
        self.extra.sort_by_key(|(url, _)| {
            asked
                .iter()
                .position(|asked| asked == url)
                .unwrap_or(usize::MAX)
        });
        let (index, left_out) = Index::merged(&self.official, &self.extra);
        self.index = index;
        self.left_out = left_out;
    }

    /// What the list says about the index: what the official one is, and what is
    /// wrong with the sources.
    fn shown_note(&self) -> Option<Note> {
        let mut parts: Vec<String> = self.note.iter().cloned().collect();
        parts.extend(self.problems.iter().cloned());
        if !self.left_out.is_empty() {
            parts.push(format!(
                "left out, the name is taken: {}",
                self.left_out.join(", ")
            ));
        }
        // Something is wrong when a source failed, a name was taken or the index is offline.
        let warn = !self.problems.is_empty()
            || !self.left_out.is_empty()
            || self
                .note
                .as_deref()
                .is_some_and(|note| note.starts_with("offline"));
        (!parts.is_empty()).then(|| Note {
            text: parts.join(" | "),
            warn,
        })
    }
}

#[cfg(test)]
impl Store {
    pub(super) fn with_source(source: Source) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }
}

/// What the user most likely wants to do with a plugin of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Next {
    Install,
    Update,
    TurnOn,
    Place,
    Open,
    Say(String),
}

impl App {
    /// Fetches the index and the plugins from this server, whatever it is (tests).
    #[cfg(test)]
    pub(crate) fn with_plugin_source(mut self, index_url: String) -> Self {
        self.plugins.store =
            Store::with_source(Source::local(index_url, Policy::local(&["127.0.0.1"])));
        self
    }

    /// Opens the manager with the index known so far and asks for a fresh one.
    pub(super) fn show_plugins(&mut self) {
        if self.plugins_dir().is_none() {
            self.state.notify("there is no folder for plugins here");
            return;
        }
        self.plugins.store.official = Index::cached(self.config_dir.as_deref());
        self.plugins.store.rebuild();
        self.show_plugin_list(None);
        self.fetch_index();
    }

    /// Opens the list with the plugin `chosen` chosen.
    pub(super) fn show_plugin_list(&mut self, chosen: Option<&str>) {
        let Some(dir) = self.plugins_dir() else {
            return;
        };
        self.plugins.manager.return_to = None;
        self.plugins.manager.found = manifest::scan(&dir, Host::THIS);
        self.stop_plugins_that_are_gone();
        let mut list = PluginList::new(self.plugin_rows(), self.plugins.store.shown_note());
        if let Some(id) = chosen {
            list.select(id);
        }
        if let Some(job) = &self.plugins.store.job {
            list.set_busy(Some(format!("installing {}", job.id)));
        }
        self.state.open_plugin_list(list);
        self.needs_redraw = true;
    }

    /// A question cannot open over the list: the list closes, and comes back with
    /// the plugin `id` chosen when the questions are answered (`back_to_plugin_list`).
    fn leave_list_for_question(&mut self, id: &str) {
        self.state.close_plugin_list();
        self.plugins.manager.return_to = Some(id.into());
    }

    /// Opens the list again when what it closed for is over: no question or menu
    /// is open and none waits.
    pub(super) fn back_to_plugin_list(&mut self) {
        if self.plugins.manager.return_to.is_none()
            || !matches!(self.state.popup(), super::state::Popup::None)
            || !self.plugins.manager.waiting.is_empty()
        {
            return;
        }
        if let Some(id) = self.plugins.manager.return_to.take() {
            self.show_plugin_list(Some(&id));
        }
    }

    /// A plugin that runs but whose folder was deleted by hand (or whose manifest no
    /// longer reads) is stopped, and its tab goes with it.
    fn stop_plugins_that_are_gone(&mut self) {
        let gone: Vec<Box<str>> = self
            .plugins
            .manager
            .running_ids()
            .filter(|id| self.plugins.manager.installed(id).is_none())
            .collect();
        for id in gone {
            self.stop_plugin(&id);
            self.state.notify(format!(
                "plugin {id} is gone from the plugins folder: stopped"
            ));
        }
    }

    /// Makes the open list say what is true now.
    fn refresh_plugin_list(&mut self) {
        let rows = self.plugin_rows();
        let note = self.plugins.store.shown_note();
        if let Some(list) = self.state.plugin_list_mut() {
            list.set_rows(rows);
            list.set_note(note);
        }
        self.needs_redraw = true;
    }

    fn fetch_index(&mut self) {
        let (Some(url), Some(events)) = (
            self.plugins.store.source.index_url.clone(),
            self.background.sender(),
        ) else {
            return;
        };
        let policy = self.plugins.store.source.policy;
        let config = self.config_dir.clone();
        self.plugins.store.note = Some("asking for the latest list of plugins".to_owned());
        self.plugins.store.problems.clear();
        // The official index asked for as a source too (an old settings file) is one fetch.
        let sources: Vec<String> = self
            .settings
            .plugin_sources
            .iter()
            .filter(|source| **source != url)
            .cloned()
            .collect();
        self.plugins.store.asked = sources.clone();
        for source in sources {
            let events = events.clone();
            std::thread::spawn(move || {
                let cancel = AtomicBool::new(false);
                // An extra index is not cached: offline, its plugins are not listed.
                let result = install::fetch_index(&source, policy, None, &cancel)
                    .map(|index| index.listed_by(&source))
                    .map_err(|error| error.to_string());
                let _ = events
                    .blocking_send(Event::PluginJob(JobEvent::SourceIndex { source, result }));
            });
        }
        std::thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let result = install::fetch_index(&url, policy, config.as_deref(), &cancel)
                .map_err(|error| error.to_string());
            let _ = events.blocking_send(Event::PluginJob(JobEvent::Index(result)));
        });
    }

    /// The rows of the list: every plugin of the index, then the ones that are only in
    /// the plugins folder.
    fn plugin_rows(&self) -> Vec<Row> {
        let index = &self.plugins.store.index;
        // One row for each plugin, however many versions the index lists.
        let mut rows: Vec<Row> = index
            .ids()
            .into_iter()
            .filter_map(|id| index.entry(id))
            .map(|entry| self.row_of_entry(entry))
            .collect();
        for found in &self.plugins.manager.found {
            match found {
                Ok(installed) if index.entry(installed.id()).is_none() => {
                    rows.push(self.row_of_installed(installed, None));
                }
                Err((folder, error)) => rows.push(Row {
                    id: folder.as_str().into(),
                    name: folder.clone(),
                    version: String::new(),
                    state: "cannot run".to_owned(),
                    source: None,
                    detail: error.to_string(),
                    mark: Mark::Broken,
                    grey: true,
                }),
                Ok(_) => {}
            }
        }
        rows
    }

    fn row_of_entry(&self, entry: &IndexEntry) -> Row {
        match self.plugins.manager.installed(&entry.id) {
            Some(installed) => self.row_of_installed(installed, Some(entry)),
            None => {
                if let Some(error) = self.plugins.manager.broken(&entry.id) {
                    return Row {
                        id: entry.id.as_str().into(),
                        name: entry.name.clone(),
                        version: entry.version.clone(),
                        state: "cannot run".to_owned(),
                        source: entry.source_label(),
                        detail: error,
                        mark: Mark::Broken,
                        grey: true,
                    };
                }
                let missing = entry.availability(Host::THIS).missing();
                let state = missing
                    .clone()
                    .unwrap_or_else(|| "not installed".to_owned());
                Row {
                    id: entry.id.as_str().into(),
                    name: entry.name.clone(),
                    version: entry.version.clone(),
                    state,
                    source: entry.source_label(),
                    detail: described(entry),
                    mark: match missing.is_some() {
                        true => Mark::Unavailable,
                        false => Mark::NotInstalled,
                    },
                    grey: missing.is_some(),
                }
            }
        }
    }

    fn row_of_installed(&self, installed: &Installed, entry: Option<&IndexEntry>) -> Row {
        let id = installed.id();
        let managed = install::marker(&installed.dir).is_some();
        let update = update_for(installed, entry);
        let mut parts = Vec::new();
        if !managed {
            parts.push("local".to_owned());
        }
        if let Some(entry) = update {
            parts.push(format!("update {} available", entry.version));
        }
        parts.push(self.state_of(id));
        let detail = match (managed, update) {
            (false, _) => "a plugin of your own: the manager leaves it alone".to_owned(),
            (true, Some(entry)) => format!(
                "{} is installed, {} is in the index",
                installed.manifest.version, entry.version
            ),
            (true, None) => entry.map(described).unwrap_or_default(),
        };
        let mark = match (update.is_some(), self.plugins.manager.is_running(id)) {
            (true, _) => Mark::Update,
            (false, true) => Mark::On,
            (false, false) if self.plugins.manager.stopped.contains_key(id) => Mark::Broken,
            (false, false) => Mark::Off,
        };
        let detail = match self.open_key_of(id) {
            Some(keys) => format!("{detail} · opens with {keys}"),
            None => detail,
        };
        Row {
            id: id.into(),
            name: installed.manifest.display_name().to_owned(),
            version: installed.manifest.version.clone(),
            state: parts.join(" · "),
            source: install::marker(&installed.dir).and_then(|marker| marker.entry.source_label()),
            detail,
            mark,
            grey: false,
        }
    }

    /// Whether a plugin of the plugins folder is on, and where it is.
    fn state_of(&self, id: &str) -> String {
        let manager = &self.plugins.manager;
        if manager.is_running(id) {
            return match self.saved_plugin_record(id).and_then(|record| record.place) {
                _ if self.layout_places(id) => "on".to_owned(),
                Some(Place::Side) if self.state.screen().width < RIGHT_BAR_MIN_WIDTH => {
                    format!("on · side pane (hidden: needs {RIGHT_BAR_MIN_WIDTH} columns)")
                }
                Some(place) => format!("on · {}", place.label()),
                None => "on · not placed".to_owned(),
            };
        }
        match manager.stopped.get(id) {
            Some(_) => "off: it failed too often".to_owned(),
            None => "off".to_owned(),
        }
    }

    /// What Enter does for the plugin `id`.
    fn next_for(&self, id: &str) -> Next {
        let manager = &self.plugins.manager;
        let entry = self.plugins.store.index.entry(id);
        let Some(installed) = manager.installed(id) else {
            if let Some(error) = manager.broken(id) {
                return Next::Say(format!("plugin {id} cannot run: {error}"));
            }
            return match entry.map(|entry| entry.availability(Host::THIS)) {
                Some(Availability::Ready) => Next::Install,
                Some(missing) => Next::Say(format!(
                    "plugin {id} {}",
                    missing.missing().unwrap_or_default()
                )),
                None => Next::Say(format!("no plugin named {id}")),
            };
        };
        let managed = install::marker(&installed.dir).is_some();
        let update = entry.is_some_and(|entry| {
            managed
                && is_newer(&entry.version, &installed.manifest.version)
                && entry.source == installed_source(installed)
                && entry.availability(Host::THIS) == Availability::Ready
        });
        match (manager.is_running(id), update) {
            (_, true) => Next::Update,
            (false, false) => Next::TurnOn,
            (true, false)
                if self.saved_plugin_record(id).and_then(|r| r.place).is_none()
                    && !self.layout_places(id) =>
            {
                Next::Place
            }
            (true, false) => Next::Open,
        }
    }

    pub(super) fn handle_plugin_list_key(&mut self, key: KeyEvent) {
        let Some(command) = Command::from_key(key) else {
            return;
        };
        self.needs_redraw = true;
        let busy = self
            .state
            .plugin_list_mut()
            .is_some_and(|list| list.is_busy());
        if busy {
            if command == Command::Cancel
                && let Some(job) = &self.plugins.store.job
            {
                job.cancel.store(true, Ordering::Relaxed);
                self.plugins.store.queue.clear();
                if let Some(list) = self.state.plugin_list_mut() {
                    list.set_busy(Some("stopping".to_owned()));
                }
            }
            return;
        }
        let Some(chosen) = self
            .state
            .plugin_list_mut()
            .and_then(|list| list.selected_id().map(str::to_owned))
        else {
            if command == Command::Cancel {
                self.state.close_plugin_list();
            }
            return;
        };
        match command {
            Command::Previous | Command::Next => {
                if let Some(list) = self.state.plugin_list_mut() {
                    list.step(command == Command::Next);
                }
            }
            Command::Cancel => self.state.close_plugin_list(),
            Command::Refresh => {
                self.fetch_index();
                self.refresh_plugin_list();
            }
            Command::Primary => match self.next_for(&chosen) {
                Next::Install => self.install_plugin(&chosen),
                Next::Update => self.ask_to_update(&chosen),
                Next::TurnOn => self.toggle_in_list(&chosen),
                Next::Place => self.ask_where(&chosen),
                Next::Open => {
                    self.state.close_plugin_list();
                    self.open_plugin_popup(&chosen);
                }
                Next::Say(message) => self.state.notify(message),
            },
            Command::Install => match self.next_for(&chosen) {
                Next::Install => self.install_plugin(&chosen),
                Next::Update => self.ask_to_update(&chosen),
                Next::Say(message) => self.state.notify(message),
                _ => self.say_about_install(&chosen),
            },
            Command::UpdateAll => self.ask_to_update_all(&chosen),
            Command::Describe => self.describe_plugin(&chosen),
            Command::BindKey => self.ask_for_key(&chosen),
            Command::Remove => self.ask_to_remove(&chosen),
            Command::Toggle => self.toggle_in_list(&chosen),
            Command::Place => match self.plugins.manager.is_running(&chosen) {
                true => self.ask_where(&chosen),
                false => self.state.notify(format!(
                    "plugin {chosen} is not on: turn it on first, then place it"
                )),
            },
        }
    }

    fn say_about_install(&mut self, id: &str) {
        let managed = self
            .plugins
            .manager
            .installed(id)
            .is_some_and(|installed| install::marker(&installed.dir).is_some());
        match managed {
            true => self.state.notify(format!("plugin {id} is up to date")),
            false => self.state.notify(format!(
                "plugin {id} is one of your own: remove its folder by hand to install the one of the index"
            )),
        }
    }

    fn toggle_in_list(&mut self, id: &str) {
        if self.plugins.manager.installed(id).is_none() && self.plugins.manager.broken(id).is_none()
        {
            self.state
                .notify(format!("plugin {id} is not installed: press i"));
            return;
        }
        // Turning on may ask what the plugin may do, and a question cannot open over
        // the list: the list closes first. Turning off keeps it and shows what changed.
        if !self.plugins.manager.is_running(id) {
            self.leave_list_for_question(id);
        }
        self.toggle_plugin(id);
        if self.state.plugin_list_mut().is_some() {
            self.refresh_plugin_list();
        }
    }

    // --- install and update ---

    /// Installs the plugin `id` of the list; one of an extra source is asked about first.
    fn install_plugin(&mut self, id: &str) {
        let Some(entry) = self.plugins.store.index.entry(id).cloned() else {
            return;
        };
        match (&entry.source, entry.source_label()) {
            (Some(url), Some(label)) => {
                let confirm = Confirm::install_from_source(
                    id,
                    &entry.name,
                    &entry.version,
                    &label,
                    url,
                    &entry.sha256,
                );
                self.leave_list_for_question(id);
                self.state.open_confirm(confirm);
            }
            _ => self.start_install(entry, false),
        }
    }

    /// The user said yes to installing from the source at `source`. It is the same
    /// plugin from the same source or nothing: the index may have changed meanwhile.
    pub(super) fn install_plugin_from(&mut self, id: &str, source: &str) {
        match self.plugins.store.index.entry(id).cloned() {
            Some(entry) if entry.source.as_deref() == Some(source) => {
                self.start_install(entry, false)
            }
            _ => {
                self.state.notify(format!(
                    "plugin {id} is no longer listed by that source: nothing was installed"
                ));
                self.show_plugin_list(Some(id));
            }
        }
    }

    fn ask_to_update(&mut self, id: &str) {
        let (Some(entry), Some(installed)) = (
            self.plugins.store.index.entry(id),
            self.plugins.manager.installed(id),
        ) else {
            return;
        };
        let confirm = Confirm::update_plugin(
            id,
            &entry.name,
            &installed.manifest.version,
            &entry.version,
            entry.source_label().as_deref(),
            self.plugins.manager.is_running(id),
        );
        self.state.open_confirm(confirm);
    }

    /// The user said yes to an update.
    pub(super) fn update_plugin(&mut self, id: &str) {
        match self.plugins.store.index.entry(id).cloned() {
            Some(entry) => self.start_install(entry, true),
            None => self.show_plugin_list(Some(id)),
        }
    }

    fn start_install(&mut self, entry: IndexEntry, update: bool) {
        let Some(events) = self.background.sender() else {
            self.state
                .notify_error("nothing can be downloaded here".to_owned());
            return;
        };
        let Some(dir) = self.plugins_dir() else {
            return;
        };
        if self.plugins.store.job.is_some() {
            self.state.notify("a plugin is being installed already");
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.plugins.store.job = Some(Job {
            id: entry.id.as_str().into(),
            cancel: Arc::clone(&cancel),
        });
        let policy = self.plugins.store.source.policy;
        let chosen = entry.id.clone();
        let id: Box<str> = entry.id.as_str().into();
        std::thread::spawn(move || {
            let progress = |stage| {
                let _ = events.blocking_send(Event::PluginJob(JobEvent::Stage {
                    id: id.clone(),
                    stage,
                }));
            };
            let result = install::install(&entry, &dir, Host::THIS, policy, &cancel, progress)
                .map(|_| ())
                .map_err(|error| match error {
                    InstallError::Cancelled => "stopped".to_owned(),
                    other => other.to_string(),
                });
            let _ = events.blocking_send(Event::PluginJob(JobEvent::Done { id, update, result }));
        });
        self.show_plugin_list(Some(&chosen));
    }

    /// Something happened on a thread of the manager.
    pub(super) fn plugin_job_event(&mut self, event: JobEvent) {
        self.needs_redraw = true;
        match event {
            JobEvent::Index(Ok(index)) => {
                self.plugins.store.official = index;
                self.plugins.store.rebuild();
                self.plugins.store.note = None;
                self.refresh_plugin_list();
            }
            JobEvent::Index(Err(reason)) => {
                self.plugins.store.note =
                    Some(format!("offline, showing the list known so far: {reason}"));
                self.refresh_plugin_list();
            }
            JobEvent::Readme { url, result } => {
                let body = match result {
                    Ok(text) => {
                        self.plugins.store.readmes.insert(url.clone(), text.clone());
                        Body::Text(text)
                    }
                    Err(reason) => {
                        Body::Missing(format!("The description could not be fetched: {reason}"))
                    }
                };
                if let Some(readme) = self.state.plugin_readme_mut()
                    && readme.url() == Some(url.as_str())
                {
                    readme.set_body(body);
                }
            }
            JobEvent::SourceIndex { source, result } => {
                match result {
                    Ok(index) => {
                        self.plugins.store.extra.retain(|(url, _)| *url != source);
                        self.plugins.store.extra.push((source, index));
                        self.plugins.store.rebuild();
                    }
                    Err(reason) => {
                        let label = crate::plugins::index::source_label(&source);
                        self.plugins
                            .store
                            .problems
                            .push(format!("source {label}: {reason}"));
                    }
                }
                self.refresh_plugin_list();
            }
            JobEvent::Stage { id, stage } => {
                let what = match stage {
                    Stage::Manifest => "downloading the manifest",
                    Stage::Module => "downloading the module",
                    Stage::Checking => "checking the module",
                    Stage::Installing => "installing",
                };
                if let Some(list) = self.state.plugin_list_mut() {
                    list.set_busy(Some(format!("{id}: {what}")));
                }
            }
            JobEvent::Done { id, update, result } => {
                self.plugins.store.job = None;
                if let Some(list) = self.state.plugin_list_mut() {
                    list.set_busy(None);
                }
                match result {
                    Ok(()) => self.installed_plugin(&id, update),
                    Err(reason) if reason == "stopped" => {
                        self.state
                            .notify(format!("plugin {id}: stopped, nothing was installed"));
                        self.show_plugin_list(Some(&id));
                    }
                    Err(reason) => {
                        self.state
                            .notify_error(format!("plugin {id} was not installed: {reason}"));
                        self.show_plugin_list(Some(&id));
                    }
                }
                self.continue_updates();
            }
        }
    }

    /// The module is in the plugins folder: a new plugin is asked about (what it may
    /// do, then where it goes); an updated one that ran runs again.
    fn installed_plugin(&mut self, id: &str, update: bool) {
        let Some(dir) = self.plugins_dir() else {
            return;
        };
        let was_running = self.plugins.manager.is_running(id);
        if was_running {
            self.stop_plugin(id);
        }
        self.plugins.manager.found = manifest::scan(&dir, Host::THIS);
        if update && was_running {
            let Some(installed) = self.plugins.manager.installed(id).cloned() else {
                self.show_plugin_list(Some(id));
                return;
            };
            self.state.notify(format!(
                "plugin {} is now {}",
                installed.manifest.display_name(),
                installed.manifest.version
            ));
            match self.saved_plugin_record(id) {
                Some(record) if super::plugin_manager::covers(&record, &installed) => {
                    self.start_plugin(&installed);
                    self.show_plugin_list(Some(id));
                }
                // The new version wants more than the user allowed: ask again.
                _ => {
                    self.leave_list_for_question(id);
                    self.plugins.manager.waiting.push_front(id.into());
                    self.ask_about_next_plugin();
                }
            }
            return;
        }
        self.state.notify(format!("plugin {id} installed"));
        // A plugin the user allowed and placed before, then removed, comes back as it was.
        match self.saved_plugin_record(id) {
            Some(record)
                if record.enabled
                    && self.plugins.manager.installed(id).is_some_and(|installed| {
                        super::plugin_manager::covers(&record, installed)
                    }) =>
            {
                if let Some(installed) = self.plugins.manager.installed(id).cloned() {
                    self.start_plugin(&installed);
                }
                self.show_plugin_list(Some(id));
            }
            _ => {
                self.leave_list_for_question(id);
                self.plugins.manager.waiting.push_front(id.into());
                self.ask_about_next_plugin();
            }
        }
    }

    // --- update all ---

    /// The plugins that have a newer version to install: what is installed, then what comes.
    fn update_candidates(&self) -> Vec<(Installed, IndexEntry)> {
        let index = &self.plugins.store.index;
        self.plugins
            .manager
            .found
            .iter()
            .filter_map(|found| found.as_ref().ok())
            .filter_map(|installed| {
                let entry = update_for(installed, index.entry(installed.id()))?;
                Some((installed.clone(), entry.clone()))
            })
            .collect()
    }

    fn ask_to_update_all(&mut self, chosen: &str) {
        let updates: Vec<(String, String, String)> = self
            .update_candidates()
            .into_iter()
            .map(|(installed, entry)| {
                (
                    installed.manifest.display_name().to_owned(),
                    installed.manifest.version.clone(),
                    entry.version,
                )
            })
            .collect();
        if updates.is_empty() {
            self.state.notify("every plugin is up to date");
            return;
        }
        self.leave_list_for_question(chosen);
        self.state.open_confirm(Confirm::update_all(&updates));
    }

    /// The user said yes to updating everything: one after the other.
    pub(super) fn update_all_plugins(&mut self) {
        let entries: Vec<IndexEntry> = self
            .update_candidates()
            .into_iter()
            .map(|(_, entry)| entry)
            .collect();
        self.plugins.store.queue = entries.into();
        self.continue_updates();
    }

    /// Starts the next update of "update all" when nothing is being installed and no
    /// question is open: the answers come first, the downloads go on after them.
    pub(super) fn continue_updates(&mut self) {
        if self.plugins.store.job.is_some()
            || self.plugins.store.queue.is_empty()
            || !self.plugins.manager.waiting.is_empty()
            || !matches!(
                self.state.popup(),
                super::state::Popup::None | super::state::Popup::PluginList(_)
            )
        {
            return;
        }
        if let Some(entry) = self.plugins.store.queue.pop_front() {
            self.start_install(entry, true);
        }
    }

    // --- about ---

    /// Opens what the plugin says about itself: what the manager knows, and its README.
    fn describe_plugin(&mut self, id: &str) {
        let entry = self.plugins.store.index.entry(id).cloned();
        let installed = self.plugins.manager.installed(id).cloned();
        let name = match (&installed, &entry) {
            (Some(installed), _) => installed.manifest.display_name().to_owned(),
            (None, Some(entry)) => entry.name.clone(),
            (None, None) => id.to_owned(),
        };
        let version = match (&installed, &entry) {
            (Some(installed), _) => installed.manifest.version.clone(),
            (None, Some(entry)) => entry.version.clone(),
            (None, None) => String::new(),
        };
        let mut head = Vec::new();
        if let Some(entry) = &entry
            && !entry.description.is_empty()
        {
            head.push(entry.description.clone());
        }
        match entry.as_ref().and_then(IndexEntry::source_label) {
            Some(label) => head.push(format!("From {label}: not an official plugin.")),
            None if entry.is_some() => head.push("An official plugin.".to_owned()),
            None => {}
        }
        if let (Some(installed), Some(entry)) = (&installed, &entry)
            && installed.manifest.version != entry.version
        {
            head.push(format!(
                "{} is installed, {} is in the list.",
                installed.manifest.version, entry.version
            ));
        }
        match &installed {
            Some(installed) if install::marker(&installed.dir).is_none() => {
                head.push("A plugin of your own: the manager leaves it alone.".to_owned());
            }
            _ => {}
        }
        if let Some(installed) = &installed {
            let kinds = super::plugin_manager::shown_file_types(installed);
            let run = super::plugin_manager::shown_run(installed);
            let mut asks: Vec<String> = installed
                .capabilities
                .iter()
                .map(|capability| super::plugin_manager::describe(*capability, &kinds, &run))
                .collect();
            asks.extend(super::plugin_manager::extra_asks(installed));
            match asks.is_empty() {
                true => head.push(
                    "It asks to be allowed to draw in its own pane and nothing else.".to_owned(),
                ),
                false => {
                    head.push("It asks to be allowed to:".to_owned());
                    head.extend(asks.into_iter().map(|ask| format!("  - {ask}")));
                }
            }
        }
        // The description of the plugin itself, as it was installed; else the one its
        // publisher keeps next to the manifest, else the README the list links.
        let local = installed
            .as_ref()
            .and_then(|installed| fs::read(installed.dir.join(install::DESCRIPTION_FILE)).ok())
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        let candidates: Vec<String> = entry
            .iter()
            .flat_map(|entry| {
                [
                    Some(install::sibling(&entry.manifest, install::DESCRIPTION_FILE)),
                    entry.readme.clone(),
                ]
            })
            .flatten()
            .collect();
        // The first address names the page (to fill it in when the text arrives).
        let url = candidates.first().cloned();
        let body = match (local, &url) {
            (Some(text), _) => Body::Text(text),
            (None, None) => Body::Missing("This plugin has no description.md.".to_owned()),
            (None, Some(url)) => match self.plugins.store.readmes.get(url) {
                Some(text) => Body::Text(text.clone()),
                None => {
                    self.fetch_description(url.clone(), candidates);
                    Body::Loading
                }
            },
        };
        let title = match version.is_empty() {
            true => name,
            false => format!("{name} {version}"),
        };
        self.state
            .open_plugin_readme(Readme::new(id, url, title, head, body));
        self.needs_redraw = true;
    }

    fn fetch_description(&mut self, key: String, candidates: Vec<String>) {
        let Some(events) = self.background.sender() else {
            return;
        };
        let policy = self.plugins.store.source.policy;
        std::thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let mut result = Err("nothing to fetch".to_owned());
            for url in &candidates {
                result = match install::fetch_readme(url, policy, &cancel) {
                    Ok(text) => Ok(text),
                    // No description.md and no README listed: say that, not "404".
                    Err(InstallError::Status { status: 404, .. }) if candidates.len() == 1 => {
                        Err("there is no description.md, and no README is listed".to_owned())
                    }
                    Err(error) => Err(error.to_string()),
                };
                if result.is_ok() {
                    break;
                }
            }
            let _ = events.blocking_send(Event::PluginJob(JobEvent::Readme { url: key, result }));
        });
    }

    pub(super) fn handle_plugin_readme_key(&mut self, key: KeyEvent) {
        let Some(command) = plugin_readme::Command::from_key(key) else {
            return;
        };
        self.needs_redraw = true;
        if command == plugin_readme::Command::Close {
            let id = self
                .state
                .plugin_readme_mut()
                .map(|readme| readme.id().to_owned());
            self.show_plugin_list(id.as_deref());
            return;
        }
        if let Some(readme) = self.state.plugin_readme_mut() {
            readme.apply(command);
        }
    }

    // --- remove ---

    fn ask_to_remove(&mut self, id: &str) {
        let Some(installed) = self.plugins.manager.installed(id) else {
            self.state.notify(match self.plugins.manager.broken(id) {
                Some(_) => format!(
                    "plugin {id} was not installed by the manager: remove its folder by hand"
                ),
                None => format!("plugin {id} is not installed"),
            });
            return;
        };
        if install::marker(&installed.dir).is_none() {
            self.state.notify(format!(
                "plugin {id} is one of your own: the manager does not remove it, delete its folder by hand"
            ));
            return;
        }
        let confirm = Confirm::remove_plugin(id, installed.manifest.display_name());
        self.state.open_confirm(confirm);
    }

    /// The user said yes to removing a plugin: it stops and its folder goes; what
    /// was decided about it stays.
    pub(super) fn remove_plugin(&mut self, id: &str) {
        let Some(dir) = self.plugins_dir() else {
            return;
        };
        self.stop_plugin(id);
        match install::remove(&dir, id) {
            Ok(()) => self.state.notify(format!("plugin {id} removed")),
            Err(error) => self
                .state
                .notify_error(format!("plugin {id} was not removed: {error}")),
        }
        self.plugins.manager.found = manifest::scan(&dir, Host::THIS);
        self.show_plugin_list(Some(id));
    }

    // --- placing ---

    /// Asks where the plugin's view goes: a menu of the places this layout has.
    pub(super) fn ask_where(&mut self, id: &str) {
        if self.layout_places(id) {
            self.state
                .notify(format!("plugin {id} is placed by your layout file"));
            return;
        }
        let entries: Vec<Entry> = self
            .available_places()
            .into_iter()
            .map(|place| Entry::Item {
                label: format!("Put {id} in the {}", place.label()).into(),
                action: Action::PluginPlaced(format!("{id}:{}", place.key()).into()),
                keys: None,
            })
            .collect();
        let screen = self.state.screen();
        let menu = Menu::on_app(
            entries,
            Position::new(screen.x + screen.width / 3, screen.y + screen.height / 3),
        )
        .starting_on_anchor_row();
        self.state.open_menu(menu);
        self.needs_redraw = true;
    }

    /// The user chose a place from the menu.
    pub(super) fn plugin_placed(&mut self, chosen: &str) {
        let Some((id, place)) = chosen
            .rsplit_once(':')
            .and_then(|(id, place)| Some((id, Place::from_key(place)?)))
        else {
            return;
        };
        self.place_plugin(id, place);
    }
}
