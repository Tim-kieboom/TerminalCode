//! The text of the file in front, pushed to a plugin with `files.text`: only the kinds
//! its manifest names and the user left on, the saved text on disk, at most 1 MB, and
//! asked about before the plugin runs.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Popup};
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::event::Event;
use crate::persist::saved_state::{PluginRecord, SavedState};
use crate::plugins::fixtures::{Echo, First};
use crate::ui::PluginViewId;
use crate::ui::layout::LayoutTree;
use crate::ui::view::ViewNode;

const LAYOUT: &str = r#"Row([Pane(view: Editor), Fixed(60, Pane(view: Plugin("hello")))])"#;

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    root: PathBuf,
    _dirs: (tempfile::TempDir, tempfile::TempDir),
    _terminal: Terminal<TestBackend>,
}

fn manifest(extra: &str) -> String {
    format!(
        "id = \"hello\"\nname = \"Hello\"\nversion = \"0.1.0\"\nabi = \"1.5\"\nwasm = \"plugin.wasm\"\n{extra}"
    )
}

fn record(file_types: &[&str]) -> PluginRecord {
    PluginRecord {
        enabled: true,
        capabilities: vec!["files.text".to_owned()],
        place: None,
        file_types: file_types.iter().map(|kind| (*kind).to_owned()).collect(),
        own_thread: false,
    }
}

