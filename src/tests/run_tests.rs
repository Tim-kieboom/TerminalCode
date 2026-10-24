//! Run, from the key to the program in its tab, with a stand-in for `cargo` (a script that
//! answers `metadata` and `build` with what it is told to) and a program that is a script.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::json;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus, Popup};
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::event::action::Action;
use crate::persist::saved_state::SavedState;

/// What the stand-in cargo and the program do.
pub(super) struct Setup {
    /// What `cargo build` prints on its standard error, and how long it takes.
    pub(super) build_says: &'static str,
    pub(super) build_seconds: &'static str,
    pub(super) build_code: i32,
    /// The script of the program.
    pub(super) program: &'static str,
    /// The file `.trcode/launch.toml`, when there is one.
    pub(super) launch_file: Option<&'static str>,
    /// Whether the project has a runnable target.
    pub(super) has_target: bool,
    /// The program that is cargo, when it is not the stand-in.
    pub(super) cargo: Option<&'static str>,
    /// Whether the user trusts the folder already.
    pub(super) trusted: bool,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            build_says: "Compiling app v0.1.0",
            build_seconds: "0",
            build_code: 0,
            program: "echo hello from the program",
            launch_file: None,
            has_target: true,
            cargo: None,
            trusted: true,
        }
    }
}

pub(super) struct Rig {
    pub(super) app: App,
    pub(super) events: mpsc::Receiver<Event>,
    pub(super) terminal: Terminal<TestBackend>,
    /// Keeps the project folder until the rig is dropped.
    pub(super) _dir: tempfile::TempDir,
    /// The folder `state.json` is written to.
    pub(super) config: tempfile::TempDir,
    pub(super) root: PathBuf,
}

fn executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

pub(super) fn rig(setup: Setup) -> Rig {
    rig_with(setup, None)
}

/// The rig, with `adapter` to start the debugger with when there is one.
pub(super) fn rig_with(setup: Setup, adapter: Option<crate::app::debug::Adapter>) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::write(root.join("a.txt"), "alpha\n").unwrap();
    let program = root.join("program.sh");
    executable(
        &program,
        &format!(
            "#!/bin/sh\necho started >> starts.txt\necho \"manifest=$CARGO_MANIFEST_DIR pkg=$CARGO_PKG_NAME\"\n{}\n",
            setup.program
        ),
    );
    let targets = match setup.has_target {
        true => vec![json!({
            "name": "app", "kind": ["bin"], "crate_types": ["bin"],
            "src_path": root.join("src/main.rs"),
        })],
        false => Vec::new(),
    };
    let metadata = json!({ "packages": [
        { "name": "app", "manifest_path": root.join("Cargo.toml"), "targets": targets },
    ]});
    fs::write(root.join("metadata.json"), metadata.to_string()).unwrap();
    let artifact = json!({
        "reason": "compiler-artifact",
        "target": { "name": "app", "kind": ["bin"], "crate_types": ["bin"] },
        "executable": program,
    });
    fs::write(root.join("artifact.json"), format!("{artifact}\n")).unwrap();
    executable(
        &root.join("cargo.sh"),
        &format!(
            "#!/bin/sh\n\
             case \"$1\" in\n\
             metadata) cat metadata.json ;;\n\
             build)\n\
               echo \"$@\" > build-args.txt\n\
               cat a.txt > seen-by-build.txt\n\
               echo \"{says}\" >&2\n\
               sleep {seconds}\n\
               cat artifact.json\n\
               exit {code} ;;\n\
             esac\n",
            says = setup.build_says,
            seconds = setup.build_seconds,
            code = setup.build_code,
        ),
    );
    if let Some(text) = setup.launch_file {
        fs::create_dir_all(root.join(".trcode")).unwrap();
        fs::write(root.join(".trcode/launch.toml"), text).unwrap();
    }
    let config = tempfile::tempdir().unwrap();
    let mut saved = SavedState::default();
    if setup.trusted {
        saved.trust(&root.to_string_lossy());
    }
    let stand_in = root.join("cargo.sh");
    let cargo = setup.cargo.unwrap_or(stand_in.to_str().unwrap());
    let (mut app, events) = app_in(&root, &config, saved, cargo);
    if let Some(adapter) = adapter {
        app = app.with_debug_adapter(adapter);
    }
    let mut rig = Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(110, 30)).unwrap(),
        _dir: dir,
        config,
        root,
    };
    rig.draw();
    rig
}

