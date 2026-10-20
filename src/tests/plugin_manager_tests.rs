//! The plugin manager popup: the plugins of the index and the ones in the plugins
//! folder, installing, updating and removing with a server on this machine standing
//! in for GitHub, and the questions that come after a download.

use std::collections::HashMap;
use std::fs;
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Popup};
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::event::action::Action;
use crate::persist::saved_state::{self, Place, PluginRecord, SavedState};
use crate::plugins::fixtures::{Echo, First};
use crate::plugins::index::Index;
use crate::plugins::install::tests::{Reply, serve};
use crate::plugins::install::{self, JobEvent, Policy};
use crate::plugins::manifest::Host;
use crate::ui::layout::LayoutTree;
use crate::ui::{ComponentKind, PluginViewId};

const LAYOUT: &str = "Col([Row([Fixed(30, SideBar(views: [Explorer])), Col([Pane(view: Editor), \
                      Fixed(8, BottomBar(views: [Terminal]))])])])";

fn module() -> Vec<u8> {
    Echo::new(First::Echo).build()
}

fn manifest_text(version: &str, extra: &str) -> String {
    format!(
        "id = \"echo\"\nname = \"Echo\"\nversion = \"{version}\"\nabi = \"1.2\"\nwasm = \"plugin.wasm\"\n{extra}"
    )
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A server with the index and the plugin `echo` at `version`; the index says the
/// module has the hash `promised` (the real one when `None`).
fn server(version: &str, extra: &str, promised: Option<String>, requires: &str) -> String {
    server_of(version, extra, promised, requires, false, None)
}

/// The README the servers of the tests give for `echo`.
const README: &str = "# Echo\n\nSays **what it is told**.\n\n```\nsome code\n```\n";

fn server_of(
    version: &str,
    extra: &str,
    promised: Option<String>,
    requires: &str,
    with_readme: bool,
    description: Option<&str>,
) -> String {
    let module = module();
    // The address is only known once the server runs, and the index names it.
    let routes: HashMap<&'static str, Reply> = HashMap::from([
        (
            "/manifest.toml",
            Reply::Body(manifest_text(version, extra).into_bytes()),
        ),
        ("/plugin.wasm", Reply::Body(module.clone())),
        ("/README.md", Reply::Body(README.as_bytes().to_vec())),
    ]);
    let routes = match description {
        Some(text) => {
            let mut routes = routes;
            routes.insert("/description.md", Reply::Body(text.as_bytes().to_vec()));
            routes
        }
        None => routes,
    };
    let address = serve(routes);
    let readme = match with_readme {
        true => format!(r#","readme":"{address}/README.md""#),
        false => String::new(),
    };
    let promised = promised.unwrap_or_else(|| hash(&module));
    let index = format!(
        r#"{{"format":1,"plugins":[{{"id":"echo","name":"Echo","description":"Says what it is told","version":"{version}","manifest":"{address}/manifest.toml","module":"{address}/plugin.wasm","sha256":"{promised}","abi":"1.2"{requires}{readme}}}]}}"#
    );
    // A second server for the index, since it names the first.
    serve(HashMap::from([(
        "/index.json",
        Reply::Body(index.into_bytes()),
    )])) + "/index.json"
}

/// An official index with no plugins in it.
fn empty_official() -> String {
    serve(HashMap::from([(
        "/index.json",
        Reply::Body(br#"{"format":1,"plugins":[]}"#.to_vec()),
    )])) + "/index.json"
}

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
    config: tempfile::TempDir,
}

