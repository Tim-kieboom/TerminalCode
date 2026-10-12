//! The language servers: starting the one the file in front needs, and saying
//! how it is doing. The servers themselves are run by [`Manager`].

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::components::workspace::{DocumentId, LspDocument};
use crate::event::Event;
#[cfg(not(test))]
use crate::lsp::ProcessLauncher;
use crate::lsp::uri::file_uri;
use crate::lsp::{
    Diagnostic, DocSnapshot, DocumentSync, LanguageServers, LaunchSpec, Launcher, Manager, Notice,
    ServerKey, language_id, parse_published,
};
use crate::paths::{canonical, without_verbatim_prefix};

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
    /// What each server has been told about the documents it serves.
    sync: DocumentSync,
    /// Which server each open document goes to, found once for each path.
    targets: HashMap<DocumentId, (PathBuf, Option<Target>)>,
    /// What the documents and servers were like when they were last synced.
    fingerprint: u64,
}

impl Default for Lsp {
    fn default() -> Self {
        Self {
            manager: None,
            launcher: Self::default_launcher(),
            shown: None,
            sync: DocumentSync::default(),
            targets: HashMap::new(),
            fingerprint: 0,
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
        let notifications = manager.take_notifications();
        self.show_lsp_notices(notices);
        for notification in notifications {
            if notification.method == "textDocument/publishDiagnostics" {
                self.take_published_diagnostics(&notification.key, &notification.params);
            }
        }
        self.refresh_lsp_status();
        self.needs_redraw = true;
    }

    /// A server published the diagnostics of a document: puts them in the text
    /// the server was last told, from the version it looked at. Ones about a
    /// document that is not open are not wanted.
    fn take_published_diagnostics(&mut self, key: &ServerKey, params: &serde_json::Value) {
        let Some(published) = parse_published(params) else {
            return;
        };
        let Some(document) = self.document_with_uri(key, &published.uri) else {
            return;
        };
        let ranges: Vec<_> = published
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.start, diagnostic.end))
            .collect();
        let Some(placed) =
            self.lsp
                .sync
                .ranges_in_told_text(key, document, published.version, &ranges)
        else {
            // About a text that is not kept any more: nothing it says can be placed.
            self.state.diagnostics_mut().clear(document);
            return;
        };
        let diagnostics = published
            .diagnostics
            .into_iter()
            .zip(placed)
            .filter_map(|(diagnostic, range)| {
                Some(Diagnostic {
                    range: range?,
                    severity: diagnostic.severity,
                    message: diagnostic.message,
                    source: diagnostic.source,
                    code: diagnostic.code,
                })
            })
            .collect();
        self.state.diagnostics_mut().set(document, diagnostics);
        self.needs_redraw = true;
    }

    /// The open document that the server `key` knows by `uri`.
    fn document_with_uri(&self, key: &ServerKey, uri: &str) -> Option<DocumentId> {
        self.lsp.targets.iter().find_map(|(id, (_, target))| {
            let target = target.as_ref()?;
            (target.key == *key && target.uri == uri).then_some(*id)
        })
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
        self.lsp.sync.clear();
        self.state.diagnostics_mut().clear_all();
        self.lsp.targets.clear();
        self.lsp.fingerprint = 0;
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

/// Where a document goes: the server it is served by, and how that server
/// names it.
#[derive(Debug, Clone)]
struct Target {
    key: ServerKey,
    uri: String,
    language_id: String,
}

impl App {
    /// Tells the running servers what changed in the documents they serve since
    /// the last time: `didOpen` for a document they have not seen, `didChange`
    /// for what was typed, `didSave`, and `didClose`. Called before each frame,
    /// so a burst of typing between two frames is one message.
    pub(crate) fn lsp_sync(&mut self) {
        let project = self.project_root();
        let Some(manager) = self.lsp.manager.as_mut() else {
            return;
        };
        let running = manager.running();
        if running.is_empty() {
            self.drop_stopped_servers(&running);
            return;
        }
        let docs = self.state.workspace().lsp_documents();
        let fingerprint = fingerprint(&docs, &running);
        if fingerprint == self.lsp.fingerprint {
            return;
        }
        self.lsp.fingerprint = fingerprint;

        let mut by_server: HashMap<ServerKey, Vec<DocSnapshot>> = HashMap::new();
        for doc in docs {
            let cached = self
                .lsp
                .targets
                .get(&doc.id)
                .filter(|(path, _)| *path == doc.path);
            let target = match cached {
                Some((_, target)) => target.clone(),
                None => {
                    let target = find_target(&self.settings.language_servers, &doc.path, &project);
                    self.lsp
                        .targets
                        .insert(doc.id, (doc.path.clone(), target.clone()));
                    target
                }
            };
            let Some(target) = target else {
                continue;
            };
            by_server.entry(target.key).or_default().push(DocSnapshot {
                id: doc.id,
                uri: target.uri,
                language_id: target.language_id,
                text: doc.text,
                version: doc.version,
                saves: doc.saves,
                reloads: doc.reloads,
            });
        }

        let mut moved = Vec::new();
        for (key, generation) in &running {
            let snapshots = by_server.get(key).map_or(&[][..], Vec::as_slice);
            let output = self.lsp.sync.update(key, *generation, snapshots);
            for message in output.messages {
                manager.notify(key, message.method, message.params);
            }
            moved.push((output.resets, output.changes));
        }
        self.drop_stopped_servers(&running);
        // What the texts went through, so that the diagnostics follow them.
        for (resets, changes) in moved {
            for document in resets {
                self.state.diagnostics_mut().clear(document);
            }
            for (document, change) in changes {
                self.state.diagnostics_mut().remap(document, &change);
            }
        }
    }
}

/// The server for the file at `path` and how it names it, if there is one.
fn find_target(servers: &LanguageServers, path: &Path, project: &Path) -> Option<Target> {
    let (key, _) = servers.resolve(path, project)?;
    let uri = file_uri(&without_verbatim_prefix(&canonical(path)))?;
    let extension = path.extension()?.to_str()?;
    Some(Target {
        key,
        uri,
        language_id: language_id(extension),
    })
}

/// A number that changes when any document or any server run does.
fn fingerprint(docs: &[LspDocument], running: &[(ServerKey, u64)]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for doc in docs {
        (doc.id, &doc.path, doc.version, doc.saves).hash(&mut hasher);
    }
    let mut running: Vec<_> = running.iter().collect();
    running.sort();
    running.hash(&mut hasher);
    hasher.finish()
}

impl App {
    /// Servers that are not running any more have nothing open, and what they
    /// said about the documents is no longer anything to go by.
    fn drop_stopped_servers(&mut self, running: &[(ServerKey, u64)]) {
        let gone: Vec<ServerKey> = self
            .lsp
            .sync
            .servers()
            .into_iter()
            .filter(|key| !running.iter().any(|(running, _)| running == key))
            .collect();
        for key in gone {
            for document in self.lsp.sync.drop_server(&key) {
                self.state.diagnostics_mut().clear(document);
            }
        }
    }
}

impl App {
    /// `f8` and `shift+f8`: the cursor goes to the next or the previous
    /// diagnostic of the open file, round to the other end after the last.
    pub(super) fn step_diagnostic(&mut self, next: bool) {
        let Some(document) = self.state.workspace().active_document_id() else {
            return;
        };
        let head = self.state.editor().selections().primary().head();
        let Ok(byte) = self.state.editor().buffer().position_to_byte(head) else {
            return;
        };
        let diagnostics = self.state.diagnostics();
        let found = match next {
            true => diagnostics.next_after(document, byte),
            false => diagnostics.previous_before(document, byte),
        };
        let Some(start) = found.map(|diagnostic| diagnostic.range.start) else {
            self.state
                .notify("no diagnostics in this file (a language server reports them)");
            return;
        };
        match self.state.editor().buffer().byte_to_position(start) {
            Ok(position) => self.state.edit(|editor| editor.go_to(position)),
            Err(error) => self.state.notify_error(error.to_string()),
        }
        self.needs_redraw = true;
    }
}
