//! The plugins the user has: found in the plugins folder, turned on once (after
//! being told what they may do) and remembered, and listed to be turned on and off.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus, Popup};
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::event::action::Action;
use crate::persist::saved_state::{self, PluginRecord, SavedState};
use crate::plugins::fixtures::{Echo, First};
use crate::ui::PluginViewId;
use crate::ui::layout::LayoutTree;
use crate::ui::view::ViewNode;

// --- what is remembered ---

#[test]
fn what_the_user_decided_about_a_plugin_is_remembered_and_a_reset_forgets_it() {
    let mut state = SavedState::default();
    let record = PluginRecord {
        run: Vec::new(),
        enabled: true,
        capabilities: vec!["git.read".into()],
        place: None,
        file_types: Vec::new(),
        own_thread: false,
    };

    assert!(state.set_plugin("viewer", record.clone()), "news");
    assert!(
        !state.set_plugin("viewer", record.clone()),
        "not news twice"
    );
    assert_eq!(state.plugin("viewer"), Some(&record));
    assert_eq!(state.plugin("other"), None);
    assert_eq!(state.reset().plugin("viewer"), None, "a reset asks again");
}

#[test]
fn the_decisions_are_written_to_the_state_file_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = SavedState::default();
    state.set_plugin(
        "viewer",
        PluginRecord {
            run: Vec::new(),
            enabled: true,
            capabilities: vec!["git.read".into(), "git.write".into()],
            place: None,
            file_types: Vec::new(),
            own_thread: false,
        },
    );

    saved_state::save(dir.path(), &state).unwrap();
    let (read, warning) = saved_state::load(Some(dir.path()));

    assert!(warning.is_none());
    assert_eq!(read, state);
}

#[test]
fn a_state_file_without_plugins_reads_and_writes_none() {
    let dir = tempfile::tempdir().unwrap();
    saved_state::save(dir.path(), &SavedState::default()).unwrap();

    let text = fs::read_to_string(saved_state::path_in(dir.path())).unwrap();

    assert!(!text.contains("plugins"), "{text}");
}

// --- the app ---

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
    config: tempfile::TempDir,
}

const LAYOUT: &str = r#"Row([Pane(view: Editor), Fixed(40, Pane(view: Plugin("hello")))])"#;

fn manifest(id: &str, extra: &str) -> String {
    format!(
        "id = \"{id}\"\nname = \"Hello\"\nversion = \"0.1.0\"\nabi = \"1.2\"\nwasm = \"plugin.wasm\"\n{extra}"
    )
}

/// Puts a plugin in the plugins folder of `config`.
fn install(config: &Path, id: &str, extra: &str, module: &[u8]) {
    let dir = config.join("plugins").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("manifest.toml"), manifest(id, extra)).unwrap();
    fs::write(dir.join("plugin.wasm"), module).unwrap();
}

fn on(capabilities: &[&str]) -> PluginRecord {
    PluginRecord {
        run: Vec::new(),
        enabled: true,
        capabilities: capabilities.iter().map(|name| (*name).to_owned()).collect(),
        place: None,
        file_types: Vec::new(),
        own_thread: false,
    }
}

fn started(config: tempfile::TempDir, saved: SavedState) -> Rig {
    let mut state = AppState::new(Editor::new(Buffer::from_text("")));
    state.set_layout(LayoutTree::from_ron(LAYOUT).unwrap());
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state)
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(saved)
        .with_events(tx);
    Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(120, 24)).unwrap(),
        config,
    }
}

/// A config folder with the plugin `hello` in it.
fn rig(extra: &str, saved: SavedState) -> Rig {
    let config = tempfile::tempdir().unwrap();
    install(
        config.path(),
        "hello",
        extra,
        &Echo::new(First::Echo).build(),
    );
    started(config, saved)
}

fn saved_with(record: PluginRecord) -> SavedState {
    let mut saved = SavedState::default();
    saved.set_plugin("hello", record);
    saved
}

impl Rig {
    fn screen(&mut self) -> String {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        self.terminal.backend().to_string()
    }

