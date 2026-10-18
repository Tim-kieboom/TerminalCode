//! The real Git pane: the module built from the plugin's own repository
//! (<https://github.com/Tim-kieboom/trcode-plugin-git>), run by the editor against a
//! real repository. These tests need a checkout of it with the module built, so
//! they are ignored unless asked for:
//!
//! ```text
//! (in the plugin's repository) cargo build --target wasm32-unknown-unknown --release
//! TRCODE_PLUGIN_GIT_DIR=/path/to/trcode-plugin-git cargo test -p terminal_code real_plugin -- --ignored
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::event::action::Action;
use crate::persist::saved_state::{PluginRecord, SavedState};
use crate::ui::PluginViewId;
use crate::ui::layout::LayoutTree;
use crate::ui::view::ViewNode;

/// The checkout of the plugin's repository named by `TRCODE_PLUGIN_GIT_DIR`.
fn plugin_dir() -> PathBuf {
    std::env::var_os("TRCODE_PLUGIN_GIT_DIR")
        .map(PathBuf::from)
        .expect(
            "set TRCODE_PLUGIN_GIT_DIR to a checkout of trcode-plugin-git with the module built",
        )
}

fn manifest() -> String {
    let path = plugin_dir().join("manifest.toml");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn module() -> Vec<u8> {
    let path = plugin_dir().join("target/wasm32-unknown-unknown/release/trcode_plugin_git.wasm");
    fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "{}: {error}\nbuild it there with: cargo build --target wasm32-unknown-unknown --release",
            path.display()
        )
    })
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
    root: PathBuf,
    _dirs: (tempfile::TempDir, tempfile::TempDir),
}

/// A repository with `a.txt` changed and `b.txt` new, and the Git pane turned on.
fn rig() -> Rig {
    let project = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(project.path()).unwrap();
    fs::write(root.join("a.txt"), "alpha\n").unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "a.txt"]);
    git(&root, &["commit", "-q", "-m", "first"]);
    fs::write(root.join("a.txt"), "alpha changed\n").unwrap();
    fs::write(root.join("b.txt"), "beta\n").unwrap();

    let config = tempfile::tempdir().unwrap();
    let folder = config.path().join("plugins/git");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("manifest.toml"), manifest()).unwrap();
    fs::write(folder.join("git.wasm"), module()).unwrap();
    let mut saved = SavedState::default();
    saved.set_plugin(
        "git",
        PluginRecord {
            enabled: true,
            capabilities: ["git.read", "git.write", "git.remote", "editor.open"]
                .map(str::to_owned)
                .to_vec(),
            place: None,
        },
    );

    let mut state = AppState::new(Editor::new(Buffer::from_text("")));
    state.open_project(&root).unwrap();
    state.set_layout(
        LayoutTree::from_ron(r#"Row([Pane(view: Editor), Fixed(50, Pane(view: Plugin("git")))])"#)
            .unwrap(),
    );
    let (tx, events) = mpsc::channel(1024);
    let mut app = App::new(state)
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(saved)
        .with_events(tx);
    app.git_changed();
    Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(120, 24)).unwrap(),
        root,
        _dirs: (project, config),
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

    fn rows(&self) -> Vec<String> {
        let Some(view) = self.app.state().plugin_view(&PluginViewId::new("git")) else {
            return Vec::new();
        };
        // The files are the middle part of a column of buttons, files and the message.
        let ViewNode::Column(parts) = view.content() else {
            return Vec::new();
        };
        let Some(ViewNode::Rows { rows, .. }) = parts.get(1) else {
            return Vec::new();
        };
        rows.iter()
            .map(|row| row.spans.iter().map(|span| span.text.as_ref()).collect())
            .collect()
    }

    /// Whether the screen shows `text` now.
    fn screen_has(&self, text: &str) -> bool {
        let Some(view) = self.app.state().plugin_view(&PluginViewId::new("git")) else {
            return false;
        };
        let ViewNode::Column(parts) = view.content() else {
            return false;
        };
        match parts.first() {
            Some(ViewNode::Buttons(buttons)) => {
                buttons.iter().any(|button| button.label.as_ref() == text)
            }
            _ => false,
        }
    }

    /// The text of the commit message line.
    fn message(&self) -> String {
        let Some(view) = self.app.state().plugin_view(&PluginViewId::new("git")) else {
            return String::new();
        };
        let ViewNode::Column(parts) = view.content() else {
            return String::new();
        };
        match parts.get(2) {
            Some(ViewNode::Input { value, .. }) => value.to_string(),
            _ => String::new(),
        }
    }

    fn pump(&mut self) {
        if self
            .app
            .git_deadline()
            .is_some_and(|at| Instant::now() >= at)
        {
            self.app.git_due();
        }
        while let Ok(event) = self.events.try_recv() {
            self.app.handle_event(event);
        }
    }

    fn wait_until(&mut self, what: &str, done: impl Fn(&Rig) -> bool) {
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

    fn has_row(&self, text: &str) -> bool {
        self.rows().iter().any(|row| row == text)
    }

    fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn cached(&self) -> String {
        git(&self.root, &["diff", "--cached", "--name-only"])
            .trim()
            .to_owned()
    }
}