/// An app for the project at `root` that remembers in `config`, starting with what was
/// `saved`.
fn app_in(
    root: &Path,
    config: &tempfile::TempDir,
    saved: SavedState,
    cargo: &str,
) -> (App, mpsc::Receiver<Event>) {
    let mut state = AppState::default();
    state.open_project(root).unwrap();
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state)
        .with_events(tx)
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(saved)
        .with_cargo(cargo);
    (app, events)
}

impl Rig {
    pub(super) fn draw(&mut self) {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
    }

    pub(super) fn run(&mut self) {
        self.app.run_action(Action::Run);
    }

    pub(super) fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    pub(super) fn wait_for(&mut self, what: &str, condition: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            self.draw();
            if condition(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}:\n{}\n{:?}",
                self.screen(),
                self.said()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub(super) fn wait_for_text(&mut self, text: &str) {
        self.wait_for(text, |rig| rig.screen().contains(text));
    }

    pub(super) fn screen(&self) -> String {
        self.terminal.backend().to_string()
    }

    /// Everything the user was told.
    pub(super) fn said(&self) -> Vec<String> {
        self.app
            .state()
            .notifications()
            .texts()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    pub(super) fn said_has(&self, text: &str) -> bool {
        self.said().iter().any(|told| told.contains(text))
    }

    pub(super) fn read(&self, name: &str) -> String {
        fs::read_to_string(self.root.join(name)).unwrap_or_default()
    }

    /// What the screen of the tab in front shows (the notifications do not cover it).
    pub(super) fn tab_text(&self) -> String {
        self.app
            .state()
            .terminal()
            .session()
            .map(|session| session.with_screen(|screen| screen.contents()))
            .unwrap_or_default()
    }

    pub(super) fn tabs(&self) -> usize {
        self.app.state().terminal().tab_count()
    }

    pub(super) fn starts(&self) -> usize {
        self.read("starts.txt").lines().count()
    }
}

#[test]
fn run_builds_the_active_target_and_starts_the_program_in_a_tab_of_its_own() {
    let mut rig = rig(Setup::default());

    rig.run();
    rig.wait_for_text("hello from the program");

    assert_eq!(rig.tabs(), 1);
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
    // The program is told what `cargo run` would tell it.
    let root = rig.root.display().to_string();
    rig.wait_for_text(&format!("manifest={root} pkg=app"));
    let args = rig.read("build-args.txt");
    assert!(
        args.starts_with("build --message-format=json-render-diagnostics"),
        "{args}"
    );
    assert!(args.contains("--bin app"), "{args}");
    assert!(
        args.contains(&format!("--manifest-path {root}/Cargo.toml")),
        "{args}"
    );
    assert_eq!(
        rig.app.active_run().map(|id| id.to_string()).as_deref(),
        Some("app:bin:app")
    );
    // The program ended: the tab says so and with what.
    rig.wait_for_text("[exited with code 0]");
    assert!(rig.said_has("app (bin) ended"), "{:?}", rig.said());
}

#[test]
fn what_cargo_says_while_it_builds_is_in_the_tab_and_the_program_replaces_it() {
    let mut rig = rig(Setup {
        build_says: "Compiling app v0.1.0 (the build is going)",
        build_seconds: "1",
        program: "echo the program",
        ..Setup::default()
    });

    rig.run();
    rig.wait_for_text("the build is going");
    assert!(
        rig.app.state().terminal().is_running(),
        "a build is running in the tab"
    );
    rig.wait_for_text("the program");

    assert!(
        !rig.screen().contains("the build is going"),
        "a fresh screen for the program"
    );
    assert_eq!(rig.tabs(), 1);
}

#[test]
fn a_build_that_fails_shows_cargos_errors_and_starts_nothing() {
    let mut rig = rig(Setup {
        build_says: "error[E0425]: cannot find value 'x'",
        build_code: 101,
        ..Setup::default()
    });

    rig.run();
    rig.wait_for_text("[the build failed: cargo ended with code 101]");

    assert!(rig.screen().contains("cannot find value 'x'"));
    assert_eq!(rig.starts(), 0, "the program was not started");
    rig.wait_for("the notice", |rig| rig.said_has("not started"));
    assert!(!rig.app.state().terminal().is_running(), "the tab is over");
}

#[test]
fn a_second_run_reuses_the_tab_and_starts_a_fresh_screen() {
    let mut rig = rig(Setup::default());
    rig.run();
    rig.wait_for_text("[exited with code 0]");

    rig.run();
    rig.wait_for("the second start", |rig| rig.starts() == 2);
    rig.wait_for_text("[exited with code 0]");

    assert_eq!(rig.tabs(), 1, "the same tab");
    assert_eq!(
        rig.screen().matches("hello from the program").count(),
        1,
        "{}",
        rig.screen()
    );
}

#[test]
fn a_run_asks_before_it_stops_a_program_that_still_runs_and_no_leaves_it() {
    let mut rig = rig(Setup {
        program: "echo up; sleep 30",
        ..Setup::default()
    });
    rig.run();
    rig.wait_for_text("up");

    rig.run();

    assert!(matches!(rig.app.state().popup(), Popup::Confirm(_)));
    let question = rig.app.state().confirm().unwrap().text();
    assert!(
        question.contains("app (bin) is still running"),
        "{question}"
    );
    rig.press(KeyCode::Char('n'));
    assert!(matches!(rig.app.state().popup(), Popup::None));
    assert_eq!(rig.starts(), 1, "nothing was started again");
    assert!(
        rig.app.state().terminal().is_running(),
        "the program goes on"
    );
    assert_eq!(rig.tabs(), 1);
}

#[test]
fn yes_stops_the_program_and_runs_it_again_in_the_same_tab() {
    let mut rig = rig(Setup {
        program: "echo up; sleep 30",
        ..Setup::default()
    });
    rig.run();
    rig.wait_for_text("up");

    rig.run();
    rig.press(KeyCode::Char('y'));
    rig.wait_for("the second start", |rig| rig.starts() == 2);

    assert_eq!(rig.tabs(), 1);
    assert!(rig.app.state().terminal().is_running());
}

#[test]
fn a_run_while_the_build_goes_asks_too_and_the_old_build_is_not_followed() {
    let mut rig = rig(Setup {
        build_seconds: "2",
        program: "echo the program; sleep 30",
        ..Setup::default()
    });
    rig.run();
    rig.wait_for_text("Compiling app");

    rig.run();
    assert!(matches!(rig.app.state().popup(), Popup::Confirm(_)));
    rig.press(KeyCode::Char('y'));
    rig.wait_for_text("the program");
    // Time enough for the build that was given up to have been heard from.
    rig.wait_for("a quiet moment", {
        let until = Instant::now() + Duration::from_millis(2500);
        move |_| Instant::now() > until
    });

    assert_eq!(
        rig.starts(),
        1,
        "only the second build's program was started"
    );
    assert_eq!(rig.tabs(), 1);
}

#[test]
fn modified_files_are_saved_before_the_build_and_what_was_saved_is_said() {
    let mut rig = rig(Setup::default());
    rig.app
        .state_mut()
        .workspace_mut()
        .open_path(&rig.root.join("a.txt"))
        .unwrap();
    rig.app.state_mut().edit(|editor| {
        let _ = editor.insert_text("edited ");
    });
    assert!(!rig.app.state().workspace().dirty_documents().is_empty());

    rig.run();
    rig.wait_for_text("hello from the program");

    assert!(
        rig.read("seen-by-build.txt").starts_with("edited alpha"),
        "the build saw the saved text"
    );
    assert!(rig.said_has("saved 1 file"), "{:?}", rig.said());
}

#[test]
fn an_untitled_buffer_is_left_alone_and_the_run_goes_on() {
    let mut rig = rig(Setup::default());
    rig.app.state_mut().workspace_mut().new_file();
    rig.app.state_mut().edit(|editor| {
        let _ = editor.insert_text("scratch");
    });

    rig.run();
    rig.wait_for_text("hello from the program");

    assert!(rig.said_has("not saved: it has no file yet") || rig.said_has("nothing to save"));
}

#[test]
fn a_file_that_cannot_be_saved_stops_the_run() {
    let mut rig = rig(Setup::default());
    rig.app
        .state_mut()
        .workspace_mut()
        .open_path(&rig.root.join("a.txt"))
        .unwrap();
    rig.app.state_mut().edit(|editor| {
        let _ = editor.insert_text("mine ");
    });
    // The file changes on disk after it was read: the save is refused.
    std::thread::sleep(Duration::from_millis(1100));
    fs::write(rig.root.join("a.txt"), "theirs, and longer than before\n").unwrap();

    rig.run();
    rig.wait_for("the stop", |rig| {
        rig.said_has("not running: some files could not be saved")
    });

    assert_eq!(rig.tabs(), 0, "nothing was started");
    assert_eq!(rig.read("build-args.txt"), "", "no build");
}

#[test]
fn a_project_with_nothing_to_run_says_so() {
    let mut rig = rig(Setup {
        has_target: false,
        ..Setup::default()
    });

    rig.run();
    rig.wait_for("the message", |rig| rig.said_has("nothing to run"));

    assert_eq!(rig.tabs(), 0);
}

#[test]
fn a_command_of_the_file_starts_at_once_with_its_variables_and_folder() {
    let mut rig = rig(Setup {
        has_target: false,
        launch_file: Some(
            r#"
[[run]]
name = "Hello"
command = "sh"
args = ["-c", "echo [$FOO] in $(pwd)"]
env = { FOO = "bar" }
cwd = "sub"
"#,
        ),
        ..Setup::default()
    });
    fs::create_dir(rig.root.join("sub")).unwrap();

    rig.run();
    let wanted = format!("[bar] in {}/sub", rig.root.display());
    rig.wait_for_text(&wanted);

    assert_eq!(rig.read("build-args.txt"), "", "a command is not built");
    assert_eq!(rig.tabs(), 1);
}

#[test]
fn what_is_wrong_in_the_file_is_said_and_the_rest_still_runs() {
    let mut rig = rig(Setup {
        launch_file: Some("[[run]]\nname = \"no command\"\n"),
        ..Setup::default()
    });

    rig.run();
    rig.wait_for_text("hello from the program");

    assert!(
        rig.said_has("entry 1: an entry needs a `target`"),
        "{:?}",
        rig.said()
    );
}

#[test]
fn an_override_of_the_target_changes_its_arguments_and_variables() {
    let mut rig = rig(Setup {
        program: "echo \"args: $*\" \"mode=$MODE\"",
        launch_file: Some(
            "[[run]]\ntarget = \"app:bin:app\"\nargs = [\"--fast\", \"two words\"]\nenv = { MODE = \"debug\" }\n",
        ),
        ..Setup::default()
    });

    rig.run();

    rig.wait_for_text("args: --fast two words mode=debug");
}

#[test]
fn without_cargo_a_command_of_the_file_still_runs() {
    let mut rig = rig(Setup {
        cargo: Some("trcode-no-such-cargo"),
        launch_file: Some(
            "[[run]]\nname = \"Cmd\"\ncommand = \"echo\"\nargs = [\"still works\"]\n",
        ),
        ..Setup::default()
    });

    rig.run();

    rig.wait_for_text("still works");
    assert_eq!(rig.tabs(), 1);
}

#[test]
fn without_cargo_and_without_commands_it_says_why_there_is_nothing_to_run() {
    let mut rig = rig(Setup {
        cargo: Some("trcode-no-such-cargo"),
        ..Setup::default()
    });

    rig.run();

    rig.wait_for("the reason", |rig| rig.said_has("cargo is not installed"));
    assert_eq!(rig.tabs(), 0);
}

#[test]
fn the_real_cargo_builds_a_real_project_and_its_program_runs_in_the_tab() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"hello-run\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        root.join("src/main.rs"),
        "fn main() {\n    println!(\"hello from {} with {:?}\", env!(\"CARGO_PKG_NAME\"), std::env::args().count());\n}\n",
    )
    .unwrap();
    let mut state = AppState::default();
    state.open_project(&root).unwrap();
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state).with_events(tx);
    let mut rig = Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(110, 30)).unwrap(),
        _dir: dir,
        config: tempfile::tempdir().unwrap(),
        root,
    };
    rig.draw();

    rig.run();

    rig.wait_for_text("hello from hello-run with 1");
    rig.wait_for_text("[exited with code 0]");
    assert_eq!(rig.tabs(), 1);
}