fn rig(index_url: String, saved: SavedState, before: impl FnOnce(&std::path::Path)) -> Rig {
    let config = tempfile::tempdir().unwrap();
    before(config.path());
    let mut state = AppState::new(Editor::new(Buffer::from_text("")));
    state.set_layout(LayoutTree::from_ron(LAYOUT).unwrap());
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state)
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(saved)
        .with_plugin_source(index_url)
        .with_events(tx);
    Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(140, 30)).unwrap(),
        config,
    }
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

    fn wait_for_text(&mut self, text: &str) {
        let text = text.to_owned();
        self.wait_until(&format!("`{text}` on the screen"), |rig| {
            rig.screen().contains(&text)
        });
    }

    fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn type_text(&mut self, text: &str) {
        for letter in text.chars() {
            self.press(KeyCode::Char(letter));
        }
    }

    fn open_manager(&mut self) {
        self.app.run_action(Action::ShowPlugins);
    }

    fn popup_is(&self, check: impl Fn(&Popup) -> bool) -> bool {
        check(self.app.state().popup())
    }

    fn asking(&self) -> bool {
        self.popup_is(|popup| matches!(popup, Popup::Confirm(_)))
    }

    fn menu_open(&self) -> bool {
        self.popup_is(|popup| matches!(popup, Popup::Menu(_)))
    }

    fn folder(&self) -> std::path::PathBuf {
        self.config.path().join("plugins/echo")
    }

    fn remembered(&self) -> SavedState {
        saved_state::load(Some(self.config.path())).0
    }

    fn notification(&self) -> Option<String> {
        self.app.state().latest_notification().map(str::to_owned)
    }

    fn running(&self) -> bool {
        self.app
            .state()
            .plugin_view(&PluginViewId::new("echo"))
            .is_some()
    }

    /// Opens the manager, waits for the index and installs `echo` from it.
    fn install_echo(&mut self) {
        self.open_manager();
        self.wait_for_text("not installed");
        self.type_text("i");
    }
}

fn record(enabled: bool, place: Option<Place>) -> PluginRecord {
    PluginRecord {
        enabled,
        capabilities: Vec::new(),
        place,
        file_types: Vec::new(),
        own_thread: false,
    }
}

fn saved(record: PluginRecord) -> SavedState {
    let mut saved = SavedState::default();
    saved.set_plugin("echo", record);
    saved
}

fn nothing(_: &std::path::Path) {}

/// Puts `echo` at `version` in the plugins folder the way the manager would.
fn installed_by_the_manager(version: &'static str) -> impl FnOnce(&std::path::Path) {
    move |config| {
        let index_url = server(version, "", None, "");
        let index = install::fetch_index(
            &index_url,
            Policy::local(&["127.0.0.1"]),
            None,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        let dir = config.join("plugins");
        fs::create_dir_all(&dir).unwrap();
        install::install(
            &index.entries[0],
            &dir,
            Host::THIS,
            Policy::local(&["127.0.0.1"]),
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    }
}

// --- the list ---

#[test]
fn the_list_shows_the_plugins_of_the_index_with_their_state() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        SavedState::default(),
        nothing,
    );

    rig.open_manager();
    rig.wait_for_text("Echo 1.0.0");

    let screen = rig.screen();
    assert!(screen.contains("not installed"), "{screen}");
    assert!(screen.contains("Says what it is told"), "{screen}");
}

#[test]
fn what_needs_a_newer_editor_is_shown_dim_with_what_it_needs_and_is_not_downloaded() {
    let index = server("1.0.0", "", None, r#","requires":"9.0.0""#);
    let mut rig = rig(index, SavedState::default(), nothing);
    rig.open_manager();
    rig.wait_for_text("needs TerminalCode 9.0.0");

    rig.press(KeyCode::Enter);

    assert!(
        rig.notification()
            .unwrap()
            .contains("needs TerminalCode 9.0.0")
    );
    assert!(!rig.folder().exists());
}

#[test]
fn an_index_that_cannot_be_fetched_says_so_and_the_list_goes_on() {
    let mut rig = rig(
        "http://127.0.0.1:1/index.json".to_owned(),
        SavedState::default(),
        nothing,
    );

    rig.open_manager();
    rig.wait_for_text("offline, showing the list known so far");

    assert!(matches!(rig.app.state().popup(), Popup::PluginList(_)));
}

#[test]
fn the_index_that_was_fetched_is_kept_for_the_next_time() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        SavedState::default(),
        nothing,
    );
    rig.open_manager();
    rig.wait_for_text("Echo 1.0.0");

    let cached = fs::read_to_string(rig.config.path().join("plugin-index.json")).unwrap();

    assert!(Index::parse(&cached).is_ok());
    assert!(cached.contains("\"echo\""));
}

// --- install ---

