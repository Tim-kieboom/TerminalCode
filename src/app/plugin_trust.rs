//! Whether plugins may run commands in the project folder: the user's answer is kept
//! for each folder, the runtime is told it, and a plugin that asks to run a command in a
//! folder that is not trusted makes the editor ask once.

use std::path::{Path, PathBuf};

use super::App;
use crate::components::popups::confirm::Confirm;

/// Who asked to run commands in a folder that is not trusted, and whether the user was
/// asked about it since the editor started.
#[derive(Debug, Default)]
pub(super) struct Ask {
    plugins: Vec<Box<str>>,
    asked: bool,
}

impl App {
    /// Whether the user trusts the project folder.
    pub(super) fn folder_is_trusted(&self) -> bool {
        let key = trust_key(&self.project_root());
        self.is_folder_trusted(&key)
    }

    /// Tells the plugins that run commands which folder is the project's and whether the
    /// user trusts it.
    pub(super) fn publish_workspace_to_plugins(&self) {
        let Some(runtime) = &self.plugins.runtime else {
            return;
        };
        runtime.set_workspace(self.project_root(), self.folder_is_trusted());
    }

    /// A plugin asked to run a command in a folder that is not trusted: the user is
    /// asked once (a no is left alone; "Plugins: Trust This Folder" asks again).
    pub(super) fn plugin_needs_trust(&mut self, plugin: &str) {
        if !self
            .plugins
            .trust
            .plugins
            .iter()
            .any(|name| &**name == plugin)
        {
            self.plugins.trust.plugins.push(plugin.into());
        }
        self.ask_about_trust();
    }

    /// Asks about the folder when a plugin wants it, nothing else is being asked, and
    /// the user was not asked yet.
    pub(super) fn ask_about_trust(&mut self) {
        let waiting = &self.plugins.trust;
        if waiting.asked || waiting.plugins.is_empty() {
            return;
        }
        if !matches!(self.state.popup(), super::state::Popup::None) {
            return;
        }
        self.plugins.trust.asked = true;
        let plugins = self.plugins.trust.plugins.clone();
        let confirm = Confirm::trust_folder(&self.project_root(), &plugins);
        self.state.open_confirm(confirm);
        self.needs_redraw = true;
    }

    /// The action: asks whether to trust the folder.
    pub(super) fn ask_to_trust_folder(&mut self) {
        if self.folder_is_trusted() {
            self.state.notify("this folder is trusted already");
            return;
        }
        if !matches!(self.state.popup(), super::state::Popup::None) {
            return;
        }
        let confirm = Confirm::trust_folder(&self.project_root(), &[]);
        self.state.open_confirm(confirm);
        self.needs_redraw = true;
    }

    /// The user said yes: the folder is remembered as trusted and the plugins are told.
    pub(super) fn trust_folder(&mut self) {
        let key = trust_key(&self.project_root());
        self.remember_trusted(&key);
        self.publish_workspace_to_plugins();
        self.state
            .notify("this folder is trusted: plugins and Run may run commands in it");
    }
}

/// What a folder is remembered by: its whole path.
pub(super) fn trust_key(folder: &Path) -> String {
    let whole: PathBuf = folder
        .canonicalize()
        .unwrap_or_else(|_| folder.to_path_buf());
    whole.to_string_lossy().into_owned()
}
