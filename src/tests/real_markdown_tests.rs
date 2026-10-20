//! The real markdown preview: the module built from the plugin's own repository
//! (<https://github.com/Tim-kieboom/trcode-plugin-markdown>), run by the editor. These
//! tests need a checkout of it with the module built, so they are ignored unless
//! asked for:
//!
//! ```text
//! (in the plugin's repository) cargo build --target wasm32-unknown-unknown --release
//! TRCODE_PLUGIN_MARKDOWN_DIR=/path/to/trcode-plugin-markdown cargo test -p terminal_code real_markdown -- --ignored --nocapture
//! ```
//!
//! One of them measures how much fuel the module spends on a document, which is where
//! the rate of `FUEL_PER_BYTE` comes from (3 times the worst measured).

use std::fs;
use std::path::PathBuf;

use crate::plugin_api::{MAX_FILE_TEXT, ToPlugin};
use crate::plugins::{CALLBACK_FUEL, FUEL_PER_BYTE, fuel_for, measure_fuel};

fn plugin_dir() -> PathBuf {
    std::env::var_os("TRCODE_PLUGIN_MARKDOWN_DIR")
        .map(PathBuf::from)
        .expect("set TRCODE_PLUGIN_MARKDOWN_DIR to a checkout of trcode-plugin-markdown with the module built")
}

fn module() -> Vec<u8> {
    let path =
        plugin_dir().join("target/wasm32-unknown-unknown/release/trcode_plugin_markdown.wasm");
    fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "{}: {error}\nbuild it there with: cargo build --target wasm32-unknown-unknown --release",
            path.display()
        )
    })
}

/// A block with everything that costs something: wrapped prose with styles and links,
/// nested lists, a quote, a table and code.
const BLOCK: &str = "## A section of the document\n\nA paragraph with *emphasis*, **strong text**, `inline code`, a [link](docs/a.md#part) and ~~struck~~ words, long enough that it has to wrap over several lines of a narrow pane, with more words after that, and more, and more.\n\n- an item with a [link](a.md)\n  - a nested **bold** item\n    1. a numbered `code` item\n    2. and one more item with some words in it\n- [x] a task that is done\n\n> a quote with a [link](b.md) and some more text that goes on for a while so that it wraps\n\n| name | quantity | where |\n|:-----|---------:|-------|\n| apple | 3 | [shelf](a.md) |\n| fig | 12 | `box` |\n| cherry | 7 | bag |\n| plum | 100 | [crate](c.md) |\n\n```rust\nfn main() {\n    let answer = 42;\n    println!(\"{answer}\");\n}\n```\n\n---\n\n";

/// Markdown of about `bytes` bytes, made of `BLOCK`.
fn document(bytes: usize) -> String {
    let mut text = String::new();
    let mut n = 0;
    while text.len() + BLOCK.len() < bytes {
        n += 1;
        text.push_str(&BLOCK.replace("A section", &format!("Section {n}:")));
    }
    text
}

fn file_text(text: String) -> ToPlugin {
    ToPlugin::FileText {
        path: "docs/big.md".to_owned(),
        text,
        modified: false,
    }
}

/// What the module spends on each message after the document: a key, a new width (which
/// draws the document again) and a redraw.
struct Spent {
    name: String,
    size: usize,
    text: u64,
    key: u64,
    resize: u64,
    redraw: u64,
}

/// The fuel the module spends on each document and on what follows it, with a budget so big
/// that nothing is cut short.
fn measure(documents: Vec<(&str, String)>) -> Vec<Spent> {
    let wasm = module();
    documents
        .into_iter()
        .map(|(name, text)| {
            let size = text.len();
            let used = measure_fuel(
                &wasm,
                u64::MAX / 4,
                vec![
                    ToPlugin::Size {
                        width: 80,
                        height: 40,
                    },
                    file_text(text),
                    ToPlugin::Action {
                        action: "down".to_owned(),
                    },
                    ToPlugin::Size {
                        width: 60,
                        height: 40,
                    },
                    ToPlugin::Refresh,
                ],
            );
            let fuel = |at: usize| used[at].clone().expect("the module answers");
            Spent {
                name: name.to_owned(),
                size,
                text: fuel(1),
                key: fuel(2),
                resize: fuel(3),
                redraw: fuel(4),
            }
        })
        .collect()
}

/// The most text the plugin renders (`MAX_PREVIEW_BYTES` there).
const PLUGIN_LIMIT: usize = 128 * 1024;

#[test]
#[ignore = "needs a checkout of the plugin with the module built (TRCODE_PLUGIN_MARKDOWN_DIR)"]
fn real_markdown_fuel_per_document_is_measured_and_has_headroom() {
    let readme = fs::read_to_string(plugin_dir().join("README.md")).unwrap();
    let editor_readme =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap();
    let measured = measure(vec![
        ("tiny", "a".to_owned()),
        ("the plugin's README", readme),
        ("the editor's README", editor_readme),
        ("100 KB of everything at once", document(100 * 1024)),
        ("128 KB, the most it renders", document(PLUGIN_LIMIT - 1)),
    ]);
    let mut worst_per_byte = 0.0f64;
    for spent in &measured {
        // Drawing the document again for a new width costs about what the text did.
        let per_byte = spent.text.max(spent.resize) as f64 / spent.size as f64;
        // A document of a few bytes is mostly the cost of starting.
        if spent.size > 1000 {
            worst_per_byte = worst_per_byte.max(per_byte);
        }
        println!(
            "{}: {} bytes; the text {} fuel ({per_byte:.0} per byte); a key {}, a new width {}, a redraw {}",
            spent.name, spent.size, spent.text, spent.key, spent.resize, spent.redraw
        );
    }
    println!(
        "worst: {worst_per_byte:.0} per byte; 3 times that is {:.0}; the rate in use is {FUEL_PER_BYTE}",
        3.0 * worst_per_byte
    );
    for spent in measured {
        let held = spent.size as u64;
        let budget = |message: &ToPlugin| fuel_for(message, true, held);
        let text = file_text("x".repeat(spent.size));
        let key = ToPlugin::Action {
            action: "down".to_owned(),
        };
        for (what, used, message) in [
            ("the text", spent.text, &text),
            ("a key", spent.key, &key),
            ("a new width", spent.resize, &key),
            ("a redraw", spent.redraw, &key),
        ] {
            assert!(
                3 * used <= budget(message),
                "{}: 3 times {used} fuel for {what} is more than the budget of {}",
                spent.name,
                budget(message)
            );
        }
        // The flat budget is for a message that does not depend on the document: a key must
        // not cost what a re-parse costs.
        assert!(
            spent.key < CALLBACK_FUEL || spent.size < 1000,
            "{}: a key costs {} fuel",
            spent.name,
            spent.key
        );
    }
}

