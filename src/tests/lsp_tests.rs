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

#[test]
fn a_rust_file_in_front_starts_rust_analyzer_and_the_status_follows_it() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    rig.look();
    assert_eq!(rig.status().as_deref(), Some("rust-analyzer: starting"));
    rig.wait_for_status(Some("rust-analyzer"));

    assert_eq!(rig.launcher.launched(), 1);
    assert!(rig.texts().is_empty(), "{:?}", rig.texts());
}

#[test]
fn the_server_is_started_for_the_root_of_the_project_not_the_folder_of_the_file() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    let specs = rig.launcher.specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].command, "rust-analyzer");
    assert_eq!(specs[0].name, "rust");
    assert_eq!(
        crate::paths::without_verbatim_prefix(&crate::paths::canonical(&specs[0].root)),
        crate::paths::without_verbatim_prefix(&crate::paths::canonical(dir.path()))
    );
}

#[test]
fn the_status_shows_in_the_status_bar() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    let screen = rig.screen();

    assert!(screen.contains("[rust-analyzer]"), "{screen}");
}

#[test]
fn a_file_no_server_wants_has_no_status_and_starts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    write(&file, "hello\n");
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    rig.look();

    assert_eq!(rig.status(), None);
    assert_eq!(rig.launcher.launched(), 0);
    assert!(!rig.screen().contains("[rust-analyzer"));
}

#[test]
fn two_rust_files_of_one_workspace_share_one_server() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    );
    write(&dir.path().join("crates/a/Cargo.toml"), "[package]\n");
    let a = dir.path().join("crates/a/src/lib.rs");
    write(&a, "");
    write(&dir.path().join("crates/b/Cargo.toml"), "[package]\n");
    let b = dir.path().join("crates/b/src/lib.rs");
    write(&b, "");
    let mut rig = rig(dir.path(), &[a, b], FakeLauncher::new(|_| stays()));

    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.app
        .handle_input(InputEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::CONTROL,
        )));
    rig.wait_for_status(Some("rust-analyzer"));

    assert_eq!(rig.launcher.launched(), 1);
}

#[test]
fn switching_to_a_file_with_no_server_clears_the_status_and_back_again_shows_it_without_a_new_start()
 {
    let (dir, rust) = rust_project();
    let notes = dir.path().join("notes.txt");
    write(&notes, "hello\n");
    let mut rig = rig(dir.path(), &[rust, notes], FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    let next = || {
        InputEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::CONTROL,
        ))
    };

    rig.app.handle_input(next());
    assert_eq!(rig.status(), None, "the notes have no server");
    rig.app.handle_input(next());

    assert_eq!(rig.status().as_deref(), Some("rust-analyzer"));
    assert_eq!(rig.launcher.launched(), 1);
}

#[test]
fn a_server_that_cannot_start_is_one_sticky_error_and_the_status_says_off() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| None));

    rig.look();
    rig.wait_for_status(Some("rust-analyzer: off"));
    rig.look();
    rig.look();

    assert!(rig.app.state().notifications().has_errors());
    let texts = rig.texts();
    assert_eq!(texts.len(), 1, "{texts:?}");
    assert!(
        texts[0].contains("could not start") && texts[0].contains("was not found"),
        "{texts:?}"
    );
    assert_eq!(rig.launcher.launched(), 1, "not tried again");
}

#[test]
fn a_server_that_dies_is_restarted_and_the_user_is_told() {
    let (dir, file) = rust_project();
    let mut rig = rig(
        dir.path(),
        &[file],
        FakeLauncher::new(|number| if number == 0 { dies() } else { stays() }),
    );

    rig.look();
    let deadline = Instant::now() + Duration::from_secs(10);
    while rig.launcher.launched() < 2 || rig.status().as_deref() != Some("rust-analyzer") {
        while let Ok(event) = rig.events.try_recv() {
            rig.app.handle_event(event);
        }
        assert!(Instant::now() < deadline, "no restart: {:?}", rig.texts());
        std::thread::sleep(Duration::from_millis(2));
    }

    assert!(
        rig.texts()
            .iter()
            .any(|text| text.contains("restarting it (1 of 3)")),
        "{:?}",
        rig.texts()
    );
    assert!(!rig.app.state().notifications().has_errors());
}