#[test]
fn installing_downloads_then_asks_what_it_may_do_then_where_it_goes() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        SavedState::default(),
        nothing,
    );
    rig.install_echo();

    rig.wait_until("the question about what it may do", |rig| rig.asking());
    assert!(rig.folder().join("installed.json").is_file());
    assert!(rig.folder().join("plugin.wasm").is_file());
    assert!(!rig.running(), "it does not run before the answer");

    rig.press(KeyCode::Char('y'));
    rig.wait_until("the question about where", |rig| rig.menu_open());
    assert!(rig.screen().contains("Put echo in the popup"));
    // The fourth place: the popup.
    rig.press(KeyCode::Down);
    rig.press(KeyCode::Down);
    rig.press(KeyCode::Down);
    rig.press(KeyCode::Enter);

    rig.wait_until("the plugin's popup", |rig| {
        rig.popup_is(|popup| matches!(popup, Popup::PluginView(_)))
    });
    let remembered = rig.remembered();
    let record = remembered.plugin("echo").unwrap();
    assert!(record.enabled);
    assert_eq!(record.place, Some(Place::Popup));
}

#[test]
fn a_module_that_is_not_the_one_the_index_promised_writes_nothing() {
    let wrong = hash(b"another module");
    let mut rig = rig(
        server("1.0.0", "", Some(wrong), ""),
        SavedState::default(),
        nothing,
    );
    rig.install_echo();

    rig.wait_until("the refusal", |rig| {
        rig.notification()
            .is_some_and(|text| text.contains("does not match what the index promised"))
    });

    assert!(!rig.folder().exists());
    assert!(matches!(rig.app.state().popup(), Popup::PluginList(_)));
    assert!(!rig.asking());
}

#[test]
fn a_stop_during_the_download_installs_nothing_and_goes_back_to_the_list() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        SavedState::default(),
        nothing,
    );
    rig.open_manager();
    rig.wait_for_text("not installed");

    rig.app.handle_event(Event::PluginJob(JobEvent::Done {
        id: "echo".into(),
        update: false,
        result: Err("stopped".to_owned()),
    }));

    assert!(
        rig.notification()
            .unwrap()
            .contains("stopped, nothing was installed")
    );
    assert!(matches!(rig.app.state().popup(), Popup::PluginList(_)));
    assert!(!rig.folder().exists());
}

#[test]
fn saying_no_to_what_it_may_do_leaves_it_installed_and_off_and_asks_nothing_more() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        SavedState::default(),
        nothing,
    );
    rig.install_echo();
    rig.wait_until("the question", |rig| rig.asking());

    rig.press(KeyCode::Esc);

    assert!(rig.folder().is_dir(), "installed");
    assert!(!rig.running());
    assert!(!rig.menu_open(), "no question about where");
    assert!(!rig.remembered().plugin("echo").unwrap().enabled);
}

#[test]
fn cancelling_the_placement_leaves_it_on_and_not_placed_until_place_asks_again() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        SavedState::default(),
        nothing,
    );
    rig.install_echo();
    rig.wait_until("the question", |rig| rig.asking());
    rig.press(KeyCode::Char('y'));
    rig.wait_until("the question about where", |rig| rig.menu_open());

    rig.press(KeyCode::Esc);

    assert!(!rig.menu_open());
    rig.wait_until("the plugin", |rig| rig.running());
    let remembered = rig.remembered();
    assert_eq!(remembered.plugin("echo").unwrap().place, None);
    assert!(
        rig.app
            .state()
            .layout()
            .bar_views(&ComponentKind::Plugin(PluginViewId::new("echo")))
            .is_none()
    );

    rig.open_manager();
    rig.wait_for_text("on · not placed");
    rig.type_text("p");
    assert!(rig.menu_open(), "Place asks again");
    rig.press(KeyCode::Enter);
    assert_eq!(
        rig.remembered().plugin("echo").unwrap().place,
        Some(Place::Explorer)
    );
    assert!(
        rig.app
            .state()
            .layout()
            .bar_views(&ComponentKind::Plugin(PluginViewId::new("echo")))
            .is_some()
    );
}

