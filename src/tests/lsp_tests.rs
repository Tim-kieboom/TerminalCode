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
    write(&dir.path().join("Cargo.toml"), "[workspace]\n");
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