#[test]
fn a_server_that_keeps_dying_ends_up_off_with_one_error() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| dies()));

    rig.look();
    rig.wait_for_status(Some("rust-analyzer: off"));

    assert_eq!(rig.launcher.launched(), 4);
    let errors: Vec<_> = rig
        .texts()
        .into_iter()
        .filter(|text| text.contains("stays off"))
        .collect();
    assert_eq!(errors.len(), 1, "{:?}", rig.texts());
    assert!(rig.app.state().notifications().has_errors());
}

// ---- the settings

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

#[test]
fn turning_the_server_off_in_the_settings_ends_it_and_clears_the_status() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let mut rig = config_app(dir.path(), &file, FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": null } }"#,
    );

    assert_eq!(rig.status(), None);
    assert_eq!(rig.launcher.launched(), 1);
}

#[test]
fn changing_the_server_in_the_settings_starts_the_new_one_for_the_file_in_front() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let mut rig = config_app(dir.path(), &file, FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": { "command": "ra-multiplex", "extensions": ["rs"] } } }"#,
    );
    rig.wait_for_status(Some("ra-multiplex"));

    assert_eq!(rig.launcher.launched(), 2);
    assert_eq!(rig.launcher.specs()[1].command, "ra-multiplex");
}

#[test]
fn a_server_that_was_off_gets_another_try_after_the_settings_change() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let mut rig = config_app(
        dir.path(),
        &file,
        FakeLauncher::new(|number| if number == 0 { None } else { stays() }),
    );
    rig.look();
    rig.wait_for_status(Some("rust-analyzer: off"));

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": { "command": "rust-analyzer", "extensions": ["rs"], "args": ["--log-file", "x"] } } }"#,
    );
    rig.wait_for_status(Some("rust-analyzer"));

    assert_eq!(rig.launcher.launched(), 2);
    assert_eq!(rig.launcher.specs()[1].args, ["--log-file", "x"]);
}

#[test]
fn a_reload_that_leaves_the_servers_alone_does_not_restart_them() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let mut rig = config_app(dir.path(), &file, FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    reload_with(&mut rig, &config, r#"{ "tab_size": 2 }"#);

    assert_eq!(rig.status().as_deref(), Some("rust-analyzer"));
    assert_eq!(rig.launcher.launched(), 1);
}

#[test]
fn the_initialization_options_from_the_settings_reach_the_launch() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let mut rig = config_app(dir.path(), &file, FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": { "command": "rust-analyzer", "extensions": ["rs"],
            "initialization_options": { "cargo": { "features": "all" } } } } }"#,
    );
    rig.wait_for_status(Some("rust-analyzer"));

    let specs = rig.launcher.specs();
    assert_eq!(
        specs[1].initialization_options,
        serde_json::json!({ "cargo": { "features": "all" } })
    );
}

// ---- telling the server about the documents

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

#[test]
fn a_file_the_server_serves_is_opened_with_its_text_once_the_server_runs() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());

    rig.sync();

    let opened = next_notification(&received, "textDocument/didOpen");
    let document = &opened["textDocument"];
    assert_eq!(document["languageId"], json!("rust"));
    assert_eq!(document["text"], json!("fn main() {}\n"));
    assert!(
        document["uri"].as_str().unwrap().ends_with("/src/main.rs"),
        "{document}"
    );
    assert_eq!(document["version"], json!(0));
    rig.sync();
    assert!(gets_nothing(&received), "opened once");
}

#[test]
fn nothing_is_sent_while_the_server_is_still_starting() {
    let (dir, file) = rust_project();
    let (sent, received) = std::sync::mpsc::channel::<Message>();
    let launcher = FakeLauncher::new(move |_| recording(sent.clone()));
    let mut rig = rig(dir.path(), &[file], launcher);

    rig.look();
    rig.sync();

    assert_eq!(rig.status().as_deref(), Some("rust-analyzer: starting"));
    assert!(gets_nothing(&received));
}

