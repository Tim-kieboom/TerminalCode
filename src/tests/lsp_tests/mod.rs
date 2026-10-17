//! The app and its language servers, with fake servers: a file in front gets
//! the server it needs, and the user hears what happens to it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::Event as InputEvent;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::initial_state;
use crate::lsp::tests::support::{FakeLauncher, dies, stays};

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    launcher: Arc<FakeLauncher>,
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// An app with `files` open, the first in front, the project in `project`, and
/// a fake launcher.
fn rig(project: &Path, files: &[PathBuf], launcher: Arc<FakeLauncher>) -> Rig {
    let (tx, events) = mpsc::channel(256);
    let app = App::new(opened(project, files))
        .with_events(tx)
        .with_lsp_launcher(Arc::clone(&launcher) as _);
    Rig {
        app,
        events,
        launcher,
    }
}

impl Rig {
    /// Any input makes the app look at the file in front.
    fn look(&mut self) {
        self.app.handle_input(InputEvent::FocusLost);
    }

    fn status(&self) -> Option<String> {
        self.app.state().lsp_status().map(str::to_owned)
    }

    /// Takes what the servers sent to the app until the status is `wanted`.
    fn wait_for_status(&mut self, wanted: Option<&str>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            if self.status().as_deref() == wanted {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the status stayed {:?}, wanted {wanted:?}",
                self.status()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn texts(&self) -> Vec<String> {
        self.app
            .state()
            .notifications()
            .texts()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn screen(&mut self) -> String {
        let mut terminal = Terminal::new(TestBackend::new(140, 20)).unwrap();
        let app = &mut self.app;
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        terminal.backend().to_string()
    }
}

/// The state of an app started on `project` with `files` open.
fn opened(project: &Path, files: &[PathBuf]) -> crate::app::state::AppState {
    let mut state = initial_state(files).unwrap();
    state.open_project(project).unwrap();
    state
}

fn rust_project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("Cargo.toml"), "[package]\n");
    let file = dir.path().join("src/main.rs");
    write(&file, "fn main() {}\n");
    (dir, file)
}

fn config_app(dir: &Path, file: &Path, launcher: Arc<FakeLauncher>) -> Rig {
    let config = dir.join("config");
    fs::create_dir_all(&config).unwrap();
    let (tx, events) = mpsc::channel(256);
    let app = App::new(opened(dir, std::slice::from_ref(&file.to_path_buf())))
        .with_events(tx)
        .with_config_dir(Some(config))
        .with_lsp_launcher(Arc::clone(&launcher) as _);
    Rig {
        app,
        events,
        launcher,
    }
}

fn reload_with(rig: &mut Rig, config: &Path, settings: &str) {
    write(&config.join("settings.json"), settings);
    rig.app
        .handle_event(Event::FilesChanged(vec![config.join("settings.json")]));
}

use std::sync::mpsc::Receiver;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use crate::lsp::Message;
use crate::lsp::tests::support::{recording, recording_then_dying};

/// The next notification with `method` the server was sent, skipping the rest.
fn next_notification(received: &Receiver<Message>, method: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match received.recv_timeout(left) {
            Ok(Message::Notification {
                method: got,
                params,
            }) if got == method => return params,
            Ok(_) => {}
            Err(_) => panic!("the server was never sent {method}"),
        }
    }
}

/// Whether the server was sent a notification with `method` in the next moment.
fn gets_nothing(received: &Receiver<Message>) -> bool {
    received.recv_timeout(Duration::from_millis(150)).is_err()
}

impl Rig {
    fn press(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c), KeyModifiers::NONE);
        }
    }

    /// What the loop does before each frame.
    fn sync(&mut self) {
        self.app.lsp_sync();
    }

    /// A rig whose server passes on everything it is sent, with the server running.
    fn recording(file: &Path, project: &Path) -> (Rig, Receiver<Message>) {
        let (sent, received) = std::sync::mpsc::channel();
        let launcher = FakeLauncher::new(move |_| recording(sent.clone()));
        let mut rig = rig(project, &[file.to_path_buf()], launcher);
        rig.look();
        rig.wait_for_status(Some("rust-analyzer"));
        (rig, received)
    }
}

use std::ops::Range;
use std::sync::mpsc::Sender;

use crate::ids::DocumentId;
use crate::lsp::Severity;
use crate::lsp::tests::support::Script;

/// A server that passes on what it is sent and answers each message with the
/// `publishDiagnostics` params that `respond` gives for it.
fn responding(
    sent: Sender<Message>,
    respond: impl Fn(&Message) -> Vec<Value> + Send + 'static,
) -> Option<Script> {
    Some(Box::new(move |session| {
        while let Some(message) = session.next() {
            for params in respond(&message) {
                session.notify("textDocument/publishDiagnostics", params);
            }
            let _ = sent.send(message);
        }
    }))
}

fn method_of(message: &Message) -> Option<&str> {
    match message {
        Message::Notification { method, .. } => Some(method),
        _ => None,
    }
}

fn params_of(message: &Message) -> &Value {
    match message {
        Message::Notification { params, .. } => params,
        _ => &Value::Null,
    }
}

/// One diagnostic from `start` to `end` as (line, character) pairs.
fn diagnostic_json(start: (u32, u32), end: (u32, u32), severity: u8, message: &str) -> Value {
    json!({
        "range": {
            "start": { "line": start.0, "character": start.1 },
            "end": { "line": end.0, "character": end.1 },
        },
        "severity": severity,
        "message": message,
        "source": "rustc",
    })
}

fn publication(uri: &Value, version: Value, diagnostics: Vec<Value>) -> Value {
    json!({ "uri": uri, "version": version, "diagnostics": diagnostics })
}

impl Rig {
    fn wait_until(&mut self, what: &str, done: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "never happened: {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn front_document(&self) -> DocumentId {
        self.app.state().workspace().lsp_documents()[0].id
    }

    fn diagnostics(&self) -> Vec<(Range<usize>, Severity, String)> {
        self.app
            .state()
            .diagnostics()
            .of(self.front_document())
            .iter()
            .map(|d| (d.range.clone(), d.severity, d.message.clone()))
            .collect()
    }

    /// A rig whose server publishes `diagnostics` for version 0 as soon as it
    /// is told about the document, with the diagnostics already arrived.
    fn with_published(
        text: &str,
        diagnostics: Vec<Value>,
    ) -> (Rig, Receiver<Message>, tempfile::TempDir, PathBuf) {
        let (dir, file) = rust_project();
        write(&file, text);
        let (sent, received) = std::sync::mpsc::channel();
        let launcher = FakeLauncher::new(move |_| {
            let diagnostics = diagnostics.clone();
            responding(sent.clone(), move |message| match method_of(message) {
                Some("textDocument/didOpen") => {
                    let uri = params_of(message)["textDocument"]["uri"].clone();
                    vec![publication(&uri, json!(0), diagnostics.clone())]
                }
                _ => Vec::new(),
            })
        });
        let mut rig = rig(dir.path(), std::slice::from_ref(&file), launcher);
        rig.look();
        rig.wait_for_status(Some("rust-analyzer"));
        rig.sync();
        rig.wait_until("the diagnostics", |rig| !rig.diagnostics().is_empty());
        (rig, received, dir, file)
    }
}

impl Rig {
    /// What "LSP: Show Server of This File" says now.
    fn server_report(&mut self) -> String {
        self.app
            .run_action(crate::event::action::Action::ShowLspServer);
        self.texts().last().cloned().unwrap_or_default()
    }
}

mod basics;
mod diagnostics;
mod documents;
mod real_rust_analyzer;
mod server_report;
mod settings;
