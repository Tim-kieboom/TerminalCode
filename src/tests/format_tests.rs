//! Format on save and "Format Document", against a fake server that answers
//! formatting requests with the edits a test gives it.

use std::fs;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::completion_tests::{Answer, Rig, completing, write};
use crate::components::editor::Motion;
use crate::lsp::test_support::FakeLauncher;
use crate::lsp::{LaunchSpec, Launcher, Message, Session, StartError};

const SOURCE: &str = "fn main(){}\n";
const FORMATTED: &str = "fn main() {}\n";

fn provider() -> Value {
    json!({ "textDocumentSync": 2, "documentFormattingProvider": true })
}

/// The edit that makes `SOURCE` into `FORMATTED`.
fn format_edit() -> Value {
    json!([{
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 11 },
        },
        "newText": "fn main() {}",
    }])
}

fn formatting(answer: Value) -> Answer {
    Arc::new(move |_| answer.clone())
}

fn format_to_the_standard() -> Answer {
    formatting(format_edit())
}

fn silent() -> Answer {
    formatting(json!({ "__silent": true }))
}

impl Rig {
    fn main_path(&self) -> std::path::PathBuf {
        self._dir.path().join("src/main.rs")
    }

    fn on_disk(&self) -> String {
        fs::read_to_string(self.main_path()).unwrap()
    }

    fn ctrl(&mut self, c: char) {
        self.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::CONTROL,
        )));
    }

    fn format_document(&mut self) {
        self.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char('f'),
            KeyModifiers::ALT | KeyModifiers::SHIFT,
        )));
    }

    /// `ctrl+k a`.
    fn save_all(&mut self) {
        self.ctrl('k');
        self.press(KeyCode::Char('a'));
    }

    fn format_on_save(mut self) -> Self {
        self.app.settings_mut().format_on_save = true;
        self
    }

    fn typed(&mut self, text: &str) {
        self.app
            .state_mut()
            .edit(|editor| editor.insert_text(text).unwrap());
    }

    /// Like `wait_until_saved`, with the clock of the loop ticking: the deadlines
    /// of the formatting only pass when the loop looks at them.
    fn tick_until_saved(&mut self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while self.app.state().editor().buffer().is_dirty() {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            self.app.formatting_step();
            assert!(std::time::Instant::now() < deadline, "it never happened");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_until_saved(&mut self) {
        self.pump_until(|app| !app.state().editor().buffer().is_dirty());
    }

    fn formatting_requests(&self) -> Vec<Value> {
        let mut found = Vec::new();
        while let Ok(message) = self.received.recv_timeout(Duration::from_millis(150)) {
            if let Message::Request { method, params, .. } = message
                && method == "textDocument/formatting"
            {
                found.push(params);
            }
        }
        found
    }

    /// An app over `SOURCE` with a comment line added at the end: a dirty buffer.
    fn dirty(capabilities: Value, answer: Answer) -> Self {
        let mut rig = Rig::start(capabilities, SOURCE, answer).format_on_save();
        rig.typed("//\n");
        rig
    }
}

#[test]
fn with_the_setting_off_saving_writes_the_text_as_it_is() {
    let mut rig = Rig::start(provider(), SOURCE, format_to_the_standard());
    rig.typed("//\n");

    rig.ctrl('s');

    assert_eq!(rig.on_disk(), format!("{SOURCE}//\n"));
    assert!(rig.formatting_requests().is_empty());
}

#[test]
fn saving_formats_first_and_writes_the_formatted_text_as_one_undo_step() {
    let mut rig = Rig::start(provider(), SOURCE, format_to_the_standard()).format_on_save();
    // Dirty, with the text as it was: undoing back still counts as modified.
    rig.typed("x");
    rig.app.state_mut().edit(|editor| editor.undo().unwrap());
    assert_eq!(rig.text(), SOURCE);
    assert!(rig.app.state().editor().buffer().is_dirty());

    rig.ctrl('s');
    rig.wait_until_saved();

    assert_eq!(rig.on_disk(), FORMATTED);
    assert_eq!(rig.text(), FORMATTED);
    let told = rig.app.state().notifications().texts().join("|");
    assert!(
        told.contains("saved main.rs") && !told.contains("without"),
        "{told}"
    );
    rig.app.state_mut().edit(|editor| editor.undo().unwrap());
    assert_eq!(rig.text(), SOURCE, "the formatting was one undo step");
}

#[test]
fn nothing_to_change_saves_the_text_and_leaves_the_undo_history_alone() {
    let mut rig = Rig::dirty(provider(), formatting(json!([])));

    rig.ctrl('s');
    rig.wait_until_saved();

    assert_eq!(rig.on_disk(), format!("{SOURCE}//\n"));
    assert_eq!(rig.formatting_requests().len(), 1);
}

#[test]
fn a_buffer_that_changed_while_formatting_is_saved_as_typed_with_the_reason() {
    let mut rig = Rig::dirty(provider(), format_to_the_standard());
    rig.ctrl('s');
    // The answer is on its way; this keystroke comes first.
    rig.typed("x");

    rig.wait_until_saved();

    assert_eq!(rig.on_disk(), format!("{SOURCE}//\nx"));
    let told = rig.app.state().notifications().texts().join("|");
    assert!(
        told.contains("without formatting: the file changed while formatting"),
        "{told}"
    );
}

#[test]
fn an_error_of_the_server_saves_unformatted_and_says_why() {
    let mut rig = Rig::dirty(
        provider(),
        formatting(json!({ "__error": "rustfmt failed" })),
    );

    rig.ctrl('s');
    rig.wait_until_saved();

    assert_eq!(rig.on_disk(), format!("{SOURCE}//\n"));
    let told = rig.app.state().notifications().texts().join("|");
    assert!(
        told.contains("without formatting") && told.contains("rustfmt failed"),
        "{told}"
    );
}

#[test]
fn an_answer_that_never_comes_saves_unformatted_when_the_wait_ends() {
    let mut rig = Rig::dirty(provider(), silent());
    rig.app
        .set_format_patience(Duration::from_secs(10), Duration::from_millis(150));

    rig.ctrl('s');
    assert!(
        rig.app.state().editor().buffer().is_dirty(),
        "not yet written"
    );
    rig.tick_until_saved();

    assert_eq!(rig.on_disk(), format!("{SOURCE}//\n"));
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("did not answer in time"), "{told}");
}

