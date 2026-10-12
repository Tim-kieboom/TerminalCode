//! The language servers: starting the one the file in front needs, and saying
//! how it is doing. The servers themselves are run by [`Manager`].

use std::path::PathBuf;
use std::sync::Arc;

use crate::event::Event;
#[cfg(not(test))]
use crate::lsp::ProcessLauncher;
use crate::lsp::{LaunchSpec, Launcher, Manager, Notice, ServerKey};

use super::App;

/// What the app keeps of the language servers.
pub(super) struct Lsp {
    /// Made when the first file that wants a server is shown.
    manager: Option<Manager>,
    /// What starts a server: the real one runs programs. The tests have none
    /// until they bring one, so that no test starts a real server.
    launcher: Option<Arc<dyn Launcher>>,
    /// The server of the file in front, whose state the status bar shows.
    shown: Option<ServerKey>,
}

impl Default for Lsp {
    fn default() -> Self {
        Self {
            manager: None,
            launcher: Self::default_launcher(),
            shown: None,
        }
    }
}

impl Lsp {
    #[cfg(not(test))]
    fn default_launcher() -> Option<Arc<dyn Launcher>> {
        Some(Arc::new(ProcessLauncher))
    }

    #[cfg(test)]
    fn default_launcher() -> Option<Arc<dyn Launcher>> {
        None
    }
}

impl std::fmt::Debug for Lsp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lsp")
            .field("shown", &self.shown)
            .finish_non_exhaustive()
    }
}

impl App {
    /// Starts servers with `launcher` instead of running programs.
    #[cfg(test)]
    pub(crate) fn with_lsp_launcher(mut self, launcher: Arc<dyn Launcher>) -> Self {
        self.lsp.launcher = Some(launcher);
        self
    }

    /// The file in front, looked at for a language server: the one it needs is
    /// started (once, however many files want it), and how it is doing shows in
    /// the status bar.
    pub(super) fn lsp_file_shown(&mut self) {
        let wanted = self
            .lsp
            .launcher
            .is_some()
            .then(|| self.server_for_file_in_front())
            .flatten();
        let Some((key, spec)) = wanted else {
            self.lsp.shown = None;
            self.refresh_lsp_status();
            return;
        };
        self.lsp.shown = Some(key.clone());
        let notices = match self.lsp_manager() {
            Some(manager) => manager.ensure(&key, spec),
            None => Vec::new(),
        };
        self.show_lsp_notices(notices);
        self.refresh_lsp_status();
    }

    /// What the servers said that is for the app: servers that finished
    /// starting, news about them, and servers that died.
    pub(super) fn lsp_changed(&mut self) {
        let Some(manager) = self.lsp.manager.as_mut() else {
            return;
        };
        let notices = manager.poll();
        self.show_lsp_notices(notices);
        self.refresh_lsp_status();
        self.needs_redraw = true;
    }

    /// The `language_servers` setting changed: every server is ended and the
    /// file in front gets the one it needs from the new settings.
    pub(super) fn lsp_settings_changed(&mut self) {
        // At startup the loop is not running yet and `run` starts what is
        // needed; nothing could be told of the result before then.
        if !self.background.is_connected() {
            return;
        }
        if let Some(manager) = self.lsp.manager.as_mut() {
            manager.reset();
        }
        self.lsp.shown = None;
        self.lsp_file_shown();
    }

    /// The server the file in front is served by, and how to start it.
    fn server_for_file_in_front(&self) -> Option<(ServerKey, LaunchSpec)> {
        if !self.state.workspace().has_tabs() {
            return None;
        }
        let path: PathBuf = self.state.editor().buffer().path()?.to_path_buf();
        let (key, config) = self
            .settings
            .language_servers
            .resolve(&path, &self.project_root())?;
        let spec = LaunchSpec {
            name: key.name.clone(),
            command: config.command.clone(),
            args: config.args.clone(),
            root: key.root.clone(),
            initialization_options: config.initialization_options.clone(),
        };
        Some((key, spec))
    }

    fn lsp_manager(&mut self) -> Option<&mut Manager> {
        let launcher = Arc::clone(self.lsp.launcher.as_ref()?);
        let events = self.background.sender();
        Some(self.lsp.manager.get_or_insert_with(|| {
            // Delivery matters more than speed here, as for the terminal.
            Manager::new(launcher, move || {
                if let Some(events) = &events {
                    let _ = events.blocking_send(Event::LspChanged);
                }
            })
        }))
    }

    fn show_lsp_notices(&mut self, notices: Vec<Notice>) {
        for notice in notices {
            match notice.error {
                true => self.state.notify_error(notice.text),
                false => self.state.notify(notice.text),
            }
            self.needs_redraw = true;
        }
    }

    /// Puts how the server of the file in front is doing in the status bar.
    fn refresh_lsp_status(&mut self) {
        let status = self
            .lsp
            .shown
            .as_ref()
            .zip(self.lsp.manager.as_ref())
            .and_then(|(key, manager)| manager.status(key));
        if self.state.lsp_status() != status.as_deref() {
            self.state.set_lsp_status(status);
            self.needs_redraw = true;
        }
    }
}
