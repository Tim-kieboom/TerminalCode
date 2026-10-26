//! The test pane and its adapters, as the modules built from their own repository
//! (<https://github.com/Tim-kieboom/trcode-plugin-test>), run by the editor: the real
//! runtime, threads, budgets, plugin calls, `process.run` and trust. What that repository's
//! own tests can only model (a trap, a budget that stops a loop, a thread of a plugin's own,
//! a command that runs) is tried here. They need a checkout of it with the modules built, so
//! they are ignored unless asked for:
//!
//! ```text
//! (in the plugin's repository) cargo build --target wasm32-unknown-unknown --release
//! TRCODE_PLUGIN_TEST_DIR=/path/to/trcode-plugin-test cargo test -p terminal_code real_test_plugin -- --ignored
//! ```
//!
//! The one with the Rust adapter also needs `cargo-nextest`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::state::{AppState, Popup};
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::event::Event;
use crate::event::action::Action;
use crate::persist::saved_state::{PluginRecord, SavedState};
use crate::plugin_api::ToPlugin;
use crate::plugins::manifest::{Host, Installed, read};
use crate::plugins::{Notice, Runtime};
use crate::ui::PluginViewId;
use crate::ui::layout::LayoutTree;
use crate::ui::view::ViewNode;

/// The checkout named by `TRCODE_PLUGIN_TEST_DIR`.
fn repo() -> PathBuf {
    std::env::var_os("TRCODE_PLUGIN_TEST_DIR")
        .map(PathBuf::from)
        .expect(
            "set TRCODE_PLUGIN_TEST_DIR to a checkout of trcode-plugin-test with the modules built",
        )
}

/// Puts the plugin made by the crate `krate` (its folder in `crates/`, and the name of its
/// module) in the plugins folder `plugins` as its manifest says: a folder of its id with the
/// manifest and the module.
fn install(plugins: &Path, folder: &str, wasm_crate: &str) {
    let manifest_path = repo().join("crates").join(folder).join("manifest.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("{}: {error}", manifest_path.display()));
    let value: toml::Table = manifest.parse().unwrap();
    let id = value["id"].as_str().unwrap();
    let module_name = value["wasm"].as_str().unwrap();
    let module_path = repo()
        .join("target/wasm32-unknown-unknown/release")
        .join(format!("{wasm_crate}.wasm"));
    let module = fs::read(&module_path).unwrap_or_else(|error| {
        panic!(
            "{}: {error}\nbuild it there with: cargo build --target wasm32-unknown-unknown --release",
            module_path.display()
        )
    });
    let dir = plugins.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("manifest.toml"), manifest).unwrap();
    fs::write(dir.join(module_name), module).unwrap();
}

