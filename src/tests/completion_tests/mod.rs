//! Completion as you type, from the keys to the box on the screen, against a
//! fake server that answers `textDocument/completion` with what a test gives it.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::app::App;
use crate::components::editor::Motion;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::initial_state;
use crate::lsp::Message;
use crate::lsp::test_support::{FakeLauncher, Script};

pub(super) type Answer = Arc<dyn Fn(&Value) -> Value + Send + Sync>;

/// A server that passes on what it is sent, and answers each completion request
/// with what `answer` makes of its params.
pub(super) fn completing(sent: Sender<Message>, answer: Answer) -> Option<Script> {
    Some(Box::new(move |session| {
        while let Some(message) = session.next() {
            let request = match &message {
                Message::Request { id, method, params } => {
                    Some((id.clone(), method.clone(), params.clone()))
                }
                _ => None,
            };
            let _ = sent.send(message);
            match request {
                Some((id, method, params))
                    if method == "textDocument/completion"
                        || method == "textDocument/hover"
                        || method == "textDocument/definition"
                        || method == "textDocument/prepareRename"
                        || method == "textDocument/rename"
                        || method == "textDocument/codeAction"
                        || method == "codeAction/resolve"
                        || method == "workspace/executeCommand" =>
                {
                    let value = answer(&params);
                    // An answer that says `__error` is an error of the server.
                    match value.get("__error").and_then(Value::as_str) {
                        Some(message) => session.respond_error(id, -32803, message),
                        None => session.respond(id, value),
                    }
                }
                // The item comes back with the edits its `data` says it needs, or
                // the server refuses when `data` says so.
                Some((id, method, params)) if method == "completionItem/resolve" => {
                    let data = &params["data"];
                    if data["fail"] == json!(true) {
                        session.respond_error(id, -32803, "no");
                    } else {
                        let mut item = params.clone();
                        item["additionalTextEdits"] = data["resolve_edits"].clone();
                        session.respond(id, item);
                    }
                }
                _ => {}
            }
        }
    }))
}

pub(super) struct Rig {
    pub(super) app: App,
    pub(super) events: mpsc::Receiver<Event>,
    pub(super) received: Receiver<Message>,
    pub(super) _dir: tempfile::TempDir,
}

fn items(labels: &[&str]) -> Value {
    Value::Array(
        labels
            .iter()
            .map(|label| json!({ "label": label, "kind": 3, "detail": format!("fn {label}()") }))
            .collect(),
    )
}

pub(super) fn answering(list: Value) -> Answer {
    Arc::new(move |_| list.clone())
}

pub(super) fn provider() -> Value {
    json!({ "textDocumentSync": 2, "completionProvider": { "triggerCharacters": ["."] } })
}

pub(super) fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

impl Rig {
    /// An app over a Rust file with `text` in it, and a server with `capabilities`
    /// that answers with `answer`, running.
    pub(super) fn start(capabilities: Value, text: &str, answer: Answer) -> Self {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Cargo.toml"), "[package]\n");
        let file = dir.path().join("src/main.rs");
        write(&file, text);
        let (sent, received) = std::sync::mpsc::channel();
        let launcher = FakeLauncher::with_capabilities(capabilities, move |_| {
            completing(sent.clone(), Arc::clone(&answer))
        });
        Self::with_launcher(dir, file, launcher, received)
    }

    /// An app over `file` in the project `dir`, with servers started by `launcher`,
    /// once the server runs; the cursor is at the end of the text.
    pub(super) fn with_launcher(
        dir: tempfile::TempDir,
        file: std::path::PathBuf,
        launcher: Arc<dyn crate::lsp::Launcher>,
        received: Receiver<Message>,
    ) -> Self {
        let mut state = initial_state(&[file]).unwrap();
        state.open_project(dir.path()).unwrap();
        let (tx, events) = mpsc::channel(256);
        let app = App::new(state).with_events(tx).with_lsp_launcher(launcher);
        let mut rig = Self {
            app,
            events,
            received,
            _dir: dir,
        };
        rig.app.handle_input(InputEvent::FocusLost);
        rig.pump_until(|app| app.state().lsp_status() == Some("rust-analyzer"));
        rig.app
            .state_mut()
            .edit(|editor| editor.move_cursor(Motion::DocumentEnd).unwrap());
        rig.frame();
        rig
    }

    pub(super) fn with_answer(list: Value) -> Self {
        Self::start(provider(), "", answering(list))
    }

    /// What the loop does before each frame.
    pub(super) fn frame(&mut self) {
        self.app.lsp_sync();
        self.app.completion_step();
    }