    fn pump(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.app.handle_event(event);
        }
    }

    fn wait_until(&mut self, what: &str, done: impl Fn(&mut Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            self.pump();
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn has_view(&self) -> bool {
        self.app
            .state()
            .plugin_view(&PluginViewId::new("hello"))
            .is_some()
    }

    fn view_text(&self) -> Option<String> {
        let view = self.app.state().plugin_view(&PluginViewId::new("hello"))?;
        match view.content() {
            ViewNode::Lines(lines) => lines.first().map(|line| line.text.to_string()),
            _ => None,
        }
    }

    fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
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

    /// Waits a moment and says whether the plugin has shown anything.
    fn stays_quiet(&mut self) -> bool {
        let until = Instant::now() + Duration::from_millis(400);
        while Instant::now() < until {
            self.pump();
            std::thread::sleep(Duration::from_millis(10));
        }
        !self.has_view()
    }

    fn state_file(&self) -> PathBuf {
        saved_state::path_in(self.config.path())
    }

    /// What is in the state file now, read back as the next start would.
    fn remembered(&self) -> SavedState {
        saved_state::load(Some(self.config.path())).0
    }

    fn open_list(&mut self) {
        self.app.run_action(Action::ShowPlugins);
    }

    fn notification(&self) -> Option<&str> {
        self.app.state().latest_notification()
    }
}

#[test]
fn a_plugin_the_user_turned_on_starts_with_the_app_and_its_actions_are_known() {
    let mut rig = rig(
        "[[action]]\nid = \"go\"\ntitle = \"Go\"\n",
        saved_with(on(&[])),
    );

    rig.wait_until("the plugin's view", |rig| rig.has_view());

    assert_eq!(rig.view_text().as_deref(), Some(r#"{"type":"start"}"#));
    rig.press(KeyCode::F(1));
    for letter in "hello".chars() {
        rig.press(KeyCode::Char(letter));
    }
    assert!(rig.screen().contains("hello: Go"));
}

#[test]
fn a_plugin_nobody_turned_on_does_not_run_and_nothing_is_asked() {
    let mut rig = rig("", SavedState::default());

    assert!(rig.stays_quiet());
    assert!(!rig.asking());
}

#[test]
fn a_plugin_that_was_turned_off_does_not_run() {
    let mut off = on(&[]);
    off.enabled = false;
    let mut rig = rig("", saved_with(off));

    assert!(rig.stays_quiet());
    assert!(!rig.asking());
}

#[test]
fn a_plugin_that_wants_more_than_it_was_allowed_is_asked_about_before_it_runs() {
    let mut rig = rig(
        "capabilities = [\"git.read\", \"git.write\"]\n",
        saved_with(on(&["git.read"])),
    );

    assert!(rig.asking());
    let question = rig.question();
    assert!(
        question.contains("Turn on the plugin Hello 0.1.0?"),
        "{question}"
    );
    assert!(
        question.contains("read the state of the git repository"),
        "{question}"
    );
    assert!(
        question.contains("change the git repository: stage and unstage files"),
        "{question}"
    );
    assert!(rig.stays_quiet(), "not running while the question is open");
}

#[test]
fn yes_runs_the_plugin_and_the_decision_is_remembered_for_the_next_start() {
    let mut rig = rig("capabilities = [\"git.read\"]\n", saved_with(on(&[])));
    assert!(rig.asking());

    rig.press(KeyCode::Char('y'));
    rig.wait_until("the plugin's view", |rig| rig.has_view());

    let remembered = rig.remembered();
    assert_eq!(remembered.plugin("hello"), Some(&on(&["git.read"])));
    assert!(rig.state_file().is_file());
    // The next start has what was said, and is not asked.
    let mut next = started(rig.config, remembered);
    assert!(!next.asking());
    next.wait_until("the plugin's view", |rig| rig.has_view());
}

#[test]
fn no_keeps_the_plugin_off_and_it_is_not_asked_about_again_at_the_next_start() {
    let mut rig = rig("capabilities = [\"git.read\"]\n", saved_with(on(&[])));
    assert!(rig.asking());

    rig.press(KeyCode::Char('n'));

    assert!(!rig.asking());
    assert!(rig.stays_quiet());
    assert!(
        rig.notification()
            .unwrap()
            .contains("plugin hello stays off")
    );
    let remembered = rig.remembered();
    assert_eq!(
        remembered.plugin("hello").map(|record| record.enabled),
        Some(false)
    );
    let mut next = started(rig.config, remembered);
    assert!(!next.asking());
    assert!(next.stays_quiet());
}

#[test]
fn esc_and_enter_do_not_turn_a_plugin_on() {
    for code in [KeyCode::Esc, KeyCode::Enter] {
        let mut rig = rig("capabilities = [\"git.read\"]\n", saved_with(on(&[])));

        rig.press(code);

        assert!(rig.stays_quiet(), "{code:?}");
    }
}

#[test]
fn a_plugin_that_asks_for_less_than_it_was_allowed_is_not_asked_about() {
    let mut rig = rig(
        "capabilities = [\"git.read\"]\n",
        saved_with(on(&["git.read", "git.write"])),
    );

    assert!(!rig.asking());
    rig.wait_until("the plugin's view", |rig| rig.has_view());
}

#[test]
fn several_plugins_that_need_an_answer_are_asked_one_after_the_other() {
    let config = tempfile::tempdir().unwrap();
    let module = Echo::new(First::Echo).build();
    install(
        config.path(),
        "hello",
        "capabilities = [\"git.read\"]\n",
        &module,
    );
    install(
        config.path(),
        "second",
        "capabilities = [\"git.write\"]\n",
        &module,
    );
    let mut saved = saved_with(on(&[]));
    saved.set_plugin("second", on(&[]));
    let mut rig = started(config, saved);

    assert!(rig.question().contains("Hello 0.1.0"));
    rig.press(KeyCode::Char('n'));
    assert!(rig.asking(), "the next one is asked");
    assert!(rig.question().contains("stage and unstage"));
    rig.press(KeyCode::Char('n'));
    assert!(!rig.asking());
}

#[test]
fn a_plugin_may_only_do_what_its_manifest_asked_for() {
    let config = tempfile::tempdir().unwrap();
    let module = Echo::new(First::Echo)
        .asks_on_action(r#"{"type":"git_stage","request":6,"path":"a.txt"}"#)
        .build();
    install(
        config.path(),
        "hello",
        "capabilities = [\"git.read\"]\n[[action]]\nid = \"go\"\n",
        &module,
    );
    let mut rig = started(config, saved_with(on(&["git.read"])));
    rig.wait_until("the plugin's view", |rig| rig.has_view());

    rig.app.run_action(Action::Plugin("hello:go".into()));

    rig.wait_until("the refusal", |rig| {
        rig.view_text().is_some_and(|text| text.contains("refused"))
    });
    assert_eq!(
        rig.view_text().unwrap(),
        r#"{"type":"refused","request":6,"reason":{"type":"denied","capability":"git.write"}}"#
    );
}

// --- the list ---

fn list_text(rig: &mut Rig) -> String {
    rig.open_list();
    rig.screen()
}

#[test]
fn the_list_shows_each_plugin_and_whether_it_is_on() {
    let mut rig = rig("", SavedState::default());
    install(rig.config.path(), "broken", "", b"x");
    fs::write(
        rig.config.path().join("plugins/broken/manifest.toml"),
        "nonsense",
    )
    .unwrap();

    let screen = list_text(&mut rig);

    assert!(screen.contains("Hello 0.1.0  local · off"), "{screen}");
    assert!(screen.contains("broken       cannot run"), "{screen}");
}

#[test]
fn a_plugin_that_runs_is_marked_on_in_the_list() {
    let mut rig = rig("", saved_with(on(&[])));
    rig.wait_until("the plugin's view", |rig| rig.has_view());

    let screen = list_text(&mut rig);

    assert!(screen.contains("Hello 0.1.0  local · on"), "{screen}");
}

#[test]
fn an_empty_list_says_there_are_no_plugins() {
    let config = tempfile::tempdir().unwrap();
    let mut rig = started(config, SavedState::default());

    let screen = list_text(&mut rig);

    assert!(matches!(rig.app.state().popup(), Popup::PluginList(_)));
    assert!(screen.contains("No plugins yet"), "{screen}");
}

#[test]
fn turning_a_plugin_on_from_the_list_asks_what_it_may_do_and_then_runs_it() {
    let mut rig = rig("capabilities = [\"git.read\"]\n", SavedState::default());
    rig.open_list();

    rig.press(KeyCode::Enter);

    assert!(rig.asking());
    assert!(
        rig.question()
            .contains("read the state of the git repository")
    );
    rig.press(KeyCode::Char('y'));
    rig.wait_until("the plugin's view", |rig| rig.has_view());
    assert_eq!(rig.remembered().plugin("hello"), Some(&on(&["git.read"])));
}

#[test]
fn a_plugin_that_wants_the_remote_says_in_the_question_that_it_uses_the_network() {
    let mut rig = rig("capabilities = [\"git.remote\"]\n", SavedState::default());
    rig.open_list();

    rig.press(KeyCode::Enter);

    assert!(rig.asking());
    let question = rig.question();
    assert!(
        question.contains("fetch from, pull from and push to"),
        "{question}"
    );
    assert!(question.contains("uses the network"), "{question}");
}

#[test]
fn a_plugin_that_asks_for_nothing_is_still_asked_the_first_time() {
    let mut rig = rig("", SavedState::default());
    rig.open_list();

    rig.press(KeyCode::Enter);

    assert!(
        rig.question()
            .contains("draw in its own pane and nothing else")
    );
}

#[test]
fn turning_a_plugin_off_takes_its_pane_and_actions_away_and_it_is_not_asked_again_to_turn_it_on() {
    let mut rig = rig(
        "[[action]]\nid = \"go\"\ntitle = \"Go\"\nkeys = \"s\"\n",
        saved_with(on(&[])),
    );
    rig.wait_until("the plugin's view", |rig| rig.has_view());
    rig.app.run_action(Action::FocusPluginView("hello".into()));
    assert_eq!(rig.app.state().focus(), Focus::Plugin);

    rig.open_list();
    rig.press(KeyCode::Char(' '));

    assert!(!rig.has_view(), "its pane is empty");
    assert_eq!(
        rig.app.state().focus(),
        Focus::Editor,
        "the keyboard is not in it"
    );
    assert_eq!(
        rig.remembered().plugin("hello").map(|r| r.enabled),
        Some(false)
    );
    rig.press(KeyCode::Esc);
    rig.press(KeyCode::F(1));
    for letter in "hello".chars() {
        rig.press(KeyCode::Char(letter));
    }
    assert!(!rig.screen().contains("hello: Go"), "its action is gone");
    rig.press(KeyCode::Esc);

    // Turning it on again is not a new question: it was allowed what it asks.
    rig.open_list();
    rig.press(KeyCode::Enter);
    assert!(!rig.asking());
    rig.wait_until("the plugin's view", |rig| rig.has_view());
    assert_eq!(
        rig.remembered().plugin("hello").map(|r| r.enabled),
        Some(true)
    );
}

#[test]
fn choosing_a_plugin_whose_manifest_is_no_good_says_why() {
    let mut rig = rig("", SavedState::default());
    fs::write(
        rig.config.path().join("plugins/hello/manifest.toml"),
        manifest("hello", "").replace("abi = \"1.2\"", "abi = \"9.0\""),
    )
    .unwrap();
    rig.open_list();

    rig.press(KeyCode::Enter);

    let notice = rig.notification().unwrap();
    assert!(notice.starts_with("plugin hello cannot run: "), "{notice}");
    assert!(notice.contains("update TerminalCode"), "{notice}");
    assert!(!rig.asking());
}

#[test]
fn a_plugin_that_was_turned_on_and_whose_manifest_broke_is_told_at_the_start() {
    let config = tempfile::tempdir().unwrap();
    install(config.path(), "hello", "", &Echo::new(First::Echo).build());
    fs::write(
        config.path().join("plugins/hello/manifest.toml"),
        "nonsense",
    )
    .unwrap();

    let mut rig = started(config, saved_with(on(&[])));

    assert!(
        rig.notification()
            .unwrap()
            .starts_with("plugin hello cannot run: ")
    );
    assert!(rig.stays_quiet());
}

fn looping_module() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "alloc") (param i32) (result i32) (i32.const 1024))
            (func (export "handle") (param i32 i32) (loop $forever (br $forever))))"#,
    )
    .unwrap()
}