/// What a view says, line by line: of its rows or lines, or of the first of them in a column (the
/// buttons are not lines).
fn lines_of(node: &ViewNode) -> Vec<String> {
    match node {
        ViewNode::Lines(lines) => lines.iter().map(|line| line.text.to_string()).collect(),
        ViewNode::Rows { rows, .. } => rows
            .iter()
            .map(|row| row.spans.iter().map(|span| span.text.as_ref()).collect())
            .collect(),
        ViewNode::Column(parts) => parts
            .iter()
            .map(lines_of)
            .find(|lines| !lines.is_empty())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The labels of the buttons of a view, row by row.
fn buttons_of(node: &ViewNode) -> Vec<Vec<String>> {
    let ViewNode::Column(parts) = node else {
        return Vec::new();
    };
    parts
        .iter()
        .filter_map(|part| match part {
            ViewNode::Buttons(buttons) => Some(
                buttons
                    .iter()
                    .map(|button| button.label.to_string())
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

// --- the runtime alone: the fake adapter and the pane ---

/// The runtime with the plugins loaded as their manifests say, and what it has said.
struct Real {
    runtime: Runtime,
    notices: mpsc::Receiver<Notice>,
    /// What each plugin last drew.
    views: HashMap<String, Vec<String>>,
    /// The buttons each plugin last drew, row by row.
    buttons: HashMap<String, Vec<Vec<String>>>,
    /// The plugins that were started again after a trap, in order.
    restarts: Vec<String>,
    plugins: tempfile::TempDir,
}

fn real() -> Real {
    let (sender, notices) = mpsc::channel();
    let runtime = Runtime::spawn(move |notice| {
        let _ = sender.send(notice);
    });
    Real {
        runtime,
        notices,
        views: HashMap::new(),
        buttons: HashMap::new(),
        restarts: Vec::new(),
        plugins: tempfile::tempdir().unwrap(),
    }
}

fn load(runtime: &Runtime, installed: &Installed) {
    runtime.load_with(
        installed.id(),
        installed.module().unwrap(),
        installed.capabilities.iter().copied(),
        installed.manifest.thread,
        installed.run.clone(),
    );
}

impl Real {
    /// Puts a plugin in the plugins folder and starts it as its manifest says, as the editor
    /// does when the user has turned it on.
    fn start(&self, folder: &str, wasm_crate: &str) {
        install(self.plugins.path(), folder, wasm_crate);
        let id = fs::read_to_string(repo().join("crates").join(folder).join("manifest.toml"))
            .unwrap()
            .parse::<toml::Table>()
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let installed = read(&self.plugins.path().join(id), Host::THIS)
            .unwrap_or_else(|error| panic!("{folder}: {error}"));
        load(&self.runtime, &installed);
    }

    fn absorb(&mut self, notice: Notice) {
        match notice {
            Notice::View { plugin, content } => {
                self.buttons
                    .insert(plugin.to_string(), buttons_of(&content));
                self.views.insert(plugin.to_string(), lines_of(&content));
            }
            Notice::Restarted { plugin, .. } => self.restarts.push(plugin.to_string()),
            Notice::Disabled { plugin, reason } | Notice::Broken { plugin, reason } => {
                panic!("{plugin} is off: {reason}")
            }
            _ => {}
        }
    }

    /// Takes what the runtime says until `done`, which is looked at after each thing.
    fn until(&mut self, what: &str, within: Duration, done: impl Fn(&Real) -> bool) {
        let deadline = Instant::now() + within;
        while !done(self) {
            let left = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_else(|| panic!("timed out waiting for {what}\n{:#?}", self.views));
            if let Ok(notice) = self
                .notices
                .recv_timeout(left.min(Duration::from_millis(50)))
            {
                self.absorb(notice);
            }
        }
    }

    /// The first line of the pane: the adapter, the counts, the run and what went wrong.
    fn header(&self) -> String {
        self.views
            .get("test-ui")
            .and_then(|lines| lines.first().cloned())
            .unwrap_or_default()
    }

    fn lines(&self, plugin: &str) -> &[String] {
        self.views.get(plugin).map_or(&[], Vec::as_slice)
    }

    fn press(&self, action: &str) {
        self.runtime.send("test-ui", ToPlugin::action(action));
    }

    /// A call to the fake adapter from nobody in particular, which is answered to nobody.
    fn call_adapter(&self, method: &str, payload: &str) {
        self.runtime.send(
            "fake-adapter",
            ToPlugin::Call {
                from: "harness".into(),
                request: 0,
                method: method.into(),
                payload: payload.into(),
            },
        );
    }

    /// Lets what is on its way happen. The pane draws "run 1" as it asks the adapter for the
    /// run, a moment before the adapter has the call, and a turn of the adapter's clock that
    /// comes first would find nothing to turn.
    fn settle(&mut self) {
        let until = Instant::now() + Duration::from_millis(300);
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            if let Ok(notice) = self.notices.recv_timeout(left) {
                self.absorb(notice);
            }
        }
    }

    fn wait_for_header(&mut self, text: &str) {
        let text = text.to_owned();
        self.until(
            &format!("the header to say {text}"),
            Duration::from_secs(60),
            |real| real.header().contains(&text),
        );
    }
}

/// A fake adapter told to be like `scenario`, and then a pane, which asks it for the tests as it
/// starts and has `tests` of them.
fn with_scenario(scenario: &str, tests: usize) -> Real {
    let mut real = real();
    real.start("fake-adapter", "trcode_test_fake_adapter");
    real.call_adapter("configure", scenario);
    real.start("test-ui", "trcode_test_ui");
    // The pane starts with the Rust adapter, which is not here, and never falls back to the
    // fake one: it is chosen.
    real.runtime.send("test-ui", ToPlugin::action("use_fake"));
    real.wait_for_header(&format!("tests {tests}/{tests}"));
    real
}

#[test]
#[ignore = "needs the modules built"]
fn size_twenty_thousand_tests_are_pulled_through_the_real_calls_within_the_budget() {
    let began = Instant::now();
    let mut real = with_scenario(r#"{"tests":20000}"#, 20_000);

    println!("20000 tests discovered in {:?}", began.elapsed());
    assert!(
        real.restarts.is_empty(),
        "a plugin trapped: {:?}",
        real.restarts
    );
    // A project this big starts with its folders closed; opened, it is a window.
    assert_eq!(real.lines("test-ui")[1..], ["> [  ] bin (0/20000)"]);
    real.press("right");
    real.until("the folder to open", Duration::from_secs(5), |real| {
        real.lines("test-ui").len() == 19
    });
}

#[test]
#[ignore = "needs the modules built"]
fn ids_tests_with_the_same_name_are_rows_and_one_runs_alone() {
    let mut real = with_scenario(r#"{"tests":4,"same_names":true}"#, 4);
    assert_eq!(
        real.lines("test-ui")[1..],
        [
            "v [  ] bin0 (0/2)",
            "  [  ] case_0",
            "  [  ] case_1",
            "v [  ] bin1 (0/2)",
            "  [  ] case_0",
            "  [  ] case_1",
        ]
    );

    // The case_0 of bin1: the folder, two tests, the next folder, then it.
    for _ in 0..4 {
        real.press("down");
    }
    real.press("run_selected");
    real.wait_for_header("run 1");
    real.settle();
    real.call_adapter("advance", r#"{"n":10}"#);

    real.wait_for_header("ok 1 FAIL 0");
    let lines = real.lines("test-ui");
    assert_eq!(lines[5], "  [ok] case_0", "{lines:?}");
    assert_eq!(
        lines[2], "  [  ] case_0",
        "the one of the other folder did not run"
    );
}

#[test]
#[ignore = "needs the modules built"]
fn the_find_tests_and_run_buttons_work_through_the_real_calls() {
    let mut real = with_scenario(r#"{"tests":4}"#, 4);
    assert_eq!(real.buttons["test-ui"], [["Find tests", "Run v"]]);

    // The Run button opens its list; Run all in it runs the four tests and closes the list.
    let press = |real: &Real, button: &str| {
        real.runtime.send(
            "test-ui",
            ToPlugin::Press {
                button: button.to_owned(),
            },
        );
    };
    press(&real, "run_menu");
    real.until("the list to open", Duration::from_secs(5), |real| {
        real.buttons["test-ui"].len() == 2
    });
    assert_eq!(
        real.buttons["test-ui"][1],
        ["Run all", "Run selected", "Cancel run"]
    );
    press(&real, "run_all");
    real.wait_for_header("run 1");
    real.settle();
    real.call_adapter("advance", r#"{"n":10}"#);
    real.wait_for_header("ok 3 FAIL 1");
    assert_eq!(real.buttons["test-ui"].len(), 1, "the list closed");

    // Find tests asks the adapter again: the same four tests, with no results.
    press(&real, "find");
    real.wait_for_header("tests 4/4 | ok 0 FAIL 0");
}

#[test]
#[ignore = "needs the modules built"]
fn find_tests_again_with_thousands_of_tests_known_does_not_run_the_pane_out_of_fuel() {
    // The second time the tests are asked for, everything the first answer built is freed in one
    // message: three thousand tests as three pieces of memory each was nearly twice the budget.
    let mut real = with_scenario(r#"{"tests":3000}"#, 3_000);
    let press = |real: &Real, button: &str| {
        real.runtime.send(
            "test-ui",
            ToPlugin::Press {
                button: button.to_owned(),
            },
        );
    };

    for _ in 0..3 {
        press(&real, "find");
        real.until("the list to start again", Duration::from_secs(30), |real| {
            real.header().contains("tests 0/0") || real.header().contains("ok 0 FAIL 0")
        });
        real.wait_for_header("tests 3000/3000");
    }

    assert!(
        real.restarts.is_empty(),
        "a plugin ran out of fuel: {:?}",
        real.restarts
    );
}

#[test]
#[ignore = "needs the modules built"]
fn cancel_then_an_immediate_re_run_shows_only_the_second_runs_results() {
    // An adapter that keeps telling a run after it was cancelled.
    let mut real = with_scenario(r#"{"tests":10,"talks_after_cancel":true}"#, 10);
    real.press("run_all");
    real.wait_for_header("run 1");
    real.settle();
    real.call_adapter("advance", r#"{"n":2}"#);
    real.wait_for_header("ok 2");

    real.press("cancel");
    real.press("run_all");
    real.wait_for_header("run 2");
    real.settle();
    real.call_adapter("advance", r#"{"n":3}"#);

    // The second run's three, and nothing of the first (which the adapter told again).
    real.wait_for_header("ok 3 FAIL 0");
    let settle = Instant::now() + Duration::from_millis(500);
    while Instant::now() < settle {
        if let Ok(notice) = real.notices.recv_timeout(Duration::from_millis(50)) {
            real.absorb(notice);
        }
    }
    assert!(real.header().contains("| ok 3 FAIL 0"), "{}", real.header());
    assert!(real.header().contains("run 2"), "{}", real.header());
}

#[test]
#[ignore = "needs the modules built"]
fn death_an_adapter_that_traps_mid_run_makes_the_running_tests_unknown_and_the_next_run_works() {
    let mut real = with_scenario(r#"{"tests":10,"traps_on_advance":true}"#, 10);
    real.press("run_all");
    real.wait_for_header("run 1");
    real.settle();

    real.call_adapter("advance", r#"{"n":1}"#);

    real.wait_for_header("the adapter went away");
    assert_eq!(real.restarts, ["fake-adapter"]);
    assert!(real.header().contains("? 10"), "{}", real.header());
    assert!(
        !real.header().contains("run 1"),
        "no run is going: {}",
        real.header()
    );
    // The adapter is a fresh one (six tests, as it starts): one of them runs.
    real.press("run_selected");
    real.wait_for_header("run 2");
    real.settle();
    real.call_adapter("advance", r#"{"n":1}"#);
    real.wait_for_header("ok 1");
}

#[test]
#[ignore = "needs the modules built"]
fn stalls_a_runaway_adapter_is_stopped_by_the_budget_and_started_again() {
    let mut real = with_scenario(r#"{"tests":6}"#, 6);
    // From now on every call counts to a billion first, which takes far more than the budget
    // of a message this small (the call that says so is answered before it counts).
    real.call_adapter("configure", r#"{"tests":6,"burns":1000000000}"#);
    let began = Instant::now();

    real.call_adapter("advance", "{}");

    real.until(
        "the adapter to be stopped",
        Duration::from_secs(60),
        |real| !real.restarts.is_empty(),
    );
    assert_eq!(real.restarts, ["fake-adapter"]);
    println!("stopped after {:?}", began.elapsed());
}

#[test]
#[ignore = "needs the modules built"]
fn stalls_an_adapter_that_is_busy_on_its_own_thread_does_not_freeze_the_pane() {
    let mut real = with_scenario(r#"{"tests":6}"#, 6);
    real.call_adapter("configure", r#"{"tests":6,"burns":3000000}"#);
    // A call with a payload big enough to be given the budget for the work (so it is allowed
    // to take seconds), which the pane then waits for.
    let padding = "x".repeat(120_000);
    let busy = Instant::now();
    // It counts first, then takes the scenario that says to count no more (so the pane's own
    // call, which has no payload to speak of, does not have to count too).
    real.call_adapter(
        "configure",
        &format!(r#"{{"tests":6,"burns":0,"pad":"{padding}"}}"#),
    );

    // The pane answers the editor while the adapter works: it has its tests (six rows), and a
    // pane of five lines shows the header and four of them.
    real.runtime.send(
        "test-ui",
        ToPlugin::Size {
            width: 80,
            height: 5,
        },
    );
    let asked = Instant::now();
    real.until("the pane to redraw", Duration::from_secs(5), |real| {
        real.lines("test-ui").len() == 4
    });
    let answered = asked.elapsed();
    println!("the pane answered in {answered:?}");
    assert!(answered < Duration::from_millis(1_000), "{answered:?}");
    // It asks for the list again, which the adapter can only answer when it is done.
    real.press("discover");
    real.settle();
    assert!(
        real.header().contains("tests 0/0"),
        "the adapter is still busy: {}",
        real.header()
    );
    assert!(
        real.restarts.is_empty(),
        "the adapter had the budget for the work: {:?}",
        real.restarts
    );
    // And it gets its answer when the adapter is done.
    real.wait_for_header("tests 6/6");
    println!("the adapter was busy for {:?}", busy.elapsed());
    assert!(
        busy.elapsed() > Duration::from_millis(1_500),
        "the premise: the adapter really was busy ({:?})",
        busy.elapsed()
    );
}

// --- the whole editor: the pane, the Rust adapter, trust and a real nextest ---

fn nextest_is_installed() -> bool {
    Command::new("cargo")
        .args(["nextest", "--version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Copies the project of the adapter's tests, without what was built there.
fn copy_project(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == "target" {
            continue;
        }
        let target = to.join(&name);
        match entry.file_type().unwrap().is_dir() {
            true => {
                fs::create_dir_all(&target).unwrap();
                copy_project(&entry.path(), &target);
            }
            false => {
                fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
}

struct Rig {
    app: App,
    events: tokio::sync::mpsc::Receiver<Event>,
    _dirs: (tempfile::TempDir, tempfile::TempDir),
}

/// The editor with a copy of the adapter's test project open, and the pane and the Rust adapter
/// turned on as the user would have (what each asks for is what they were shown).
fn rust_rig() -> Rig {
    let project = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(project.path()).unwrap();
    copy_project(
        &repo().join("crates/rust-adapter/tests/fixture-project"),
        &root,
    );
    let config = tempfile::tempdir().unwrap();
    let plugins = config.path().join("plugins");
    install(&plugins, "rust-adapter", "trcode_test_rust_adapter");
    install(&plugins, "test-ui", "trcode_test_ui");
    let mut saved = SavedState::default();
    for id in ["rust-adapter", "test-ui"] {
        let installed = read(&plugins.join(id), Host::THIS).unwrap();
        saved.set_plugin(
            id,
            PluginRecord {
                enabled: true,
                capabilities: installed
                    .capabilities
                    .iter()
                    .map(|capability| capability.name().to_owned())
                    .collect(),
                run: installed
                    .run
                    .iter()
                    .map(|pattern| pattern.as_str().to_owned())
                    .collect(),
                own_thread: installed.manifest.thread == crate::plugins::Thread::Own,
                file_types: Vec::new(),
                place: None,
            },
        );
    }
    let mut state = AppState::new(Editor::new(Buffer::from_text("")));
    state.open_project(&root).unwrap();
    state.set_layout(
        LayoutTree::from_ron(
            r#"Col([Row([Pane(view: Editor), Fixed(80, Pane(view: Plugin("test-ui")))]), Fixed(10, Pane(view: Terminal, hidden: false))])"#,
        )
        .unwrap(),
    );
    let (tx, events) = tokio::sync::mpsc::channel(1024);
    let app = App::new(state)
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(saved)
        .with_events(tx);
    Rig {
        app,
        events,
        _dirs: (project, config),
    }
}

impl Rig {
    fn pump(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.app.handle_event(event);
        }
    }

    fn wait_until(&mut self, what: &str, within: Duration, done: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + within;
        loop {
            self.pump();
            if done(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}\n{:#?}",
                self.pane()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// What the pane shows, line by line.
    fn pane(&self) -> Vec<String> {
        self.app
            .state()
            .plugin_view(&PluginViewId::new("test-ui"))
            .map(|view| lines_of(view.content()))
            .unwrap_or_default()
    }

    fn header(&self) -> String {
        self.pane().first().cloned().unwrap_or_default()
    }

    fn wait_for_header(&mut self, text: &str) {
        let text = text.to_owned();
        self.wait_until(
            &format!("the header to say {text}"),
            Duration::from_secs(180),
            |rig| rig.header().contains(&text),
        );
    }

    fn press(&mut self, action: &str) {
        self.app
            .run_action(Action::Plugin(format!("test-ui:{action}").into()));
    }

    fn asking(&self) -> bool {
        matches!(self.app.state().popup(), Popup::Confirm(_))
    }

    fn question(&self) -> String {
        self.app
            .state()
            .confirm()
            .map(|confirm| confirm.text())
            .unwrap_or_default()
    }

    fn key(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }
}

#[test]
#[ignore = "needs the modules built and cargo-nextest"]
fn the_rust_adapter_lists_and_runs_a_real_project_in_the_editor_once_the_folder_is_trusted() {
    if !nextest_is_installed() {
        eprintln!("cargo-nextest is not installed: skipped");
        return;
    }
    let mut rig = rust_rig();
    // The pane starts with the Rust adapter, which lists the project: not allowed in a folder
    // that is not trusted, so the user is asked, and the pane says why there are no tests.
    rig.wait_until("the question", Duration::from_secs(60), |rig| rig.asking());
    let question = rig.question();
    assert!(question.contains("Trust the folder"), "{question}");
    assert!(question.contains("rust-adapter"), "{question}");
    assert!(question.contains("code of this project"), "{question}");
    rig.wait_for_header("not trusted");
    assert!(rig.header().contains("rust-adapter"), "{}", rig.header());

    // Yes: nextest lists the project, which the pane shows.
    rig.key(KeyCode::Char('y'));
    rig.press("discover");
    rig.wait_for_header("tests 5/5");
    // The tests are folders, a binary and its modules, with the tests in them.
    assert_eq!(
        rig.pane()[1..],
        [
            "v [  ] fx (0/3)",
            "  v [  ] tests (0/3)",
            "    [  ] fails_here",
            "    [  ] panics_ok",
            "    [  ] parses_empty",
            "v [  ] fx::it (0/1)",
            "  [  ] parses_empty",
            "v [  ] fx::bin/fx (0/1)",
            "  v [  ] tests (0/1)",
            "    [  ] parses_empty",
        ],
        "the ignored test is not listed"
    );

    // Run everything: four pass, one fails.
    rig.press("run_all");
    rig.wait_for_header("ok 4 FAIL 1");
    let rows = rig.pane();
    // The failure is marked on its test and on the folders above it.
    assert_eq!(rows[1], "v [FAIL] fx (3/3)", "{rows:?}");
    assert_eq!(rows[2], "  v [FAIL] tests (3/3)", "{rows:?}");
    assert_eq!(rows[3], "    [FAIL] fails_here", "{rows:?}");
    assert_eq!(rows[6], "v [ok] fx::it (1/1)", "{rows:?}");
    rig.wait_until("the run to end", Duration::from_secs(30), |rig| {
        !rig.header().contains("| run ")
    });

    // List again and run only the test of the binary whose name two others share. The pane
    // still shows the old list, with its results, until it has taken the action.
    let before = rig.pane();
    rig.press("discover");
    rig.wait_until("the list to start again", Duration::from_secs(30), |rig| {
        rig.pane() != before
    });
    rig.wait_for_header("tests 5/5");
    // The last test is the one of the binary `fx::bin/fx`: every node before it is a step down.
    let last = rig.pane().len() - 1;
    for _ in 1..last {
        rig.press("down");
    }
    rig.press("run_selected");
    rig.wait_for_header("ok 1 FAIL 0");
    let rows = rig.pane();
    assert_eq!(
        rows.iter()
            .filter(|row| row.trim_start().starts_with("[ok]"))
            .count(),
        1,
        "only that test ran: {rows:?}"
    );
    assert_eq!(rows[last], "    [ok] parses_empty", "{rows:?}");
}

impl Rig {
    /// Where the cursor of the editor is, as line and column from 1, and the file it is in.
    fn cursor(&self) -> (String, usize, usize) {
        let editor = self.app.state().editor();
        let head = editor.selections().primary().head();
        let name = editor
            .buffer()
            .path()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        (name, head.line + 1, head.column + 1)
    }
}

#[test]
#[ignore = "needs the modules built and cargo-nextest"]
fn a_test_is_gone_to_in_the_editor_by_its_panic_when_it_failed_and_by_grep_when_it_did_not() {
    if !nextest_is_installed() {
        eprintln!("cargo-nextest is not installed: skipped");
        return;
    }
    let mut rig = rust_rig();
    rig.wait_until("the question", Duration::from_secs(60), |rig| rig.asking());
    rig.key(KeyCode::Char('y'));
    rig.press("discover");
    rig.wait_for_header("tests 5/5");
    rig.press("run_all");
    rig.wait_for_header("ok 4 FAIL 1");
    rig.wait_until("the run to end", Duration::from_secs(30), |rig| {
        !rig.header().contains("| run ")
    });

    // The folder is selected first, then `tests`, then `fails_here`.
    rig.press("down");
    rig.press("down");
    // What it wrote is under the tests, and the place it panicked is in its title.
    rig.wait_until("what it wrote", Duration::from_secs(10), |rig| {
        rig.pane()
            .iter()
            .any(|line| line.starts_with("-- fails_here"))
    });
    let pane = rig.pane();
    let title = pane
        .iter()
        .position(|line| line.starts_with("-- fails_here"))
        .unwrap_or_else(|| panic!("{pane:?}"));
    assert_eq!(pane[title], "-- fails_here at src/lib.rs:14");
    assert!(
        pane[title + 1..]
            .iter()
            .any(|line| line.contains("assertion")),
        "{pane:?}"
    );
    rig.press("goto");
    rig.wait_until("the file", Duration::from_secs(10), |rig| {
        rig.cursor().0.ends_with("src/lib.rs")
    });
    assert_eq!(
        (rig.cursor().1, rig.cursor().2),
        (14, 9),
        "{:?}",
        rig.cursor()
    );
    assert_eq!(
        rig.app.state().focus(),
        crate::app::state::Focus::Editor,
        "the keyboard is in the file"
    );

    // A test that passed has no panic: grep finds its `fn` (line 8 of the library).
    rig.app
        .run_action(Action::FocusPluginView("test-ui".into()));
    rig.press("down");
    rig.press("down");
    rig.wait_until("the next test selected", Duration::from_secs(10), |rig| {
        rig.pane().iter().all(|line| !line.starts_with("-- "))
    });
    rig.press("goto");
    rig.wait_until("the code of the test", Duration::from_secs(30), |rig| {
        rig.cursor().0.ends_with("src/lib.rs") && rig.cursor().1 == 8
    });
}

impl Rig {
    fn draw(&mut self) {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(140, 40)).unwrap();
        let app = &mut self.app;
        terminal
            .draw(|frame| crate::components::prepare_and_render(frame, app.state_mut()))
            .unwrap();
    }

    /// Right-clicks the line `row` of the tests of the pane (the first is its header), under
    /// its row of buttons.
    fn right_click_on_line(&mut self, row: usize) {
        self.draw();
        let body = self
            .app
            .state()
            .plugin_body(&PluginViewId::new("test-ui"))
            .expect("the pane is on the screen");
        self.app
            .handle_input(InputEvent::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Right),
                column: body.x + 4,
                row: body.y + 1 + row as u16,
                modifiers: KeyModifiers::NONE,
            }));
    }

    fn menu_labels(&self) -> Vec<String> {
        self.app
            .state()
            .menu()
            .map(|menu| {
                menu.labels_for_test()
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Chooses the item of the menu that is open with this label.
    fn choose(&mut self, label: &str) {
        let at = self
            .menu_labels()
            .iter()
            .position(|item| item == label)
            .unwrap_or_else(|| panic!("no {label} in {:?}", self.menu_labels()));
        for _ in 0..at {
            self.key(KeyCode::Down);
        }
        self.key(KeyCode::Enter);
    }

    /// The row of the pane that shows `text`.
    fn line_of(&self, text: &str) -> usize {
        self.pane()
            .iter()
            .position(|line| line.trim_end().ends_with(text))
            .unwrap_or_else(|| panic!("{text} is not in {:?}", self.pane()))
    }

    /// A rig that has listed and run the project, as the user would.
    fn after_a_run() -> Self {
        let mut rig = rust_rig();
        rig.wait_until("the question", Duration::from_secs(60), |rig| rig.asking());
        rig.key(KeyCode::Char('y'));
        rig.press("discover");
        rig.wait_for_header("tests 5/5");
        rig.press("run_all");
        rig.wait_for_header("ok 4 FAIL 1");
        rig.wait_until("the run to end", Duration::from_secs(30), |rig| {
            !rig.header().contains("| run ")
        });
        rig
    }
}

#[test]
#[ignore = "needs the modules built and cargo-nextest"]
fn a_right_click_on_a_failed_test_offers_its_error_which_is_shown_in_a_popup() {
    if !nextest_is_installed() {
        eprintln!("cargo-nextest is not installed: skipped");
        return;
    }
    let mut rig = Rig::after_a_run();

    let row = rig.line_of("] fails_here");
    rig.right_click_on_line(row);
    rig.wait_until("the menu", Duration::from_secs(10), |rig| {
        matches!(rig.app.state().popup(), Popup::Menu(_))
    });

    assert_eq!(
        rig.menu_labels(),
        [
            "Open Test Code",
            "Show Test Error",
            "Run Selected",
            "Debug Selected",
            "Discover Tests"
        ]
    );
    rig.choose("Show Test Error");
    rig.wait_until("the popup", Duration::from_secs(10), |rig| {
        matches!(rig.app.state().popup(), Popup::PluginReadme(_))
    });
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(140, 40)).unwrap();
    let app = &mut rig.app;
    terminal
        .draw(|frame| crate::components::prepare_and_render(frame, app.state_mut()))
        .unwrap();
    let screen = terminal.backend().to_string();
    assert!(
        screen.contains("fx tests::fails_here") || screen.contains("tests::fails_here"),
        "{screen}"
    );
    assert!(screen.contains("assertion"), "{screen}");
    assert!(screen.contains("at src/lib.rs:14"), "{screen}");
    rig.key(KeyCode::Esc);
    assert!(matches!(rig.app.state().popup(), Popup::None));
}

#[cfg(unix)]
#[test]
#[ignore = "needs the modules built and cargo-nextest"]
fn a_right_click_on_a_test_runs_it_under_the_debugger_as_the_adapter_built_it() {
    if !nextest_is_installed() {
        eprintln!("cargo-nextest is not installed: skipped");
        return;
    }
    let launched: std::sync::Arc<std::sync::Mutex<Option<serde_json::Value>>> = Default::default();
    let seen = std::sync::Arc::clone(&launched);
    let adapter = super::debug_tests::scripted(move |adapter| {
        adapter.answer_initialize(serde_json::json!({ "supportsConfigurationDoneRequest": true }));
        let (_, launching) = adapter.expect_request("launch");
        *seen.lock().unwrap() = Some(launching);
        while adapter.next().is_some() {}
    });
    let mut rig = Rig::after_a_run();
    rig.app =
        std::mem::replace(&mut rig.app, App::new(AppState::default())).with_debug_adapter(adapter);

    let row = rig.line_of("] panics_ok");
    rig.right_click_on_line(row);
    rig.wait_until("the menu", Duration::from_secs(10), |rig| {
        matches!(rig.app.state().popup(), Popup::Menu(_))
    });
    assert!(
        rig.menu_labels().contains(&"Debug Selected".to_owned()),
        "{:?} for line {row} of {:?}",
        rig.menu_labels(),
        rig.pane()
    );
    rig.choose("Debug Selected");

    rig.wait_until("the debugger to launch", Duration::from_secs(180), |rig| {
        rig.pump_is_launched(&launched)
    });
    let launching = launched.lock().unwrap().clone().unwrap();
    let program = launching["program"].as_str().unwrap();
    assert!(Path::new(program).is_file(), "{program}");
    assert_eq!(
        launching["args"],
        serde_json::json!(["--exact", "tests::panics_ok", "--nocapture"])
    );
    // The folder of the package, which is the project's own folder.
    let root = fs::canonicalize(rig._dirs.0.path()).unwrap();
    assert_eq!(launching["cwd"], root.display().to_string());
    assert_eq!(launching["runInTerminal"], true);
}

impl Rig {
    fn pump_is_launched(&self, launched: &std::sync::Mutex<Option<serde_json::Value>>) -> bool {
        launched.lock().unwrap().is_some()
    }
}