#[test]
fn a_folder_of_the_users_own_is_not_installed_over() {
    let own = |config: &std::path::Path| {
        let dir = config.join("plugins/echo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("manifest.toml"), manifest_text("0.0.1", "")).unwrap();
        fs::write(dir.join("plugin.wasm"), module()).unwrap();
    };
    let mut rig = rig(server("1.0.0", "", None, ""), SavedState::default(), own);
    rig.open_manager();
    rig.wait_for_text("local");

    rig.type_text("i");
    assert!(rig.notification().unwrap().contains("one of your own"));

    rig.type_text("x");
    assert!(rig.notification().unwrap().contains("does not remove it"));
    assert!(!rig.asking());
    assert!(rig.folder().join("manifest.toml").is_file());
}

// --- update ---

#[test]
fn an_update_is_offered_asked_about_and_the_new_version_runs() {
    let mut rig = rig(
        server("2.0.0", "", None, ""),
        saved(record(true, Some(Place::Explorer))),
        installed_by_the_manager("1.0.0"),
    );
    rig.open_manager();
    rig.wait_for_text("update 2.0.0 available");
    assert!(rig.screen().contains("Echo 1.0.0"));

    rig.type_text("i");
    assert!(rig.asking(), "asked first");
    rig.press(KeyCode::Char('y'));

    rig.wait_until("the new version", |rig| {
        fs::read_to_string(rig.folder().join("manifest.toml"))
            .is_ok_and(|text| text.contains("2.0.0"))
    });
    rig.wait_until("the list again", |rig| {
        rig.app
            .state()
            .latest_notification()
            .is_some_and(|text| text.contains("is now 2.0.0"))
    });
    rig.wait_until("the new version to run", |rig| rig.running());
}

#[test]
fn saying_no_to_an_update_changes_nothing() {
    let mut rig = rig(
        server("2.0.0", "", None, ""),
        SavedState::default(),
        installed_by_the_manager("1.0.0"),
    );
    rig.open_manager();
    rig.wait_for_text("update 2.0.0 available");
    rig.type_text("i");

    rig.press(KeyCode::Esc);

    assert!(
        fs::read_to_string(rig.folder().join("manifest.toml"))
            .unwrap()
            .contains("1.0.0")
    );
    assert!(matches!(rig.app.state().popup(), Popup::PluginList(_)));
}

// --- remove ---

#[test]
fn removing_deletes_the_folder_stops_the_plugin_and_keeps_what_was_decided() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        saved(record(true, Some(Place::Explorer))),
        installed_by_the_manager("1.0.0"),
    );
    rig.wait_until("the plugin", |rig| rig.running());
    rig.open_manager();
    rig.wait_for_text("Echo 1.0.0");

    rig.type_text("x");
    assert!(rig.asking());
    assert!(rig.folder().is_dir(), "not before the answer");
    rig.press(KeyCode::Char('y'));

    assert!(!rig.folder().exists());
    assert!(!rig.running());
    assert!(matches!(rig.app.state().popup(), Popup::PluginList(_)));
    assert!(
        rig.app
            .state()
            .layout()
            .bar_views(&ComponentKind::Plugin(PluginViewId::new("echo")))
            .is_none(),
        "no tab is left for a plugin that is gone"
    );
}

#[test]
fn installing_again_after_a_removal_brings_it_back_as_it_was() {
    let mut saved_record = record(true, Some(Place::Bottom));
    saved_record.capabilities = Vec::new();
    let mut rig = rig(server("1.0.0", "", None, ""), saved(saved_record), nothing);
    rig.install_echo();

    rig.wait_until("the plugin", |rig| rig.running());
    assert!(!rig.asking(), "it was allowed before: nothing to ask");
    assert!(!rig.menu_open(), "and it was placed before");
    assert!(
        rig.app
            .state()
            .layout()
            .bar_views(&ComponentKind::Plugin(PluginViewId::new("echo")))
            .is_some()
    );
}

#[test]
fn a_plugin_whose_folder_was_deleted_by_hand_is_stopped_when_the_manager_opens() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        saved(record(true, Some(Place::Explorer))),
        installed_by_the_manager("1.0.0"),
    );
    rig.wait_until("the plugin", |rig| rig.running());
    let bar = |rig: &Rig| {
        rig.app
            .state()
            .layout()
            .bar_views(&ComponentKind::Plugin(PluginViewId::new("echo")))
            .is_some()
    };
    assert!(bar(&rig), "its tab is there while it runs");

    fs::remove_dir_all(rig.folder()).unwrap();
    rig.open_manager();

    assert!(!bar(&rig), "no tab for a plugin that is gone");
    assert!(!rig.running());
    assert!(
        rig.notification()
            .unwrap()
            .contains("is gone from the plugins folder")
    );
}

// --- the palette ---