#[test]
fn typing_sends_the_change_with_its_position_and_the_new_version() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    // The cursor is at the start; "// " goes in front of the first line.
    rig.type_text("// ");
    rig.sync();

    let changed = next_notification(&received, "textDocument/didChange");
    assert_eq!(
        changed["contentChanges"][0]["range"],
        json!({ "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } })
    );
    assert_eq!(changed["contentChanges"][0]["text"], json!("// "));
    assert!(changed["textDocument"]["version"].as_u64().unwrap() > 0);
}

#[test]
fn typing_between_two_frames_is_one_message() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    rig.type_text("abcdef");
    rig.sync();

    let changed = next_notification(&received, "textDocument/didChange");
    assert_eq!(changed["contentChanges"][0]["text"], json!("abcdef"));
    assert!(gets_nothing(&received), "one message for six keys");
}

#[test]
fn undo_is_a_change_too() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");
    rig.type_text("abc");
    rig.sync();
    next_notification(&received, "textDocument/didChange");

    rig.press(KeyCode::Char('z'), KeyModifiers::CONTROL);
    rig.sync();

    let changed = next_notification(&received, "textDocument/didChange");
    assert_eq!(changed["contentChanges"][0]["text"], json!(""));
    assert_eq!(
        changed["contentChanges"][0]["range"]["end"],
        json!({ "line": 0, "character": 3 })
    );
}

#[test]
fn saving_sends_the_change_and_then_did_save() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    rig.type_text("x");
    rig.press(KeyCode::Char('s'), KeyModifiers::CONTROL);
    rig.sync();

    next_notification(&received, "textDocument/didChange");
    let saved = next_notification(&received, "textDocument/didSave");
    assert!(
        saved["textDocument"]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/src/main.rs"),
        "{saved}"
    );
    rig.sync();
    assert!(gets_nothing(&received), "told once");
}

#[test]
fn closing_the_tab_closes_the_document() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    rig.press(KeyCode::Char('w'), KeyModifiers::CONTROL);
    rig.sync();

    let closed = next_notification(&received, "textDocument/didClose");
    assert!(
        closed["textDocument"]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/src/main.rs"),
        "{closed}"
    );
}