#[test]
fn a_plugin_that_fails_too_often_is_shown_as_stopped_in_the_list() {
    let config = tempfile::tempdir().unwrap();
    install(config.path(), "hello", "", &looping_module());
    let mut rig = started(config, saved_with(on(&[])));

    rig.wait_until("the plugin to be stopped", |rig| {
        rig.notification()
            .is_some_and(|text| text.contains("plugin hello is off"))
            || rig
                .app
                .state()
                .notifications()
                .texts()
                .iter()
                .any(|text| text.contains("plugin hello is off"))
    });
    let screen = list_text(&mut rig);

    assert!(screen.contains("Hello 0.1.0"), "{screen}");
    assert!(screen.contains("off: it failed too often"), "{screen}");
}

#[test]
fn the_palette_opens_the_list() {
    let mut rig = rig("", SavedState::default());

    rig.press(KeyCode::F(1));
    for letter in "plugins".chars() {
        rig.press(KeyCode::Char(letter));
    }
    assert!(rig.screen().contains("Plugins: Manage"));
    rig.press(KeyCode::Enter);

    assert!(matches!(rig.app.state().popup(), Popup::PluginList(_)));
    assert!(rig.screen().contains("Hello 0.1.0"));
}

fn list_is_open(rig: &Rig) -> bool {
    matches!(rig.app.state().popup(), Popup::PluginList(_))
}