#[test]
fn a_plugin_in_a_popup_is_opened_from_the_palette() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        saved(record(true, Some(Place::Popup))),
        installed_by_the_manager("1.0.0"),
    );
    rig.wait_until("the plugin", |rig| rig.running());

    rig.press(KeyCode::F(1));
    rig.type_text("plugins: open");
    assert!(rig.screen().contains("plugins: open echo"));
    rig.press(KeyCode::Enter);

    assert!(rig.popup_is(|popup| matches!(popup, Popup::PluginView(_))));
}

#[test]
fn esc_stops_a_download_that_hangs_and_nothing_is_installed() {
    let module = module();
    let address = serve(HashMap::from([
        (
            "/manifest.toml",
            Reply::Body(manifest_text("1.0.0", "").into_bytes()),
        ),
        ("/plugin.wasm", Reply::Hang),
    ]));
    let index = format!(
        r#"{{"format":1,"plugins":[{{"id":"echo","name":"Echo","version":"1.0.0","manifest":"{address}/manifest.toml","module":"{address}/plugin.wasm","sha256":"{}","abi":"1.2"}}]}}"#,
        hash(&module)
    );
    let index_url = serve(HashMap::from([(
        "/index.json",
        Reply::Body(index.into_bytes()),
    )])) + "/index.json";
    let mut rig = rig(index_url, SavedState::default(), nothing);
    rig.install_echo();
    rig.wait_for_text("echo: downloading the module (esc stops)");

    rig.press(KeyCode::Esc);
    rig.press(KeyCode::Char('x'));
    assert!(
        matches!(rig.app.state().popup(), Popup::PluginList(_)),
        "keys do nothing while it works, and esc stops it instead of closing the list"
    );

    rig.wait_until("the stop", |rig| {
        rig.notification()
            .is_some_and(|text| text.contains("stopped, nothing was installed"))
    });
    assert!(!rig.folder().exists());
    assert!(!rig.screen().contains("esc stops"));
}

// --- extra sources ---

#[test]
fn a_plugin_of_a_source_is_listed_with_where_it_comes_from_and_installing_asks_first() {
    let source = server("1.0.0", "", None, "");
    let mut rig = rig(empty_official(), SavedState::default(), nothing);
    rig.app.settings_mut().plugin_sources = vec![source.clone()];

    rig.open_manager();
    rig.wait_for_text("Echo 1.0.0");
    let screen = rig.screen();
    assert!(screen.contains("not installed"), "{screen}");
    assert!(screen.contains("127.0.0.1"), "{screen}");
    assert!(screen.contains("not official"), "{screen}");

    rig.type_text("i");
    assert!(
        rig.asking(),
        "a source's plugin is asked about before it is downloaded"
    );
    assert!(!rig.folder().exists());
    let screen = rig.screen();
    assert!(screen.contains("not an official plugin"), "{screen}");
    assert!(screen.contains("Module sha256"), "{screen}");

    rig.press(KeyCode::Char('y'));
    rig.wait_until("the plugin installed", |rig| {
        rig.folder().join("installed.json").is_file()
    });
    let marker = install::marker(&rig.folder()).unwrap();
    assert_eq!(marker.entry.source.as_deref(), Some(source.as_str()));
}

#[test]
fn saying_no_to_a_source_downloads_nothing() {
    let mut rig = rig(empty_official(), SavedState::default(), nothing);
    rig.app.settings_mut().plugin_sources = vec![server("1.0.0", "", None, "")];
    rig.open_manager();
    rig.wait_for_text("Echo 1.0.0");

    rig.type_text("i");
    rig.press(KeyCode::Char('n'));

    assert!(!rig.asking());
    assert!(!rig.folder().exists());
    assert!(rig.popup_is(|popup| matches!(popup, Popup::PluginList(_))));
}

#[test]
fn a_source_cannot_replace_an_official_plugin_and_the_list_says_so() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        SavedState::default(),
        nothing,
    );
    rig.app.settings_mut().plugin_sources = vec![server("2.0.0", "", None, "")];

    rig.open_manager();
    rig.wait_for_text("left out, the name is taken: echo");

    let screen = rig.screen();
    assert!(screen.contains("Echo 1.0.0"), "{screen}");
    assert!(!screen.contains("Echo 2.0.0"), "{screen}");
}