fn started() -> Rig {
    let mut rig = rig();
    rig.wait_until("the changes", |rig| rig.has_row("Changes (1)"));
    rig.app.run_action(Action::FocusPluginView("git".into()));
    rig
}

#[test]
#[ignore = "needs the module built for wasm32"]
fn the_pane_shows_the_branch_and_the_changed_files_in_groups() {
    let mut rig = rig();

    rig.wait_until("the files", |rig| {
        rig.has_row("M a.txt") && rig.has_row("U b.txt")
    });

    assert_eq!(
        rig.rows(),
        vec!["main", "Changes (1)", "M a.txt", "Untracked (1)", "U b.txt"]
    );
    let screen = rig.screen();
    assert!(
        screen.contains("Changes (1)") && screen.contains("U b.txt"),
        "{screen}"
    );
}

#[test]
#[ignore = "needs the module built for wasm32"]
fn s_stages_the_selected_file_and_u_unstages_it() {
    let mut rig = started();
    rig.wait_until("the file", |rig| rig.has_row("M a.txt"));

    rig.press(KeyCode::Char('s'));
    rig.wait_until("it to be staged", |rig| rig.has_row("Staged Changes (1)"));
    assert_eq!(rig.cached(), "a.txt");

    rig.press(KeyCode::Char('u'));
    rig.wait_until("it to be unstaged", |rig| {
        !rig.has_row("Staged Changes (1)")
    });
    assert_eq!(rig.cached(), "");
}

#[test]
#[ignore = "needs the module built for wasm32"]
fn down_and_enter_open_the_next_file_in_the_editor_and_the_keyboard_stays_in_the_pane() {
    let mut rig = started();
    rig.wait_until("the files", |rig| rig.has_row("U b.txt"));

    rig.press(KeyCode::Down);
    rig.press(KeyCode::Down);
    rig.press(KeyCode::Enter);

    rig.wait_until("the file to open", |rig| {
        rig.app
            .state()
            .editor()
            .buffer()
            .path()
            .is_some_and(|path| path.ends_with("b.txt"))
    });
    assert_eq!(rig.app.state().focus(), Focus::Plugin);
}

#[test]
#[ignore = "needs the module built for wasm32"]
fn esc_gives_the_keyboard_back_to_the_editor() {
    let mut rig = started();

    rig.press(KeyCode::Esc);

    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

#[test]
#[ignore = "needs the module built for wasm32"]
fn a_change_made_outside_shows_in_the_pane() {
    let mut rig = started();
    rig.wait_until("the files", |rig| rig.has_row("U b.txt"));

    fs::write(rig.root.join("c.txt"), "gamma\n").unwrap();
    rig.app
        .handle_event(Event::FilesChanged(vec![rig.root.clone()]));

    rig.wait_until("the new file", |rig| rig.has_row("Untracked (2)"));
}

#[test]
#[ignore = "needs the module built for wasm32"]
fn the_buttons_are_on_the_screen_and_wrap_in_a_narrow_pane() {
    let mut rig = started();

    let screen = rig.screen();

    for label in [
        "Commit",
        "Stage All",
        "Unstage All",
        "Fetch",
        "Pull",
        "Push",
    ] {
        assert!(screen.contains(label), "{label}\n{screen}");
    }
    assert!(screen.contains("Message:"), "{screen}");
}

#[test]
#[ignore = "needs the module built for wasm32"]
fn a_stages_everything_and_c_then_a_message_and_enter_commit_it() {
    let mut rig = started();
    rig.wait_until("the files", |rig| rig.has_row("U b.txt"));

    rig.press(KeyCode::Char('a'));
    rig.wait_until("everything to be staged", |rig| {
        rig.cached() == "a.txt\nb.txt"
    });

    rig.press(KeyCode::Char('c'));
    rig.wait_until("the message to take the keys", |rig| {
        rig.app.state().plugin_input()
    });
    for letter in "Add both".chars() {
        rig.press(KeyCode::Char(letter));
    }
    rig.wait_until("the text to show", |rig| rig.message() == "Add both");
    rig.press(KeyCode::Enter);

    rig.wait_until("the commit", |rig| {
        git(&rig.root, &["log", "--format=%s", "-1"]).trim() == "Add both"
    });
    assert_eq!(rig.cached(), "");
    rig.wait_until("the message to be cleared", |rig| rig.message().is_empty());
}

#[test]
#[ignore = "needs the module built for wasm32"]
fn shift_p_pushes_and_the_count_on_the_button_goes_away() {
    let mut rig = started();
    let server = tempfile::tempdir().unwrap();
    let bare = fs::canonicalize(server.path()).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    git(
        &rig.root,
        &["remote", "add", "origin", bare.to_str().unwrap()],
    );
    git(&rig.root, &["push", "-q", "-u", "origin", "main"]);
    git(&rig.root, &["commit", "-q", "-am", "local change"]);
    rig.app
        .handle_event(Event::FilesChanged(vec![rig.root.clone()]));
    rig.wait_until("a commit to push", |rig| rig.screen_has("Push (1)"));

    rig.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::SHIFT,
    )));

    rig.wait_until("the push", |_| {
        git(&bare, &["log", "--format=%s", "-1"]).trim() == "local change"
    });
    rig.wait_until("the count to go", |rig| !rig.screen_has("Push (1)"));
}