#[test]
fn a_file_with_no_server_is_saved_at_once() {
    let mut rig = Rig::start(provider(), SOURCE, format_to_the_standard()).format_on_save();
    let notes = rig._dir.path().join("notes.txt");
    write(&notes, "a\n");
    rig.app
        .state_mut()
        .workspace_mut()
        .open_path(&notes)
        .unwrap();
    rig.typed("b");

    rig.ctrl('s');

    assert_eq!(fs::read_to_string(&notes).unwrap(), "ba\n");
    assert!(
        !rig.app.state().editor().buffer().is_dirty(),
        "saved on the spot"
    );
}

#[test]
fn a_server_that_does_not_format_saves_at_once_and_says_so_once() {
    let mut rig = Rig::start(
        json!({ "textDocumentSync": 2 }),
        SOURCE,
        format_to_the_standard(),
    )
    .format_on_save();
    rig.typed("//\n");

    rig.ctrl('s');

    assert_eq!(rig.on_disk(), format!("{SOURCE}//\n"));
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("does not format"), "{told}");
}

#[test]
fn format_document_formats_without_saving() {
    let mut rig = Rig::start(provider(), SOURCE, format_to_the_standard());
    rig.format_document();
    rig.pump_until(|app| app.state().editor().buffer().text() == FORMATTED);

    assert_eq!(rig.on_disk(), SOURCE, "not saved");
    assert!(rig.app.state().editor().buffer().is_dirty());
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("formatted main.rs"), "{told}");
}

#[test]
fn format_document_on_a_formatted_file_says_so() {
    let mut rig = Rig::start(provider(), SOURCE, formatting(json!([])));
    rig.format_document();
    rig.pump_until(|app| {
        app.state()
            .notifications()
            .texts()
            .iter()
            .any(|told| told.contains("already formatted"))
    });
}

// ---- quitting while a save waits

#[test]
fn quitting_waits_for_the_formatting_and_the_save() {
    let mut rig = Rig::dirty(provider(), format_to_the_standard());
    rig.ctrl('s');

    rig.ctrl('q');
    assert!(!rig.app.should_quit(), "the save is not done yet");
    rig.pump_until(|app| app.should_quit());

    assert_eq!(rig.on_disk(), format!("{FORMATTED}//\n"));
}

#[test]
fn a_second_quit_skips_the_formatting_and_saves_as_it_is() {
    let mut rig = Rig::dirty(provider(), silent());
    rig.ctrl('s');

    rig.ctrl('q');
    assert!(!rig.app.should_quit());
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("ctrl+q again"), "{told}");
    rig.ctrl('q');

    assert!(rig.app.should_quit());
    assert_eq!(rig.on_disk(), format!("{SOURCE}//\n"));
}

#[test]
fn the_quit_prompt_saves_through_the_formatter_and_quits_when_done() {
    let mut rig = Rig::dirty(provider(), format_to_the_standard());
    rig.ctrl('q');
    assert!(
        rig.app.state().quit_prompt().is_some(),
        "unsaved changes: asked"
    );

    // `s`: save all.
    rig.press(KeyCode::Char('s'));
    assert!(!rig.app.should_quit(), "the formatting is still running");
    rig.pump_until(|app| app.should_quit());

    assert_eq!(rig.on_disk(), format!("{FORMATTED}//\n"));
}