#[test]
fn closing_the_list_does_not_bring_it_back() {
    let mut rig = rig("", SavedState::default());
    rig.open_list();
    rig.press(KeyCode::Esc);
    assert!(!list_is_open(&rig));
}

// --- commands: what the user is shown, and trust in the folder ---

const RUN_ECHO: &str = r#"{"type":"run","request":5,"program":"sh","args":["-c","echo hi"]}"#;
const GOT_ENDED: &str =
    r#"{"type":"run_done","request":5,"code":0,"cancelled":false,"truncated":false}"#;

const RUNNER_MANIFEST: &str = "capabilities = [\"process.run\"]\nrun = [\"sh -c *\"]\n[[action]]\nid = \"r\"\ntitle = \"Run\"\n";

fn runner_record() -> PluginRecord {
    let mut record = on(&["process.run"]);
    record.run = vec!["sh -c *".to_owned()];
    record
}

/// A plugin that the user turned on, which runs a command when it is sent the action `r`.
fn runner_rig() -> Rig {
    let config = tempfile::tempdir().unwrap();
    let module = Echo::new(First::Echo)
        .asks_on_action_named(b'r', RUN_ECHO)
        .build();
    install(config.path(), "hello", RUNNER_MANIFEST, &module);
    started(config, saved_with(runner_record()))
}