#[test]
fn a_second_file_is_opened_when_it_is_shown_and_a_file_nobody_serves_is_not() {
    let (dir, file) = rust_project();
    let other = dir.path().join("src/other.rs");
    write(&other, "pub fn other() {}\n");
    let notes = dir.path().join("notes.txt");
    write(&notes, "hello\n");
    let (sent, received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| recording(sent.clone()));
    let mut rig = rig(dir.path(), &[file, other, notes], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    rig.sync();

    let first = next_notification(&received, "textDocument/didOpen");
    let second = next_notification(&received, "textDocument/didOpen");
    let mut uris = vec![
        first["textDocument"]["uri"].as_str().unwrap().to_owned(),
        second["textDocument"]["uri"].as_str().unwrap().to_owned(),
    ];
    uris.sort();
    assert!(
        uris[0].ends_with("/src/main.rs") && uris[1].ends_with("/src/other.rs"),
        "{uris:?}"
    );
    assert!(gets_nothing(&received), "the text file is not sent");
}

#[test]
fn a_server_that_started_again_is_told_about_the_open_documents_again() {
    let (dir, file) = rust_project();
    let (sent, received) = std::sync::mpsc::channel();
    // The first run dies after it has been told about the document.
    let launcher = FakeLauncher::new(move |number| {
        if number == 0 {
            recording_then_dying(sent.clone(), 1)
        } else {
            recording(sent.clone())
        }
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    // Wait for the restart, then sync as the loop would.
    let deadline = Instant::now() + Duration::from_secs(10);
    while rig.launcher.launched() < 2 || rig.status().as_deref() != Some("rust-analyzer") {
        while let Ok(event) = rig.events.try_recv() {
            rig.app.handle_event(event);
        }
        assert!(Instant::now() < deadline, "no restart");
        std::thread::sleep(Duration::from_millis(2));
    }
    rig.sync();

    let again = next_notification(&received, "textDocument/didOpen");
    assert_eq!(again["textDocument"]["text"], json!("fn main() {}\n"));
}

#[test]
fn a_reload_of_the_file_from_disk_reaches_the_server_as_a_change() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    fs::write(&file, "fn main() { println!(); }\n").unwrap();
    rig.app
        .handle_event(Event::FilesChanged(vec![file.clone()]));
    rig.sync();

    let changed = next_notification(&received, "textDocument/didChange");
    assert_eq!(changed["contentChanges"][0]["text"], json!(" println!(); "));
}

#[test]
fn changing_the_servers_in_the_settings_opens_the_documents_on_the_new_server() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let (sent, received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| recording(sent.clone()));
    let mut rig = config_app(dir.path(), &file, launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": { "command": "ra-multiplex", "extensions": ["rs"] } } }"#,
    );
    rig.wait_for_status(Some("ra-multiplex"));
    rig.sync();

    let opened = next_notification(&received, "textDocument/didOpen");
    assert_eq!(opened["textDocument"]["text"], json!("fn main() {}\n"));
}

// ---- the diagnostics a server publishes

use std::ops::Range;
use std::sync::mpsc::Sender;

use crate::components::workspace::DocumentId;
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

#[test]
fn what_the_server_publishes_shows_in_the_document_with_its_bytes_severity_and_message() {
    let (rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\nlet x = 1;\n",
        vec![
            diagnostic_json((0, 3), (0, 7), 1, "the name main is taken"),
            diagnostic_json((1, 4), (1, 5), 2, "unused variable: `x`"),
        ],
    );

    assert_eq!(
        rig.diagnostics(),
        vec![
            (3..7, Severity::Error, "the name main is taken".to_owned()),
            (17..18, Severity::Warning, "unused variable: `x`".to_owned()),
        ]
    );
}

#[test]
fn positions_after_a_wide_character_are_placed_by_utf_16_units() {
    let (rig, _received, _dir, _file) = Rig::with_published(
        "a😀b\n",
        vec![diagnostic_json((0, 3), (0, 4), 1, "on the b")],
    );

    // a is 1 byte and the emoji is 4, so the b is the sixth byte.
    assert_eq!(rig.diagnostics()[0].0, 5..6);
}

#[test]
fn text_typed_before_a_diagnostic_moves_it_along() {
    let (mut rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\n",
        vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
    );

    rig.type_text("// ");
    rig.sync();

    assert_eq!(rig.diagnostics()[0].0, 6..10);
}

#[test]
fn text_typed_inside_a_diagnostic_makes_it_longer() {
    let (mut rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\n",
        vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
    );
    for _ in 0..5 {
        rig.press(KeyCode::Right, KeyModifiers::NONE);
    }

    rig.type_text("XY");
    rig.sync();

    assert_eq!(rig.diagnostics()[0].0, 3..9);
}

#[test]
fn deleting_text_inside_a_diagnostic_drops_it_and_the_others_stay_in_place() {
    let (mut rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\nlet x = 1;\n",
        vec![
            diagnostic_json((0, 3), (0, 7), 1, "on main"),
            diagnostic_json((1, 4), (1, 5), 2, "on x"),
        ],
    );
    for _ in 0..5 {
        rig.press(KeyCode::Right, KeyModifiers::NONE);
    }

    rig.press(KeyCode::Backspace, KeyModifiers::NONE);
    rig.sync();

    let left = rig.diagnostics();
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(left[0].2, "on x");
    assert_eq!(left[0].0, 16..17, "one byte shorter before it");
}

#[test]
fn an_answer_about_an_older_version_is_placed_in_the_text_as_it_is_now() {
    let (dir, file) = rust_project();
    let (sent, _received) = std::sync::mpsc::channel();
    // A slow server: it answers the document it was opened with only after
    // it has been told about a change.
    let launcher = FakeLauncher::new(move |_| {
        responding(sent.clone(), |message| match method_of(message) {
            Some("textDocument/didChange") => {
                let uri = params_of(message)["textDocument"]["uri"].clone();
                // Version 0 is what it was opened with: "main" at columns 3 to 7.
                vec![publication(
                    &uri,
                    json!(0),
                    vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
                )]
            }
            _ => Vec::new(),
        })
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();

    rig.type_text("// ");
    rig.sync();
    rig.wait_until("the late answer", |rig| !rig.diagnostics().is_empty());

    assert_eq!(
        rig.diagnostics()[0].0,
        6..10,
        "on main, which moved three bytes"
    );
}

#[test]
fn text_typed_after_an_answer_but_before_the_next_sync_is_followed_too() {
    let (mut rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\n",
        vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
    );

    // Typed, and the frame has not happened yet: the server was not told.
    rig.type_text("// ");
    assert_eq!(
        rig.diagnostics()[0].0,
        3..7,
        "still where the server put it"
    );
    rig.sync();

    assert_eq!(rig.diagnostics()[0].0, 6..10);
}

#[test]
fn an_answer_about_a_version_that_is_not_kept_clears_the_document() {
    let (dir, file) = rust_project();
    let (sent, _received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| {
        responding(sent.clone(), |message| match method_of(message) {
            Some("textDocument/didOpen") => {
                let uri = params_of(message)["textDocument"]["uri"].clone();
                vec![publication(
                    &uri,
                    json!(0),
                    vec![diagnostic_json((0, 3), (0, 7), 1, "first")],
                )]
            }
            Some("textDocument/didChange") => {
                let uri = params_of(message)["textDocument"]["uri"].clone();
                vec![publication(
                    &uri,
                    json!(9999),
                    vec![diagnostic_json((0, 0), (0, 1), 1, "unplaceable")],
                )]
            }
            _ => Vec::new(),
        })
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    rig.wait_until("the first diagnostics", |rig| !rig.diagnostics().is_empty());

    rig.type_text("x");
    rig.sync();
    rig.wait_until("the unplaceable answer to be taken", |rig| {
        rig.diagnostics().is_empty()
    });
}

#[test]
fn publishing_nothing_clears_what_there_was() {
    let (dir, file) = rust_project();
    let (sent, _received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| {
        responding(sent.clone(), |message| {
            let uri = params_of(message)["textDocument"]["uri"].clone();
            match method_of(message) {
                Some("textDocument/didOpen") => {
                    vec![publication(
                        &uri,
                        json!(0),
                        vec![diagnostic_json((0, 3), (0, 7), 1, "x")],
                    )]
                }
                Some("textDocument/didChange") => vec![publication(&uri, Value::Null, Vec::new())],
                _ => Vec::new(),
            }
        })
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    rig.wait_until("diagnostics", |rig| !rig.diagnostics().is_empty());

    rig.type_text("x");
    rig.sync();

    rig.wait_until("them to be cleared", |rig| rig.diagnostics().is_empty());
}

#[test]
fn diagnostics_about_a_document_that_is_not_open_are_ignored() {
    let (dir, file) = rust_project();
    let (sent, _received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| {
        responding(sent.clone(), |message| match method_of(message) {
            Some("textDocument/didOpen") => vec![publication(
                &json!("file:///somewhere/else.rs"),
                json!(0),
                vec![diagnostic_json((0, 0), (0, 1), 1, "elsewhere")],
            )],
            _ => Vec::new(),
        })
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    rig.sync();
    std::thread::sleep(Duration::from_millis(150));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }

    assert!(rig.diagnostics().is_empty());
}

#[test]
fn reading_the_file_again_from_disk_clears_the_diagnostics() {
    let (mut rig, _received, _dir, file) = Rig::with_published(
        "fn main() {}\n",
        vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
    );

    fs::write(&file, "fn other() {}\n").unwrap();
    rig.app
        .handle_event(Event::FilesChanged(vec![file.clone()]));
    rig.sync();

    assert!(rig.diagnostics().is_empty());
}

#[test]
fn closing_the_tab_clears_its_diagnostics() {
    let (mut rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\n",
        vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
    );
    let document = rig.front_document();

    rig.press(KeyCode::Char('w'), KeyModifiers::CONTROL);
    rig.sync();

    assert!(rig.app.state().diagnostics().of(document).is_empty());
}

#[test]
fn a_server_that_dies_takes_its_diagnostics_with_it() {
    let (dir, file) = rust_project();
    let (sent, _received) = std::sync::mpsc::channel::<Message>();
    // The first run publishes and then dies; the restarted one says nothing.
    let launcher = FakeLauncher::new(move |number| {
        let sent = sent.clone();
        if number == 0 {
            Some(
                Box::new(move |session: &mut crate::lsp::tests::support::Session| {
                    let opened = session.next().expect("a didOpen");
                    let uri = params_of(&opened)["textDocument"]["uri"].clone();
                    session.notify(
                        "textDocument/publishDiagnostics",
                        publication(
                            &uri,
                            json!(0),
                            vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
                        ),
                    );
                    // A moment for it to be read before the connection ends.
                    std::thread::sleep(Duration::from_millis(100));
                }) as Script,
            )
        } else {
            responding(sent, |_| Vec::new())
        }
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    rig.wait_until("the diagnostics", |rig| !rig.diagnostics().is_empty());

    rig.wait_until("the restart", |rig| rig.launcher.launched() >= 2);
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();

    assert!(rig.diagnostics().is_empty(), "{:?}", rig.diagnostics());
}

#[test]
fn changing_the_servers_in_the_settings_clears_the_diagnostics() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let (sent, _received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| {
        responding(sent.clone(), |message| match method_of(message) {
            Some("textDocument/didOpen") => {
                let uri = params_of(message)["textDocument"]["uri"].clone();
                vec![publication(
                    &uri,
                    json!(0),
                    vec![diagnostic_json((0, 3), (0, 7), 1, "x")],
                )]
            }
            _ => Vec::new(),
        })
    });
    let mut rig = config_app(dir.path(), &file, launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    rig.wait_until("diagnostics", |rig| !rig.diagnostics().is_empty());

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": { "command": "ra-multiplex", "extensions": ["rs"] } } }"#,
    );

    assert!(rig.diagnostics().is_empty());
}

// ---- the proof: the real rust-analyzer through the whole app

/// A temporary crate with a type error on line 3 (counted from 0), opened in the
/// app with the real launcher: after the server has looked, the error is said
/// after the code on its line, underlined under the string, and
/// `f8` goes to it. Run by hand, since CI has no `rust-analyzer`:
/// `cargo test real_rust_analyzer_errors_show -- --ignored --nocapture`.
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_errors_show_on_the_right_lines_in_the_app() {
    use ratatui::style::Modifier;

    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("Cargo.toml"),
        "[package]\nname = \"small\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    let text = "fn main() {\n    let ok: u32 = 1;\n    let note = \"😀 fine\";\n    let wrong: u32 = \"not a number\";\n    println!(\"{ok} {note} {wrong}\");\n}\n";
    let file = dir.path().join("src/main.rs");
    write(&file, text);

    let (tx, mut events) = mpsc::channel(256);
    let mut app = App::new(opened(dir.path(), std::slice::from_ref(&file)))
        .with_events(tx)
        .with_lsp_launcher(Arc::new(crate::lsp::ProcessLauncher));
    app.handle_input(InputEvent::FocusLost);
    let document = app.state().workspace().active_document_id().unwrap();

    let deadline = Instant::now() + Duration::from_secs(120);
    while app.state().diagnostics().of(document).is_empty() {
        while let Ok(event) = events.try_recv() {
            app.handle_event(event);
        }
        // What the loop does before each frame.
        app.lsp_sync();
        assert!(
            Instant::now() < deadline,
            "no diagnostics in two minutes; status {:?}, notices {:?}",
            app.state().lsp_status(),
            app.state().notifications().texts()
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let draw = |app: &mut App| {
        let mut terminal = Terminal::new(TestBackend::new(120, 14)).unwrap();
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let row_with = |screen: &ratatui::buffer::Buffer, needle: &str| -> u16 {
        (0..screen.area.height)
            .find(|row| {
                (0..screen.area.width)
                    .map(|column| screen[(column, *row)].symbol().to_owned())
                    .collect::<String>()
                    .contains(needle)
            })
            .unwrap_or_else(|| panic!("no row shows {needle:?}"))
    };

    // Go to the error with f8, as a user would.
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::F(8),
        KeyModifiers::NONE,
    )));
    let screen = draw(&mut app);

    let row = row_with(&screen, "let wrong");
    let cells: Vec<_> = (0..screen.area.width)
        .map(|column| &screen[(column, row)])
        .collect();
    let string_start = cells
        .iter()
        .position(|cell| cell.symbol() == "\"")
        .expect("the string is on the row");
    let underlined: Vec<bool> = cells
        .iter()
        .map(|cell| cell.modifier.contains(Modifier::UNDERLINED))
        .collect();
    eprintln!(
        "underlined cells: {:?}",
        underlined
            .iter()
            .enumerate()
            .filter(|(_, on)| **on)
            .map(|(at, _)| at)
            .collect::<Vec<_>>()
    );
    let line: String = cells.iter().map(|cell| cell.symbol()).collect();
    eprintln!("the line: {}", line.trim_end());
    assert!(
        line.contains("expected u32"),
        "the message is after the code, on the line with the error: {line}"
    );
    assert!(!line.contains('●'), "and nothing looks like a breakpoint");
    assert!(underlined[string_start], "the string is underlined");
    assert!(
        !underlined[string_start - 2],
        "and what is before it is not"
    );
    assert_eq!(
        app.state().editor().selections().primary().head().line,
        3,
        "f8 went to the line with the error"
    );

    // f7 shows the whole message in a box.
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::F(7),
        KeyModifiers::NONE,
    )));
    let boxed = draw(&mut app);
    let box_text: String = (0..boxed.area.height)
        .map(|row| {
            (0..boxed.area.width)
                .map(|column| boxed[(column, row)].symbol().to_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    eprintln!("{box_text}");
    assert!(box_text.contains("Diagnostics"), "the box is open");
    assert!(
        box_text.contains("expected u32, found"),
        "with the whole message in it"
    );
}

/// The same in a Cargo workspace, opened at its top with a file in a member
/// crate: the server must start at the workspace's `Cargo.toml` and report the
/// member's error. Run by hand like the one above.
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_works_in_a_cargo_workspace() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/api\", \"crates/importer\"]\n\n[workspace.package]\nedition = \"2021\"\n",
    );
    for member in ["api", "importer"] {
        write(
            &dir.path().join(format!("crates/{member}/Cargo.toml")),
            &format!(
                "[package]\nname = \"{member}\"\nversion = \"0.1.0\"\nedition.workspace = true\n"
            ),
        );
    }
    write(
        &dir.path().join("crates/importer/src/main.rs"),
        "fn main() {}\n",
    );
    let file = dir.path().join("crates/api/src/main.rs");
    write(&file, "fn main() {\n    let wrong: u32 = \"text\";\n}\n");

    let (tx, mut events) = mpsc::channel(256);
    let mut app = App::new(opened(dir.path(), std::slice::from_ref(&file)))
        .with_events(tx)
        .with_lsp_launcher(Arc::new(crate::lsp::ProcessLauncher));
    app.handle_input(InputEvent::FocusLost);
    let document = app.state().workspace().active_document_id().unwrap();

    let deadline = Instant::now() + Duration::from_secs(120);
    while app.state().diagnostics().of(document).is_empty() {
        while let Ok(event) = events.try_recv() {
            app.handle_event(event);
        }
        app.lsp_sync();
        assert!(
            Instant::now() < deadline,
            "no diagnostics in two minutes; status {:?}, notices {:?}",
            app.state().lsp_status(),
            app.state().notifications().texts()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A crate inside the project that the project's own `Cargo.toml` does not know:
/// `cargo test real_rust_analyzer_in_a_nested_crate -- --ignored --nocapture`.
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_in_a_nested_crate_that_is_not_a_member_of_the_project() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("Cargo.toml"),
        "[package]\nname = \"outer\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(&dir.path().join("src/main.rs"), "fn main() {}\n");
    write(
        &dir.path().join("lib/inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    let file = dir.path().join("lib/inner/src/lib.rs");
    write(&file, "pub fn f() {\n    let wrong: u32 = \"text\";\n}\n");

    let (tx, mut events) = mpsc::channel(256);
    let mut app = App::new(opened(dir.path(), std::slice::from_ref(&file)))
        .with_events(tx)
        .with_lsp_launcher(Arc::new(crate::lsp::ProcessLauncher));
    app.handle_input(InputEvent::FocusLost);
    let document = app.state().workspace().active_document_id().unwrap();

    let deadline = Instant::now() + Duration::from_secs(90);
    while app.state().diagnostics().of(document).is_empty() {
        while let Ok(event) = events.try_recv() {
            app.handle_event(event);
        }
        app.lsp_sync();
        assert!(
            Instant::now() < deadline,
            "no diagnostics in 90 s; status {:?}, notices {:?}",
            app.state().lsp_status(),
            app.state().notifications().texts()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A nested crate that the project's `Cargo.toml` depends on by path:
/// `cargo test real_rust_analyzer_in_a_nested_crate_the_project_depends_on_by_path -- --ignored --nocapture`.
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_in_a_nested_crate_the_project_depends_on_by_path() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("Cargo.toml"),
        "[package]\nname = \"outer\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ninner = { path = \"lib/inner\" }\n",
    );
    write(&dir.path().join("src/main.rs"), "fn main() {}\n");
    write(
        &dir.path().join("lib/inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    let file = dir.path().join("lib/inner/src/lib.rs");
    write(&file, "pub fn f() {\n    let wrong: u32 = \"text\";\n}\n");

    let (tx, mut events) = mpsc::channel(256);
    let mut app = App::new(opened(dir.path(), std::slice::from_ref(&file)))
        .with_events(tx)
        .with_lsp_launcher(Arc::new(crate::lsp::ProcessLauncher));
    app.handle_input(InputEvent::FocusLost);
    let document = app.state().workspace().active_document_id().unwrap();

    let deadline = Instant::now() + Duration::from_secs(90);
    while app.state().diagnostics().of(document).is_empty() {
        while let Ok(event) = events.try_recv() {
            app.handle_event(event);
        }
        app.lsp_sync();
        assert!(
            Instant::now() < deadline,
            "no diagnostics in 90 s; status {:?}, notices {:?}",
            app.state().lsp_status(),
            app.state().notifications().texts()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

// ---- which server a file has

impl Rig {
    /// What "LSP: Show Server of This File" says now.
    fn server_report(&mut self) -> String {
        self.app
            .run_action(crate::event::action::Action::ShowLspServer);
        self.texts().last().cloned().unwrap_or_default()
    }
}

#[test]
fn the_report_says_when_no_entry_lists_the_extension() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    write(&file, "text");
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    let report = rig.server_report();

    assert!(
        report.contains("notes.txt: no entry in language_servers lists the extension txt"),
        "{report}"
    );
}

#[test]
fn the_report_says_when_the_file_is_outside_the_project_and_has_no_root() {
    let project = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let file = elsewhere.path().join("src/lib.rs");
    write(&file, "");
    let mut rig = rig(project.path(), &[file], FakeLauncher::new(|_| stays()));

    let report = rig.server_report();

    assert!(report.contains("outside the project"), "{report}");
    assert!(report.contains("Cargo.toml"), "{report}");
}

#[test]
fn the_report_follows_the_server_from_not_started_to_open() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    let before = rig.server_report();
    assert!(before.contains("has not been started"), "{before}");

    rig.look();
    let starting = rig.server_report();
    assert!(starting.contains("rust-analyzer: starting"), "{starting}");
    assert!(starting.contains("not running"), "{starting}");

    rig.wait_for_status(Some("rust-analyzer"));
    let running = rig.server_report();
    assert!(
        running.contains("the file is not open on it yet"),
        "{running}"
    );

    rig.app.lsp_sync();
    let open = rig.server_report();
    assert!(open.contains("main.rs: rust-analyzer"), "{open}");
    assert!(open.contains("the file is open on it"), "{open}");
    assert!(
        open.contains(
            &crate::paths::without_verbatim_prefix(&crate::paths::canonical(dir.path()))
                .display()
                .to_string()
        ),
        "{open}"
    );
}

#[test]
fn the_report_of_a_server_that_is_off_says_so() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| None));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer: off"));

    let report = rig.server_report();

    assert!(report.contains("rust-analyzer: off"), "{report}");
    assert!(report.contains("not running"), "{report}");
}