// ---- several files, and a server that is not ready

#[test]
fn save_all_asks_for_every_formatting_before_any_answer_and_saves_them_all() {
    let mut rig = Rig::start(provider(), SOURCE, format_to_the_standard()).format_on_save();
    let others: Vec<_> = ["a.rs", "b.rs"]
        .iter()
        .map(|name| {
            let path = rig._dir.path().join("src").join(name);
            write(&path, "fn main(){}\n");
            path
        })
        .collect();
    for path in &others {
        rig.app.state_mut().workspace_mut().open_path(path).unwrap();
        rig.frame();
        rig.app
            .state_mut()
            .edit(|editor| editor.move_cursor(Motion::DocumentEnd).unwrap());
        rig.typed("//\n");
    }
    let main = rig.main_path();
    rig.app
        .state_mut()
        .workspace_mut()
        .open_path(&main)
        .unwrap();
    rig.typed("//\n");
    rig.frame();

    rig.ctrl('s');
    rig.save_all();
    // Every request is out before one answer was taken.
    let asked = rig.formatting_requests();
    assert!(asked.len() >= 3, "{asked:?}");
    rig.pump_until(|app| app.state().workspace().dirty_documents().is_empty());

    for path in others.iter().chain([&rig.main_path()]) {
        let text = fs::read_to_string(path).unwrap();
        assert!(text.starts_with(FORMATTED), "{path:?}: {text}");
    }
}

/// Starts the server only when the test lets it, as a server that takes a while.
struct Gated {
    inner: Arc<FakeLauncher>,
    gate: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl Launcher for Gated {
    fn launch(
        &self,
        spec: &LaunchSpec,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Session, StartError> {
        let waiting = self.gate.lock().unwrap();
        let _ = waiting.recv_timeout(Duration::from_secs(20));
        drop(waiting);
        self.inner.launch(spec, wake)
    }
}

fn slow_start(answer: Answer) -> (Rig, Sender<()>) {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("Cargo.toml"), "[package]\n");
    let file = dir.path().join("src/main.rs");
    write(&file, SOURCE);
    let (sent, received) = channel();
    let inner = FakeLauncher::with_capabilities(provider(), move |_| {
        completing(sent.clone(), Arc::clone(&answer))
    });
    let (open, gate) = channel();
    let launcher = Arc::new(Gated {
        inner,
        gate: Mutex::new(gate),
    });
    let mut rig = Rig::with_launcher_not_waiting(dir, file, launcher, received).format_on_save();
    rig.frame();
    rig.app
        .state_mut()
        .edit(|editor| editor.move_cursor(Motion::DocumentEnd).unwrap());
    rig.typed("//\n");
    (rig, open)
}

#[test]
fn a_save_waits_for_a_server_that_is_starting_and_then_formats() {
    let (mut rig, open) = slow_start(format_to_the_standard());

    rig.ctrl('s');
    assert!(
        rig.app.state().editor().buffer().is_dirty(),
        "waiting for the server"
    );
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("waiting for the language server"), "{told}");

    open.send(()).unwrap();
    rig.wait_until_saved();

    assert_eq!(rig.on_disk(), format!("{FORMATTED}//\n"));
}

#[test]
fn a_server_that_is_not_ready_in_time_leaves_the_file_saved_unformatted() {
    let (mut rig, open) = slow_start(format_to_the_standard());
    rig.app
        .set_format_patience(Duration::from_millis(200), Duration::from_secs(2));

    rig.ctrl('s');
    rig.tick_until_saved();

    assert_eq!(rig.on_disk(), format!("{SOURCE}//\n"));
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("the language server was not ready"), "{told}");
    // Lets the server thread end.
    let _ = open.send(());
}

#[test]
fn the_palette_lists_format_document() {
    assert!(
        crate::event::action::Action::palette_actions()
            .iter()
            .any(|action| action.title() == Some("Language: Format Document"))
    );
}

/// Against the real `rust-analyzer` (and `rustfmt`), which have to be on the PATH:
/// `cargo test real_rust_analyzer_formats -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "needs rust-analyzer and rustfmt on the PATH; run by hand"]
fn real_rust_analyzer_formats_before_the_file_is_saved() {
    let mut rig = Rig::real("fn   main( ){ let x=1;\n}\n").format_on_save();
    rig.app
        .set_format_patience(Duration::from_secs(60), Duration::from_secs(20));
    rig.typed("// end\n");

    rig.ctrl('s');
    rig.tick_until_saved();

    let on_disk = rig.on_disk();
    eprintln!("{on_disk}");
    assert!(
        on_disk.starts_with("fn main() {\n    let x = 1;\n}\n"),
        "{on_disk}"
    );
    assert_eq!(rig.text(), on_disk);
}