impl Rig {
    fn run_the_command(&mut self) {
        self.app.run_action(Action::Plugin("hello:r".into()));
    }

    fn view_is(&mut self, text: &'static str) {
        self.wait_until(text, |rig| rig.view_text().as_deref() == Some(text));
    }
}

#[test]
fn a_plugin_that_runs_commands_is_asked_with_the_commands_listed() {
    let mut rig = rig(
        "capabilities = [\"process.run\"]\nrun = [\"cargo nextest *\", \"git status\"]\n",
        saved_with(on(&[])),
    );

    assert!(rig.asking());
    let question = rig.question();
    assert!(
        question.contains("run commands, with no shell"),
        "{question}"
    );
    assert!(
        question.contains("`cargo nextest *`, `git status`"),
        "{question}"
    );
    assert!(question.contains("code of the project"), "{question}");
    assert!(rig.stays_quiet(), "not running while the question is open");
}

#[test]
fn yes_remembers_the_commands_the_user_was_shown() {
    let mut rig = rig(
        "capabilities = [\"process.run\"]\nrun = [\"cargo nextest *\"]\n",
        saved_with(on(&[])),
    );

    rig.press(KeyCode::Char('y'));
    rig.wait_until("the plugin's view", |rig| rig.has_view());

    let remembered = rig.remembered();
    let record = remembered.plugin("hello").unwrap();
    assert_eq!(record.run, vec!["cargo nextest *".to_owned()]);
    assert_eq!(record.capabilities, vec!["process.run".to_owned()]);
}