/// A project with some files, and the plugin `hello` running with `manifest_extra`.
fn rig(manifest_extra: &str, saved: SavedState) -> Rig {
    let project = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(project.path()).unwrap();
    fs::write(root.join("a.md"), "# Alpha\n").unwrap();
    fs::write(root.join("b.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join(".env"), "SECRET=1\n").unwrap();
    let plugin = config.path().join("plugins/hello");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(plugin.join("manifest.toml"), manifest(manifest_extra)).unwrap();
    fs::write(plugin.join("plugin.wasm"), Echo::new(First::Echo).build()).unwrap();

    let mut state = AppState::new(Editor::new(Buffer::from_text("")));
    state.set_layout(LayoutTree::from_ron(LAYOUT).unwrap());
    state.open_project(&root).unwrap();
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state)
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(saved)
        .with_events(tx);
    Rig {
        app,
        events,
        root,
        _dirs: (project, config),
        _terminal: Terminal::new(TestBackend::new(120, 24)).unwrap(),
    }
}

fn saved(record: PluginRecord) -> SavedState {
    let mut saved = SavedState::default();
    saved.set_plugin("hello", record);
    saved
}

/// The plugin that wants `.md` files, turned on and allowed.
fn md_rig() -> Rig {
    let mut rig = rig(
        "capabilities = [\"files.text\"]\nfile_types = [\"md\"]\n",
        saved(record(&["md"])),
    );
    rig.wait_until("the plugin", |rig| rig.text().is_some());
    rig
}

impl Rig {
    fn pump(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.app.handle_event(event);
        }
    }

    fn wait_until(&mut self, what: &str, done: impl Fn(&mut Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.pump();
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// What the plugin was last sent, as it echoed it.
    fn text(&self) -> Option<String> {
        let view = self.app.state().plugin_view(&PluginViewId::new("hello"))?;
        match view.content() {
            ViewNode::Lines(lines) => lines.first().map(|line| line.text.to_string()),
            _ => None,
        }
    }

    fn open(&mut self, name: &str) {
        let path = self.root.join(name);
        self.app
            .state_mut()
            .workspace_mut()
            .open_path(&path)
            .unwrap();
        self.tick();
    }

    /// An input that changes nothing: the app looks at the file in front after it.
    fn tick(&mut self) {
        self.app.handle_input(InputEvent::Resize(120, 24));
    }

    fn wait_for(&mut self, needle: &str) {
        let needle = needle.to_owned();
        self.wait_until(&format!("the plugin to be sent `{needle}`"), move |rig| {
            rig.text().is_some_and(|text| text.contains(&needle))
        });
    }

    /// The plugin is sent nothing more for a moment.
    fn stays_at(&mut self, needle: &str) {
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            self.pump();
            std::thread::sleep(Duration::from_millis(10));
        }
        let text = self.text().unwrap_or_default();
        assert!(text.contains(needle), "{text}");
    }
}

#[test]
fn a_file_of_the_kind_in_front_is_sent_as_saved_with_its_path_in_the_project() {
    let mut rig = md_rig();
    rig.open("a.md");

    rig.wait_for("file_text");
    let text = rig.text().unwrap();
    assert!(text.contains(r#""path":"a.md""#), "{text}");
    assert!(text.contains(r"# Alpha\n"), "{text}");
    assert!(text.contains(r#""modified":false"#), "{text}");
}

#[test]
fn another_kind_in_front_clears_and_is_never_sent() {
    let mut rig = md_rig();
    rig.open("a.md");
    rig.wait_for("file_text");

    rig.open("b.rs");
    rig.wait_for("file_cleared");
    rig.open(".env");
    rig.stays_at("file_cleared");
    assert!(!rig.text().unwrap().contains("SECRET"));
}

#[test]
fn the_text_is_the_file_on_disk_and_modified_says_the_buffer_has_more() {
    let mut rig = md_rig();
    rig.open("a.md");
    rig.wait_for("# Alpha");

    // Typing changes the buffer only: the plugin is told so and is sent the saved text.
    rig.app
        .state_mut()
        .edit(|editor| editor.insert_text("unsaved ").unwrap());
    rig.tick();
    rig.wait_for(r#""modified":true"#);
    assert!(!rig.text().unwrap().contains("unsaved"));

    // The file changes on disk: the new text is sent.
    fs::write(rig.root.join("a.md"), "# Beta, longer\n").unwrap();
    rig.tick();
    rig.wait_for("# Beta, longer");
}

#[test]
fn the_user_can_narrow_the_kinds_never_widen_them() {
    let mut rig = md_rig();
    rig.app
        .settings_mut()
        .plugin_file_types
        .insert("hello".to_owned(), vec!["txt".to_owned()]);
    rig.open("a.md");
    // `txt` is not in the manifest and `md` is left out by the setting: nothing matches.
    rig.wait_for("file_cleared");

    rig.app
        .settings_mut()
        .plugin_file_types
        .insert("hello".to_owned(), vec!["md".to_owned()]);
    rig.tick();
    rig.wait_for("file_text");
}

#[test]
fn a_manifest_that_names_no_kind_gets_no_file() {
    let mut rig = rig("capabilities = [\"files.text\"]\n", saved(record(&[])));
    rig.wait_until("the plugin", |rig| rig.text().is_some());
    rig.open("a.md");
    rig.wait_for("file_cleared");
}

#[test]
fn a_plugin_without_the_capability_is_sent_nothing() {
    let mut rig = rig(
        "file_types = [\"md\"]\n",
        saved(PluginRecord {
            enabled: true,
            capabilities: Vec::new(),
            place: None,
            file_types: Vec::new(),
            own_thread: false,
        }),
    );
    rig.wait_until("the plugin", |rig| rig.text().is_some());
    rig.open("a.md");
    rig.stays_at("start");
}

#[test]
fn a_file_over_a_megabyte_is_told_as_too_large_not_sent() {
    let mut rig = md_rig();
    fs::write(rig.root.join("big.md"), "x".repeat(1024 * 1024 + 1)).unwrap();
    rig.open("big.md");

    rig.wait_for("file_too_large");
    let text = rig.text().unwrap();
    assert!(text.contains(r#""path":"big.md""#), "{text}");
    assert!(text.contains("1048577"), "{text}");
}

#[test]
fn the_question_names_the_kinds_and_a_wider_manifest_asks_again() {
    // Never turned on: the question says what it would see.
    let mut first = rig(
        "capabilities = [\"files.text\"]\nfile_types = [\"md\", \"markdown\"]\n",
        SavedState::default(),
    );
    first
        .app
        .run_action(crate::event::action::Action::ShowPlugins);
    first.pump();
    first.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));
    assert!(matches!(first.app.state().popup(), Popup::Confirm(_)));
    let question = first.app.state().confirm().unwrap().text();
    assert!(
        question.contains("see the text of the .md, .markdown files you open"),
        "{question}"
    );

    // Allowed for `md` only, and the manifest now says `md` and `txt`: asked again.
    let widened = rig(
        "capabilities = [\"files.text\"]\nfile_types = [\"md\", \"txt\"]\n",
        saved(record(&["md"])),
    );
    assert!(matches!(widened.app.state().popup(), Popup::Confirm(_)));
}

#[test]
fn a_thread_of_its_own_is_asked_about_and_asked_again_when_it_comes_with_an_update() {
    let manifest = "capabilities = [\"files.text\"]\nfile_types = [\"md\"]\nthread = \"own\"\n";
    // Allowed before, but only what ran on the shared thread: asked again.
    let rig_without = rig(manifest, saved(record(&["md"])));
    assert!(matches!(rig_without.app.state().popup(), Popup::Confirm(_)));
    let question = rig_without.app.state().confirm().unwrap().text();
    assert!(
        question.contains("run on a thread of its own"),
        "{question}"
    );
    assert!(question.contains("processor time"), "{question}");

    // Allowed with it: it just runs, on its own thread, and is sent the file.
    let mut allowed = rig(
        manifest,
        saved(PluginRecord {
            own_thread: true,
            ..record(&["md"])
        }),
    );
    allowed.wait_until("the plugin", |rig| rig.text().is_some());
    allowed.open("a.md");
    allowed.wait_for("file_text");
}