// --- the picker, what is remembered, trust, and the list kept up to date ---

const TWO_TARGETS_AND_A_COMMAND: &str =
    "[[run]]\nname = \"Say hi\"\ncommand = \"echo\"\nargs = [\"hi from the command\"]\n";

impl Rig {
    fn choose(&mut self) {
        self.app.run_action(Action::ChooseRun);
    }

    fn picker_rows(&self) -> Vec<String> {
        self.app
            .state()
            .run_picker()
            .map(|list| {
                list.entries()
                    .iter()
                    .map(|entry| format!("{}|{}", entry.title, entry.kind))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn wait_for_picker(&mut self) {
        self.wait_for("the picker", |rig| rig.app.state().run_picker().is_some());
    }

    fn saved_file(&self) -> String {
        fs::read_to_string(self.config.path().join("state.json")).unwrap_or_default()
    }
}

#[test]
fn the_picker_lists_the_targets_the_commands_and_the_ones_that_are_gone() {
    let mut rig = rig(Setup {
        launch_file: Some(
            "[[run]]\nname = \"Say hi\"\ncommand = \"echo\"\n[[run]]\ntarget = \"old:bin:old\"\n",
        ),
        ..Setup::default()
    });

    rig.choose();
    rig.wait_for_picker();

    assert_eq!(
        rig.picker_rows(),
        ["● app (bin)|app", "  Say hi|command", "  old (bin)|old"],
        "the active one is marked"
    );
    rig.draw();
    let screen = rig.screen();
    assert!(screen.contains("Run configurations (3)"), "{screen}");
    assert!(screen.contains("(disabled: target not found)"), "{screen}");
}

#[test]
fn choosing_a_row_makes_it_the_active_one_runs_it_and_remembers_it_for_the_project() {
    let mut rig = rig(Setup {
        launch_file: Some(TWO_TARGETS_AND_A_COMMAND),
        ..Setup::default()
    });
    rig.choose();
    rig.wait_for_picker();

    rig.press(KeyCode::Down);
    rig.press(KeyCode::Enter);
    rig.wait_for_text("hi from the command");

    assert!(
        rig.app.state().run_picker().is_none(),
        "the picker is closed"
    );
    assert_eq!(rig.app.active_run().unwrap().to_string(), "command:Say hi");
    assert!(
        rig.saved_file().contains("command:Say hi"),
        "{}",
        rig.saved_file()
    );
    // The key now starts that one.
    rig.run();
    rig.wait_for_text("hi from the command");
    assert_eq!(rig.tabs(), 1);
}

#[test]
fn a_number_picks_that_row() {
    let mut rig = rig(Setup {
        launch_file: Some(TWO_TARGETS_AND_A_COMMAND),
        ..Setup::default()
    });
    rig.choose();
    rig.wait_for_picker();

    rig.press(KeyCode::Char('2'));

    rig.wait_for_text("hi from the command");
}

#[test]
fn escape_closes_the_picker_and_changes_nothing() {
    let mut rig = rig(Setup {
        launch_file: Some(TWO_TARGETS_AND_A_COMMAND),
        ..Setup::default()
    });
    rig.choose();
    rig.wait_for_picker();
    rig.press(KeyCode::Down);

    rig.press(KeyCode::Esc);

    assert!(rig.app.state().run_picker().is_none());
    assert_eq!(rig.tabs(), 0);
    assert!(
        !rig.saved_file().contains("active_runs"),
        "nothing is remembered: {}",
        rig.saved_file()
    );
}

#[test]
fn a_row_whose_target_is_gone_says_so_and_the_picker_stays_open() {
    let mut rig = rig(Setup {
        launch_file: Some("[[run]]\ntarget = \"old:bin:old\"\n"),
        ..Setup::default()
    });
    rig.choose();
    rig.wait_for_picker();
    rig.press(KeyCode::Down);

    rig.press(KeyCode::Enter);

    assert!(
        rig.said_has("old (bin) cannot be run: target not found"),
        "{:?}",
        rig.said()
    );
    assert!(rig.app.state().run_picker().is_some());
    assert_eq!(rig.tabs(), 0);
}

#[test]
fn the_choice_is_what_the_next_start_of_the_editor_runs() {
    let mut rig = rig(Setup {
        launch_file: Some(TWO_TARGETS_AND_A_COMMAND),
        ..Setup::default()
    });
    rig.choose();
    rig.wait_for_picker();
    rig.press(KeyCode::Char('2'));
    rig.wait_for_text("hi from the command");
    // The editor starts again: what is remembered is in state.json.
    let saved = crate::persist::saved_state::load(Some(rig.config.path())).0;
    let cargo = rig.root.join("cargo.sh").display().to_string();
    let (app, events) = app_in(&rig.root, &rig.config, saved, &cargo);
    rig.app = app;
    rig.events = events;
    rig.draw();

    rig.run();

    rig.wait_for_text("hi from the command");
    assert_eq!(
        rig.read("build-args.txt"),
        "",
        "the command, not the target, ran"
    );
}

#[test]
fn the_status_bar_names_the_active_configuration() {
    let mut rig = rig(Setup::default());
    rig.app
        .handle_event(Event::FilesChanged(vec![rig.root.join("Cargo.toml")]));

    rig.wait_for_text("▶ app (bin)");

    rig.app.state_mut().workspace_mut().new_file();
    rig.draw();
    assert!(
        rig.screen().contains("▶ app (bin)"),
        "also with a file open"
    );
}

#[test]
fn the_list_is_worked_out_again_when_the_launch_file_changes() {
    let mut rig = rig(Setup::default());
    rig.app
        .handle_event(Event::FilesChanged(vec![rig.root.join("Cargo.toml")]));
    rig.wait_for_text("▶ app (bin)");
    fs::create_dir_all(rig.root.join(".trcode")).unwrap();
    let file = rig.root.join(".trcode/launch.toml");
    fs::write(&file, TWO_TARGETS_AND_A_COMMAND).unwrap();

    rig.app.handle_event(Event::FilesChanged(vec![file]));
    rig.wait_for("the new list", |rig| rig.app.run_list_len() == Some(2));
    rig.choose();

    assert_eq!(rig.picker_rows().len(), 2, "{:?}", rig.picker_rows());
}

#[test]
fn a_file_that_the_list_is_not_made_of_does_not_work_it_out_again() {
    let mut rig = rig(Setup::default());
    rig.app
        .handle_event(Event::FilesChanged(vec![rig.root.join("Cargo.toml")]));
    rig.wait_for_text("▶ app (bin)");
    // Out of date on purpose: if the list were worked out again it would see this.
    fs::write(rig.root.join("metadata.json"), "{\"packages\": []}").unwrap();

    rig.app
        .handle_event(Event::FilesChanged(vec![rig.root.join("a.txt")]));
    rig.choose();
    rig.wait_for_picker();

    assert_eq!(rig.picker_rows().len(), 1, "the list was kept");
}

#[test]
fn a_manifest_that_changed_gets_the_list_worked_out_again() {
    let mut rig = rig(Setup::default());
    rig.app
        .handle_event(Event::FilesChanged(vec![rig.root.join("Cargo.toml")]));
    rig.wait_for_text("▶ app (bin)");
    fs::write(rig.root.join("metadata.json"), "{\"packages\": []}").unwrap();

    rig.app
        .handle_event(Event::FilesChanged(vec![rig.root.join("Cargo.toml")]));

    rig.wait_for("the list to be worked out again", |rig| {
        !rig.screen().contains("▶ app (bin)")
    });
}

#[test]
fn a_command_of_a_folder_that_is_not_trusted_asks_first_and_no_runs_nothing() {
    let mut rig = rig(Setup {
        trusted: false,
        launch_file: Some(TWO_TARGETS_AND_A_COMMAND),
        ..Setup::default()
    });
    rig.choose();
    rig.wait_for_picker();
    rig.press(KeyCode::Char('2'));

    assert!(matches!(rig.app.state().popup(), Popup::Confirm(_)));
    let question = rig.app.state().confirm().unwrap().text();
    assert!(question.contains("Say hi"), "{question}");
    assert!(question.contains(".trcode/launch.toml"), "{question}");
    rig.press(KeyCode::Char('n'));

    assert_eq!(rig.tabs(), 0);
    assert!(
        !rig.saved_file().contains("trusted_folders"),
        "{}",
        rig.saved_file()
    );
}

#[test]
fn yes_trusts_the_folder_remembers_it_and_runs_what_waited() {
    let mut rig = rig(Setup {
        trusted: false,
        launch_file: Some(TWO_TARGETS_AND_A_COMMAND),
        ..Setup::default()
    });
    rig.choose();
    rig.wait_for_picker();
    rig.press(KeyCode::Char('2'));

    rig.press(KeyCode::Char('y'));
    rig.wait_for_text("hi from the command");

    assert!(
        rig.saved_file().contains("trusted_folders"),
        "{}",
        rig.saved_file()
    );
    // Asked once: the next run of a command goes on.
    rig.run();
    assert!(!matches!(rig.app.state().popup(), Popup::Confirm(_)));
}

#[test]
fn a_target_as_cargo_found_it_runs_in_a_folder_that_is_not_trusted() {
    let mut rig = rig(Setup {
        trusted: false,
        ..Setup::default()
    });

    rig.run();
    rig.wait_for_text("hello from the program");

    assert!(matches!(rig.app.state().popup(), Popup::None));
}

#[test]
fn a_target_the_file_changes_asks_in_a_folder_that_is_not_trusted() {
    let mut rig = rig(Setup {
        trusted: false,
        launch_file: Some("[[run]]\ntarget = \"app:bin:app\"\nenv = { LD_PRELOAD = \"x\" }\n"),
        ..Setup::default()
    });

    rig.run();
    rig.wait_for("the question", |rig| {
        matches!(rig.app.state().popup(), Popup::Confirm(_))
    });

    assert_eq!(rig.read("build-args.txt"), "", "nothing was built yet");
}