#[test]
fn a_source_that_cannot_be_fetched_is_named_and_the_rest_is_listed() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        SavedState::default(),
        nothing,
    );
    rig.app.settings_mut().plugin_sources = vec!["http://127.0.0.1:1/index.json".to_owned()];

    rig.open_manager();
    rig.wait_for_text("source 127.0.0.1:1");
    rig.wait_for_text("Echo 1.0.0");
}

#[test]
fn the_official_index_named_as_a_source_is_one_fetch_and_stays_official() {
    let official = server("1.0.0", "", None, "");
    let mut rig = rig(official.clone(), SavedState::default(), nothing);
    rig.app.settings_mut().plugin_sources = vec![official];

    rig.open_manager();
    rig.wait_for_text("Echo 1.0.0");
    rig.pump();

    let screen = rig.screen();
    assert!(!screen.contains("left out"), "{screen}");
    assert!(!screen.contains("not official"), "{screen}");
    rig.type_text("i");
    // The first question is what it may do, after the download: none came before it.
    rig.wait_until("the question about what it may do", |rig| rig.asking());
    assert!(
        !rig.screen().contains("not an official plugin"),
        "an official plugin is not asked about as a source's"
    );
}

#[test]
fn a_row_starts_with_a_symbol_for_what_the_plugin_is() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        SavedState::default(),
        nothing,
    );
    rig.open_manager();
    rig.wait_for_text("+ Echo 1.0.0");

    let mut rig = rig_installed_and_on();
    rig.open_manager();
    rig.wait_for_text("● Echo 1.0.0");
    assert!(rig.screen().contains("on · popup"));
}

fn rig_installed_and_on() -> Rig {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        saved(record(true, Some(Place::Popup))),
        installed_by_the_manager("1.0.0"),
    );
    rig.wait_until("the plugin", |rig| rig.running());
    rig
}

fn list_is_open(rig: &Rig) -> bool {
    rig.popup_is(|popup| matches!(popup, Popup::PluginList(_)))
}

/// A plugin installed by the manager and off, with the list open on it.
fn off_in_the_list() -> Rig {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        SavedState::default(),
        installed_by_the_manager("1.0.0"),
    );
    rig.open_manager();
    rig.wait_for_text("○ Echo 1.0.0");
    rig
}

#[test]
fn the_list_comes_back_after_the_questions_of_turning_a_plugin_on() {
    let mut rig = off_in_the_list();

    rig.press(KeyCode::Enter);
    assert!(rig.asking());
    assert!(!list_is_open(&rig), "a question cannot open over the list");

    rig.press(KeyCode::Char('y'));
    rig.wait_until("the question about where", |rig| rig.menu_open());
    assert!(!list_is_open(&rig));
    rig.press(KeyCode::Enter);

    assert!(list_is_open(&rig), "the list is where the user was");
    rig.wait_for_text("● Echo 1.0.0");
}

#[test]
fn the_list_comes_back_when_the_question_is_refused() {
    let mut rig = off_in_the_list();
    rig.press(KeyCode::Enter);
    rig.press(KeyCode::Char('n'));
    assert!(list_is_open(&rig));
    rig.wait_for_text("○ Echo 1.0.0");
}

#[test]
fn the_list_comes_back_when_the_placement_is_cancelled() {
    let mut rig = off_in_the_list();
    rig.press(KeyCode::Enter);
    rig.press(KeyCode::Char('y'));
    rig.wait_until("the question about where", |rig| rig.menu_open());
    rig.press(KeyCode::Esc);
    assert!(list_is_open(&rig));
}

#[test]
fn opening_a_plugin_from_the_list_does_not_bring_the_list_back() {
    let mut rig = rig_installed_and_on();
    rig.open_manager();
    rig.wait_for_text("● Echo 1.0.0");
    rig.press(KeyCode::Enter);
    assert!(rig.popup_is(|popup| matches!(popup, Popup::PluginView(_))));
    rig.press(KeyCode::Esc);
    assert!(!list_is_open(&rig));
}

// --- about, update all ---

