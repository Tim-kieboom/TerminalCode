//! Completion as you type, from the keys to the box on the screen, against a
//! fake server that answers `textDocument/completion` with what a test gives it.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::app::App;
use crate::components::editor::Motion;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::initial_state;
use crate::lsp::Message;
use crate::lsp::tests::support::{FakeLauncher, Script};

type Answer = Arc<dyn Fn(&Value) -> Value + Send + Sync>;

/// A server that passes on what it is sent, and answers each completion request
/// with what `answer` makes of its params.
fn completing(sent: Sender<Message>, answer: Answer) -> Option<Script> {
    Some(Box::new(move |session| {
        while let Some(message) = session.next() {
            let request = match &message {
                Message::Request { id, method, params } if method == "textDocument/completion" => {
                    Some((id.clone(), params.clone()))
                }
                _ => None,
            };
            let _ = sent.send(message);
            if let Some((id, params)) = request {
                session.respond(id, answer(&params));
            }
        }
    }))
}

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    received: Receiver<Message>,
    _dir: tempfile::TempDir,
}

fn items(labels: &[&str]) -> Value {
    Value::Array(
        labels
            .iter()
            .map(|label| json!({ "label": label, "kind": 3, "detail": format!("fn {label}()") }))
            .collect(),
    )
}

fn answering(list: Value) -> Answer {
    Arc::new(move |_| list.clone())
}