#[test]
#[ignore = "needs a checkout of the plugin with the module built (TRCODE_PLUGIN_MARKDOWN_DIR)"]
fn real_markdown_a_file_over_its_limit_costs_next_to_nothing() {
    // The editor sends up to 1 MB; the plugin says it is too large without parsing it.
    let measured = measure(vec![(
        "1 MB, the most the editor sends",
        document(MAX_FILE_TEXT - 1),
    )]);
    let spent = &measured[0];
    println!("{} bytes cost {} fuel", spent.size, spent.text);
    assert!(spent.text < CALLBACK_FUEL * 20, "{}", spent.text);
}

// --- the real module in the editor ---

use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::AppState;
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::event::action::Action;
use crate::persist::saved_state::{Place, PluginRecord, SavedState};
use crate::ui::layout::LayoutTree;

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
    root: PathBuf,
    _dirs: (tempfile::TempDir, tempfile::TempDir),
}

impl Rig {
    fn screen(&mut self) -> String {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        self.terminal.backend().to_string()
    }

    fn wait_until(&mut self, what: &str, done: impl Fn(&mut Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_for_text(&mut self, text: &str) {
        let text = text.to_owned();
        self.wait_until(&format!("`{text}` on the screen"), move |rig| {
            rig.screen().contains(&text)
        });
    }

    fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn open(&mut self, name: &str) {
        let path = self.root.join(name);
        self.app
            .state_mut()
            .workspace_mut()
            .open_path(&path)
            .unwrap();
        self.app.handle_input(InputEvent::Resize(140, 30));
    }
}

fn start() -> Rig {
    let project = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(project.path()).unwrap();
    fs::write(
        root.join("a.md"),
        "# Alpha\n\nSee [the b file](b.md#second) for more.\n",
    )
    .unwrap();
    fs::write(
        root.join("b.md"),
        "# Beta\n\ntext\n\n## Second\n\nmore words\n",
    )
    .unwrap();
    let plugin = config.path().join("plugins/markdown");
    fs::create_dir_all(&plugin).unwrap();
    for file in ["manifest.toml", "description.md"] {
        fs::copy(plugin_dir().join(file), plugin.join(file)).unwrap();
    }
    fs::write(plugin.join("markdown.wasm"), module()).unwrap();

    let mut saved = SavedState::default();
    saved.set_plugin(
        "markdown",
        PluginRecord {
            enabled: true,
            capabilities: vec!["files.text".to_owned(), "editor.open".to_owned()],
            file_types: vec!["md".to_owned(), "markdown".to_owned()],
            own_thread: true,
            place: Some(Place::Side),
        },
    );
    let mut state = AppState::new(Editor::new(Buffer::from_text("")));
    state.set_layout(
        LayoutTree::from_ron(
            "Col([Row([Fixed(24, SideBar(views: [Explorer])), Col([Pane(view: Editor), \
             Fixed(6, BottomBar(views: [Terminal]))])])])",
        )
        .unwrap(),
    );
    state.open_project(&root).unwrap();
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state)
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(saved)
        .with_events(tx);
    Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(140, 30)).unwrap(),
        root,
        _dirs: (project, config),
    }
}

#[test]
#[ignore = "needs a checkout of the plugin with the module built (TRCODE_PLUGIN_MARKDOWN_DIR)"]
fn real_markdown_preview_sits_beside_the_editor_follows_the_file_and_opens_links() {
    let mut rig = start();
    rig.wait_until("the preview's pane", |rig| {
        rig.screen().contains("Markdown preview")
    });

    rig.open("a.md");
    rig.wait_for_text("See the b file for more.");
    let screen = rig.screen();
    let column = |needle: &str| {
        screen
            .lines()
            .find_map(|line| line.find(needle).map(|at| line[..at].chars().count()))
            .unwrap_or_else(|| panic!("`{needle}` is not on the screen\n{screen}"))
    };
    // The preview's text is right of the editor's, whose first line is the markdown.
    assert!(column("# Alpha") < column("See the b file"), "{screen}");
    assert!(
        screen.contains("a.md"),
        "the preview names the file\n{screen}"
    );

    // Follow the link: the keyboard goes to the pane, tab finds the link, enter opens it.
    rig.app
        .run_action(Action::FocusPluginView("markdown".into()));
    rig.press(KeyCode::Tab);
    rig.press(KeyCode::Enter);
    rig.wait_until("b.md in front", |rig| {
        rig.app
            .state()
            .editor()
            .buffer()
            .path()
            .is_some_and(|path| path.ends_with("b.md"))
    });
    // And the preview follows it, to the heading the link names.
    rig.wait_for_text("more words");
    assert!(rig.screen().contains("b.md"));
}