#[test]
fn d_opens_what_the_plugin_says_about_itself_with_its_readme_and_esc_goes_back() {
    let mut rig = rig(
        server_of("1.0.0", "", None, "", true, None),
        SavedState::default(),
        nothing,
    );
    rig.open_manager();
    rig.wait_for_text("+ Echo 1.0.0");

    rig.type_text("d");
    assert!(rig.popup_is(|popup| matches!(popup, Popup::PluginReadme(_))));
    rig.wait_for_text("Says what it is told.");
    let screen = rig.screen();
    assert!(screen.contains("An official plugin."), "{screen}");
    assert!(screen.contains("some code"), "{screen}");
    assert!(!screen.contains("**"), "{screen}");

    rig.press(KeyCode::Esc);
    assert!(list_is_open(&rig));
    rig.wait_for_text("+ Echo 1.0.0");
}

#[test]
fn a_plugin_that_lists_no_readme_says_so() {
    let mut rig = rig(
        server("1.0.0", "", None, ""),
        SavedState::default(),
        nothing,
    );
    rig.open_manager();
    rig.wait_for_text("+ Echo 1.0.0");
    rig.type_text("d");
    rig.wait_for_text("no README is listed");
}

#[test]
fn a_readme_that_cannot_be_fetched_says_why() {
    let nowhere = r#","readme":"http://127.0.0.1:1/README.md""#;
    let mut rig = rig(
        server("1.0.0", "", None, nowhere),
        SavedState::default(),
        nothing,
    );
    rig.open_manager();
    rig.wait_for_text("+ Echo 1.0.0");
    rig.type_text("d");
    rig.wait_for_text("The description could not be fetched");
}

#[test]
fn shift_i_updates_every_plugin_that_has_a_newer_version_after_asking() {
    let mut rig = rig(
        server("2.0.0", "", None, ""),
        saved(record(true, Some(Place::Explorer))),
        installed_by_the_manager("1.0.0"),
    );
    rig.open_manager();
    rig.wait_for_text("update 2.0.0 available");

    rig.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('I'),
        KeyModifiers::SHIFT,
    )));
    assert!(rig.asking());
    let question = rig.screen();
    assert!(question.contains("Update 1 plugin?"), "{question}");
    assert!(question.contains("1.0.0 -> 2.0.0"), "{question}");
    rig.press(KeyCode::Char('y'));

    rig.wait_until("the new version", |rig| {
        fs::read_to_string(rig.folder().join("manifest.toml"))
            .is_ok_and(|text| text.contains("2.0.0"))
    });
    rig.wait_until("the plugin to run again", |rig| rig.running());
}

#[test]
fn shift_i_with_everything_up_to_date_says_so_and_asks_nothing() {
    let mut rig = rig_installed_and_on();
    rig.open_manager();
    rig.wait_for_text("● Echo 1.0.0");
    rig.type_text("I");
    assert!(!rig.asking());
    assert_eq!(
        rig.notification().as_deref(),
        Some("every plugin is up to date")
    );
}

const DESCRIPTION: &str = "# Echo, in short\n\nRepeats **everything** it hears.\n";

#[test]
fn d_shows_description_md_of_the_plugin_before_its_readme() {
    let mut rig = rig(
        server_of("1.0.0", "", None, "", true, Some(DESCRIPTION)),
        SavedState::default(),
        nothing,
    );
    rig.open_manager();
    rig.wait_for_text("+ Echo 1.0.0");

    rig.type_text("d");

    rig.wait_for_text("Repeats everything it hears.");
    assert!(!rig.screen().contains("Says what it is told."));
}

#[test]
fn an_installed_plugin_shows_the_description_that_came_with_it_without_the_network() {
    let mut rig = rig(
        server_of("1.0.0", "", None, "", true, Some(DESCRIPTION)),
        SavedState::default(),
        nothing,
    );
    rig.install_echo();
    rig.wait_until("the question about what it may do", |rig| rig.asking());
    assert!(rig.folder().join("description.md").is_file());
    rig.press(KeyCode::Char('n'));
    rig.wait_for_text("○ Echo 1.0.0");

    // Somebody replaced it on disk, so that only the file can be what is shown.
    fs::write(rig.folder().join("description.md"), "From the folder.\n").unwrap();
    rig.type_text("d");

    assert!(rig.screen().contains("From the folder."));
}

#[test]
fn a_plugin_without_a_description_falls_back_to_its_readme() {
    let mut rig = rig(
        server_of("1.0.0", "", None, "", true, None),
        SavedState::default(),
        nothing,
    );
    rig.open_manager();
    rig.wait_for_text("+ Echo 1.0.0");
    rig.type_text("d");
    rig.wait_for_text("Says what it is told.");
}