#[test]
fn a_plugin_that_comes_to_run_one_more_command_is_asked_again_and_one_that_does_not_is_not() {
    let manifest =
        "capabilities = [\"process.run\"]\nrun = [\"cargo nextest *\", \"git status\"]\n";
    let mut shown = on(&["process.run"]);
    shown.run = vec!["cargo nextest *".to_owned()];

    let asked = rig(manifest, saved_with(shown));
    assert!(asked.asking());

    let mut all = on(&["process.run"]);
    all.run = vec!["git status".to_owned(), "cargo nextest *".to_owned()];
    let mut quiet = rig(manifest, saved_with(all));
    assert!(!quiet.asking());
    quiet.wait_until("the plugin's view", |rig| rig.has_view());
}

#[test]
fn a_command_in_a_folder_that_is_not_trusted_asks_the_user_once_and_yes_lets_it_run() {
    let mut rig = runner_rig();
    rig.view_is(r#"{"type":"trust","trusted":false}"#);
    assert!(!rig.asking());

    rig.run_the_command();
    rig.wait_until("the question", |rig| rig.asking());

    let question = rig.question();
    assert!(question.contains("Trust the folder"), "{question}");
    assert!(question.contains("hello"), "{question}");
    assert!(question.contains("code of this project"), "{question}");
    rig.press(KeyCode::Char('y'));
    rig.view_is(r#"{"type":"trust","trusted":true}"#);
    assert!(
        rig.remembered().is_trusted(
            &std::env::current_dir()
                .unwrap()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
        ),
        "remembered for the folder"
    );
    rig.run_the_command();
    rig.view_is(GOT_ENDED);
}

#[test]
fn no_leaves_the_folder_untrusted_and_the_user_is_not_asked_again_by_the_same_plugin() {
    let mut rig = runner_rig();
    rig.view_is(r#"{"type":"trust","trusted":false}"#);
    rig.run_the_command();
    rig.wait_until("the question", |rig| rig.asking());

    rig.press(KeyCode::Char('n'));
    rig.run_the_command();
    rig.wait_until("the refusal", |rig| {
        rig.view_text().as_deref()
            == Some(r#"{"type":"refused","request":5,"reason":{"type":"untrusted"}}"#)
    });

    assert!(!rig.asking());
    assert!(rig.remembered().reset() == SavedState::default().reset());
    let folder = std::env::current_dir().unwrap().canonicalize().unwrap();
    assert!(!rig.remembered().is_trusted(&folder.to_string_lossy()));
}

#[test]
fn a_folder_that_was_trusted_before_is_trusted_at_the_next_start() {
    let mut rig = runner_rig();
    rig.view_is(r#"{"type":"trust","trusted":false}"#);
    rig.run_the_command();
    rig.wait_until("the question", |rig| rig.asking());
    rig.press(KeyCode::Char('y'));
    rig.view_is(r#"{"type":"trust","trusted":true}"#);

    let remembered = rig.remembered();
    let mut next = started(rig.config, remembered);

    assert!(!next.asking());
    next.view_is(r#"{"type":"trust","trusted":true}"#);
}

#[test]
fn the_action_asks_about_the_folder_and_says_so_when_it_is_trusted_already() {
    let mut rig = runner_rig();
    rig.view_is(r#"{"type":"trust","trusted":false}"#);

    rig.app.run_action(Action::TrustFolder);
    assert!(rig.asking());
    rig.press(KeyCode::Char('y'));
    rig.app.run_action(Action::TrustFolder);

    assert!(!rig.asking());
    assert_eq!(rig.notification(), Some("this folder is trusted already"));
}