    /// Takes what the servers sent to the app until `done`.
    pub(super) fn pump_until(&mut self, done: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            if done(&self.app) {
                return;
            }
            assert!(Instant::now() < deadline, "it never happened");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn wait_for_box(&mut self) {
        self.pump_until(|app| app.state().completion_box().is_some());
    }

    pub(super) fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    /// Types `text` one key at a time, with a frame after each, as the loop does.
    pub(super) fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c));
            self.frame();
        }
    }

    pub(super) fn text(&self) -> String {
        self.app.state().editor().buffer().text()
    }

    /// The completion requests the server was sent so far.
    pub(super) fn requests(&self) -> Vec<Value> {
        let mut found = Vec::new();
        while let Ok(message) = self.received.recv_timeout(Duration::from_millis(150)) {
            if let Message::Request { method, params, .. } = message
                && method == "textDocument/completion"
            {
                found.push(params);
            }
        }
        found
    }

    pub(super) fn shown(&self) -> Vec<String> {
        self.app
            .state()
            .completion_box()
            .map(|open| {
                open.completion()
                    .items()
                    .map(|item| item.label.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn highlighted(&self) -> Option<String> {
        let open = self.app.state().completion_box()?;
        Some(open.completion().selected_item()?.label.clone())
    }

    pub(super) fn screen(&mut self) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        let app = &mut self.app;
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        terminal.backend().to_string()
    }
}

// ---- asking, and showing what comes back

fn import_edit() -> Value {
    json!({
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
        "newText": "use std::collections::HashMap;\n",
    })
}

fn hash_map_with_data(data: Value) -> Answer {
    Arc::new(move |_| {
        json!([{
            "label": "HashMap",
            "kind": 22,
            "data": data.clone(),
            "textEdit": {
                "range": { "start": { "line": 1, "character": 11 }, "end": { "line": 1, "character": 15 } },
                "newText": "HashMap",
            },
        }])
    })
}

const HASH_SOURCE: &str = "fn f() {\n    let m: Has";

/// Every message the server has been sent so far, in order.
fn everything_sent(rig: &Rig) -> Vec<Message> {
    let mut all = Vec::new();
    while let Ok(message) = rig.received.recv_timeout(Duration::from_millis(150)) {
        all.push(message);
    }
    all
}

fn call_item(label: &str, snippet: &str, documentation: Option<&str>) -> Value {
    let mut item = json!({
        "label": label,
        "kind": 2,
        "detail": format!("fn {label}()"),
        "insertTextFormat": 2,
        "insertText": snippet,
    });
    if let Some(docs) = documentation {
        item["documentation"] = json!({ "kind": "markdown", "value": docs });
    }
    item
}

fn cursor_column(rig: &Rig) -> usize {
    rig.app
        .state()
        .editor()
        .selections()
        .primary()
        .head()
        .column
}

fn long_docs() -> Value {
    let long: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    json!([call_item("pop", "pop()$0", Some(&long.join("\n")))])
}

fn wheel(rig: &mut Rig, kind: crossterm::event::MouseEventKind, column: u16, row: u16) {
    rig.app
        .handle_input(InputEvent::Mouse(crossterm::event::MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
}

/// The screen cell where `needle` starts.
fn cell_of(rig: &mut Rig, needle: &str) -> (u16, u16) {
    let screen = rig.screen();
    for (row, line) in screen.lines().enumerate() {
        let line = line.trim_matches('"');
        if let Some(byte) = line.find(needle) {
            return (line[..byte].chars().count() as u16, row as u16);
        }
    }
    panic!("the screen does not show {needle:?}: {screen}");
}

fn docs_rig() -> Rig {
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n\n\n\n\n\n\n\n",
        answering(long_docs()),
    );
    rig.type_text("p");
    rig.wait_for_box();
    rig
}

#[cfg(unix)]
impl Rig {
    /// An app over a temporary crate whose `src/main.rs` holds `text`, with the
    /// real `rust-analyzer`.
    pub(super) fn real(text: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("Cargo.toml"),
            "[package]\nname = \"small\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        let file = dir.path().join("src/main.rs");
        write(&file, text);
        let (_, received) = std::sync::mpsc::channel();
        Self::with_launcher(dir, file, Arc::new(crate::lsp::ProcessLauncher), received)
    }

    /// Types `text` and waits for the box to offer `wanted`. The server may still
    /// be indexing and answer with nothing at first, so a try that comes to
    /// nothing is taken back and made again, for up to two minutes.
    pub(super) fn type_until_offered(&mut self, text: &str, wanted: &str) {
        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline {
            self.type_text(text);
            let tried = Instant::now() + Duration::from_secs(15);
            while Instant::now() < tried {
                while let Ok(event) = self.events.try_recv() {
                    self.app.handle_event(event);
                }
                self.frame();
                if self.shown().iter().any(|label| label.starts_with(wanted)) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            for _ in text.chars() {
                self.press(KeyCode::Backspace);
                self.frame();
            }
            self.app.state_mut().notify("retrying");
        }
        panic!(
            "{wanted:?} was never offered; the box shows {:?}, status {:?}",
            self.shown(),
            self.app.state().lsp_status()
        );
    }

    /// Moves the highlight to the item called `label`.
    pub(super) fn highlight(&mut self, label: &str) {
        for _ in 0..self.shown().len() {
            if self
                .highlighted()
                .is_some_and(|shown| shown.starts_with(label))
            {
                return;
            }
            self.press(KeyCode::Down);
        }
        panic!("{label:?} is not among {:?}", self.shown());
    }
}

mod basics;
mod box_goes_away;
mod call_brackets_and_docs;
mod import_with_a_suggestion;
mod import_worked_out_on_pick;
mod keys;
mod real_rust_analyzer;
mod scrolling_docs;