fn provider() -> Value {
    json!({ "textDocumentSync": 2, "completionProvider": { "triggerCharacters": ["."] } })
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

impl Rig {
    /// An app over a Rust file with `text` in it, and a server with `capabilities`
    /// that answers with `answer`, running.
    fn start(capabilities: Value, text: &str, answer: Answer) -> Self {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Cargo.toml"), "[package]\n");
        let file = dir.path().join("src/main.rs");
        write(&file, text);
        let (sent, received) = std::sync::mpsc::channel();
        let launcher = FakeLauncher::with_capabilities(capabilities, move |_| {
            completing(sent.clone(), Arc::clone(&answer))
        });

        let mut state = initial_state(&[file]).unwrap();
        state.open_project(dir.path()).unwrap();
        let (tx, events) = mpsc::channel(256);
        let app = App::new(state)
            .with_events(tx)
            .with_lsp_launcher(launcher as _);
        let mut rig = Self {
            app,
            events,
            received,
            _dir: dir,
        };
        rig.app.handle_input(InputEvent::FocusLost);
        rig.pump_until(|app| app.state().lsp_status() == Some("rust-analyzer"));
        rig.app
            .state_mut()
            .edit(|editor| editor.move_cursor(Motion::DocumentEnd).unwrap());
        rig.frame();
        rig
    }

    fn with_answer(list: Value) -> Self {
        Self::start(provider(), "", answering(list))
    }

    /// What the loop does before each frame.
    fn frame(&mut self) {
        self.app.lsp_sync();
        self.app.completion_step();
    }

    /// Takes what the servers sent to the app until `done`.
    fn pump_until(&mut self, done: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            if done(&self.app) {
                return;
            }
            assert!(Instant::now() < deadline, "it never happened");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_box(&mut self) {
        self.pump_until(|app| app.state().completion_box().is_some());
    }

    fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    /// Types `text` one key at a time, with a frame after each, as the loop does.
    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c));
            self.frame();
        }
    }

    fn text(&self) -> String {
        self.app.state().editor().buffer().text()
    }

    /// The completion requests the server was sent so far.
    fn requests(&self) -> Vec<Value> {
        let mut found = Vec::new();
        while let Ok(message) = self.received.recv_timeout(Duration::from_millis(150)) {
            if let Message::Request { method, params, .. } = message
                && method == "textDocument/completion"
            {
                found.push(params);
            }
        }
        found
    }

    fn shown(&self) -> Vec<String> {
        self.app
            .state()
            .completion_box()
            .map(|open| {
                open.completion()
                    .items()
                    .map(|item| item.label.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn highlighted(&self) -> Option<String> {
        let open = self.app.state().completion_box()?;
        Some(open.completion().selected_item()?.label.clone())
    }

    fn screen(&mut self) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        let app = &mut self.app;
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        terminal.backend().to_string()
    }
}

// ---- asking, and showing what comes back

#[test]
fn typing_the_first_letter_of_a_word_asks_the_server_and_shows_its_answer() {
    let mut rig = Rig::with_answer(items(&["vec", "vector", "value"]));

    rig.type_text("v");
    rig.wait_for_box();

    let requests = rig.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["context"], json!({ "triggerKind": 1 }));
    assert_eq!(
        requests[0]["position"],
        json!({ "line": 0, "character": 1 })
    );
    // The server's order; `value` sorts by its label like the others.
    assert_eq!(rig.shown(), ["value", "vec", "vector"]);
    assert_eq!(rig.highlighted().as_deref(), Some("value"));
}

#[test]
fn the_box_is_drawn_under_the_cursor_with_kind_name_and_note() {
    let mut rig = Rig::with_answer(items(&["vec", "vector"]));
    rig.type_text("v");
    rig.wait_for_box();

    let screen = rig.screen();

    assert!(screen.contains("f vec"), "{screen}");
    assert!(screen.contains("fn vec()"), "{screen}");
    assert!(screen.contains("f vector"), "{screen}");
}

#[test]
fn only_one_request_goes_out_while_the_word_is_typed_before_the_answer() {
    let mut rig = Rig::with_answer(items(&["vec", "vector"]));

    rig.type_text("vec");
    rig.wait_for_box();

    assert_eq!(rig.requests().len(), 1);
    // The list is for `v` and was narrowed to `vec` as it arrived.
    assert_eq!(rig.shown(), ["vec", "vector"]);
}

#[test]
fn a_complete_list_is_narrowed_as_more_is_typed_without_asking_again() {
    let mut rig = Rig::with_answer(items(&["vec", "vector", "value"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.type_text("ec");

    assert_eq!(rig.shown(), ["vec", "vector"]);
    assert_eq!(rig.requests().len(), 1);
}

#[test]
fn an_incomplete_list_is_asked_for_again_when_the_word_grows() {
    let incomplete = json!({ "isIncomplete": true, "items": items(&["vec", "vector"]) });
    let mut rig = Rig::with_answer(incomplete);
    rig.type_text("v");
    rig.wait_for_box();

    rig.type_text("e");
    rig.pump_until(|_| true);

    let requests = rig.requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert_eq!(requests[1]["context"], json!({ "triggerKind": 3 }));
    assert_eq!(
        requests[1]["position"],
        json!({ "line": 0, "character": 2 })
    );
}

#[test]
fn a_trigger_character_asks_for_everything_that_may_follow_it() {
    let mut rig = Rig::with_answer(items(&["push", "pop"]));

    rig.type_text(".");
    rig.wait_for_box();

    let requests = rig.requests();
    assert_eq!(
        requests.last().unwrap()["context"],
        json!({ "triggerKind": 2, "triggerCharacter": "." })
    );
    // Nothing is typed of the word yet and the box is there all the same.
    assert_eq!(rig.shown(), ["pop", "push"]);
}

#[test]
fn a_word_that_starts_with_a_digit_asks_nothing() {
    let mut rig = Rig::with_answer(items(&["vec"]));

    rig.type_text("12");

    assert!(rig.requests().is_empty());
    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn a_server_that_offers_no_completion_is_not_asked() {
    let mut rig = Rig::start(
        json!({ "textDocumentSync": 2 }),
        "",
        answering(items(&["vec"])),
    );

    rig.type_text("v");

    assert!(rig.requests().is_empty());
    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn text_that_was_not_typed_asks_nothing() {
    let mut rig = Rig::with_answer(items(&["vec"]));

    // A paste (here, the editor's own insert, not a key) is not typing.
    rig.app
        .state_mut()
        .edit(|editor| editor.insert_text("vec").unwrap());
    rig.frame();

    assert!(rig.requests().is_empty());
}

// ---- when the box goes away

#[test]
fn a_letter_nothing_matches_closes_the_box() {
    let mut rig = Rig::with_answer(items(&["vec", "vector"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.type_text("x");

    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn a_character_that_is_not_part_of_a_word_closes_the_box() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.type_text(" ");

    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn an_answer_for_a_word_that_is_over_is_dropped() {
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let wait = std::sync::Mutex::new(wait);
    let answer: Answer = Arc::new(move |_| {
        let _ = wait.lock().unwrap().recv();
        items(&["vec"])
    });
    let mut rig = Rig::start(provider(), "", answer);
    rig.type_text("v");
    rig.type_text(" ");

    release.send(()).unwrap();
    rig.pump_until(|_| true);
    std::thread::sleep(Duration::from_millis(200));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }

    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn backspacing_to_the_start_of_the_word_closes_a_box_that_a_trigger_did_not_open() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.press(KeyCode::Backspace);
    rig.frame();

    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn a_click_closes_the_box() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.app
        .handle_input(InputEvent::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 50,
            row: 10,
            modifiers: KeyModifiers::NONE,
        }));

    assert!(rig.app.state().completion_box().is_none());
}

// ---- the keys

#[test]
fn tab_writes_the_highlighted_suggestion_in_place_of_the_word() {
    let mut rig = Rig::with_answer(items(&["vec", "vector"]));
    rig.type_text("ve");
    rig.wait_for_box();

    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "vec");
    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn accepting_does_not_open_the_box_again() {
    let mut rig = Rig::with_answer(items(&["vec", "vector"]));
    rig.type_text("ve");
    rig.wait_for_box();
    rig.requests();

    rig.press(KeyCode::Tab);
    rig.frame();

    assert!(rig.app.state().completion_box().is_none());
    assert!(rig.requests().is_empty());
}

#[test]
fn one_undo_takes_the_written_suggestion_back_to_what_was_typed() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("ve");
    rig.wait_for_box();
    rig.press(KeyCode::Tab);

    rig.app.state_mut().edit(|editor| editor.undo().unwrap());

    assert_eq!(rig.text(), "ve");
}

#[test]
fn up_and_down_move_the_highlight_round_the_list() {
    let mut rig = Rig::with_answer(items(&["va", "vb", "vc"]));
    rig.type_text("v");
    rig.wait_for_box();
    assert_eq!(rig.highlighted().as_deref(), Some("va"));

    rig.press(KeyCode::Down);
    assert_eq!(rig.highlighted().as_deref(), Some("vb"));
    rig.press(KeyCode::Up);
    rig.press(KeyCode::Up);
    assert_eq!(rig.highlighted().as_deref(), Some("vc"));
    // The cursor did not move.
    assert_eq!(rig.text(), "v");
    rig.press(KeyCode::Down);
    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "va");
}

#[test]
fn escape_closes_the_box_and_leaves_the_text() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.press(KeyCode::Esc);

    assert!(rig.app.state().completion_box().is_none());
    assert_eq!(rig.text(), "v");
}

#[test]
fn enter_closes_the_box_and_is_a_newline() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.press(KeyCode::Enter);

    assert!(rig.app.state().completion_box().is_none());
    assert_eq!(rig.text(), "v\n");
}

#[test]
fn without_a_box_tab_indents_as_always() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("x");
    rig.pump_until(|_| true);
    assert!(rig.app.state().completion_box().is_none() || rig.shown().is_empty());
    rig.press(KeyCode::Esc);

    rig.press(KeyCode::Tab);

    assert!(rig.text().len() > 1, "{:?}", rig.text());
    assert_ne!(rig.text(), "xvec");
}

// ---- the import that goes with a suggestion

#[test]
fn a_suggestion_with_an_import_writes_both_and_one_undo_takes_both_back() {
    let answer: Answer = Arc::new(|_| {
        json!([{
            "label": "HashMap",
            "kind": 22,
            "textEdit": {
                "range": { "start": { "line": 1, "character": 11 }, "end": { "line": 1, "character": 15 } },
                "newText": "HashMap",
            },
            "additionalTextEdits": [{
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
                "newText": "use std::collections::HashMap;\n",
            }],
        }])
    });
    let source = "fn f() {\n    let m: Has";
    let mut rig = Rig::start(provider(), source, answer);

    rig.type_text("h");
    rig.wait_for_box();
    rig.press(KeyCode::Tab);

    assert_eq!(
        rig.text(),
        "use std::collections::HashMap;\nfn f() {\n    let m: HashMap"
    );
    rig.app.state_mut().edit(|editor| editor.undo().unwrap());
    assert_eq!(rig.text(), format!("{source}h"));
}
