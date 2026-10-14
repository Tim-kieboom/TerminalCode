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

pub(super) type Answer = Arc<dyn Fn(&Value) -> Value + Send + Sync>;

/// A server that passes on what it is sent, and answers each completion request
/// with what `answer` makes of its params.
pub(super) fn completing(sent: Sender<Message>, answer: Answer) -> Option<Script> {
    Some(Box::new(move |session| {
        while let Some(message) = session.next() {
            let request = match &message {
                Message::Request { id, method, params } => {
                    Some((id.clone(), method.clone(), params.clone()))
                }
                _ => None,
            };
            let _ = sent.send(message);
            match request {
                Some((id, method, params))
                    if method == "textDocument/completion"
                        || method == "textDocument/hover"
                        || method == "textDocument/definition" =>
                {
                    session.respond(id, answer(&params));
                }
                // The item comes back with the edits its `data` says it needs, or
                // the server refuses when `data` says so.
                Some((id, method, params)) if method == "completionItem/resolve" => {
                    let data = &params["data"];
                    if data["fail"] == json!(true) {
                        session.respond_error(id, -32803, "no");
                    } else {
                        let mut item = params.clone();
                        item["additionalTextEdits"] = data["resolve_edits"].clone();
                        session.respond(id, item);
                    }
                }
                _ => {}
            }
        }
    }))
}

pub(super) struct Rig {
    pub(super) app: App,
    pub(super) events: mpsc::Receiver<Event>,
    pub(super) received: Receiver<Message>,
    pub(super) _dir: tempfile::TempDir,
}

fn items(labels: &[&str]) -> Value {
    Value::Array(
        labels
            .iter()
            .map(|label| json!({ "label": label, "kind": 3, "detail": format!("fn {label}()") }))
            .collect(),
    )
}

pub(super) fn answering(list: Value) -> Answer {
    Arc::new(move |_| list.clone())
}

pub(super) fn provider() -> Value {
    json!({ "textDocumentSync": 2, "completionProvider": { "triggerCharacters": ["."] } })
}

pub(super) fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

impl Rig {
    /// An app over a Rust file with `text` in it, and a server with `capabilities`
    /// that answers with `answer`, running.
    pub(super) fn start(capabilities: Value, text: &str, answer: Answer) -> Self {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Cargo.toml"), "[package]\n");
        let file = dir.path().join("src/main.rs");
        write(&file, text);
        let (sent, received) = std::sync::mpsc::channel();
        let launcher = FakeLauncher::with_capabilities(capabilities, move |_| {
            completing(sent.clone(), Arc::clone(&answer))
        });
        Self::with_launcher(dir, file, launcher, received)
    }

    /// An app over `file` in the project `dir`, with servers started by `launcher`,
    /// once the server runs; the cursor is at the end of the text.
    pub(super) fn with_launcher(
        dir: tempfile::TempDir,
        file: std::path::PathBuf,
        launcher: Arc<dyn crate::lsp::Launcher>,
        received: Receiver<Message>,
    ) -> Self {
        let mut state = initial_state(&[file]).unwrap();
        state.open_project(dir.path()).unwrap();
        let (tx, events) = mpsc::channel(256);
        let app = App::new(state).with_events(tx).with_lsp_launcher(launcher);
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

    pub(super) fn with_answer(list: Value) -> Self {
        Self::start(provider(), "", answering(list))
    }

    /// What the loop does before each frame.
    pub(super) fn frame(&mut self) {
        self.app.lsp_sync();
        self.app.completion_step();
    }

    /// Takes what the servers sent to the app until `done`.
    pub(super) fn pump_until(&mut self, done: impl Fn(&App) -> bool) {
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

    pub(super) fn wait_for_box(&mut self) {
        self.pump_until(|app| app.state().completion_box().is_some());
    }

    pub(super) fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    /// Types `text` one key at a time, with a frame after each, as the loop does.
    pub(super) fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c));
            self.frame();
        }
    }

    pub(super) fn text(&self) -> String {
        self.app.state().editor().buffer().text()
    }

    /// The completion requests the server was sent so far.
    pub(super) fn requests(&self) -> Vec<Value> {
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

    pub(super) fn shown(&self) -> Vec<String> {
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

    pub(super) fn highlighted(&self) -> Option<String> {
        let open = self.app.state().completion_box()?;
        Some(open.completion().selected_item()?.label.clone())
    }

    pub(super) fn screen(&mut self) -> String {
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

// ---- the import the server only works out when the suggestion is picked

fn import_edit() -> Value {
    json!({
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
        "newText": "use std::collections::HashMap;\n",
    })
}

fn hash_map_with_data(data: Value) -> Answer {
    Arc::new(move |_| {
        json!([{
            "label": "HashMap",
            "kind": 22,
            "data": data.clone(),
            "textEdit": {
                "range": { "start": { "line": 1, "character": 11 }, "end": { "line": 1, "character": 15 } },
                "newText": "HashMap",
            },
        }])
    })
}

const HASH_SOURCE: &str = "fn f() {\n    let m: Has";

/// Every message the server has been sent so far, in order.
fn everything_sent(rig: &Rig) -> Vec<Message> {
    let mut all = Vec::new();
    while let Ok(message) = rig.received.recv_timeout(Duration::from_millis(150)) {
        all.push(message);
    }
    all
}

#[test]
fn an_item_with_data_is_resolved_when_picked_and_its_import_goes_in_with_the_word() {
    let data = json!({ "resolve_edits": [import_edit()] });
    let mut rig = Rig::start(provider(), HASH_SOURCE, hash_map_with_data(data));
    rig.type_text("h");
    rig.wait_for_box();
    everything_sent(&rig);

    rig.press(KeyCode::Tab);

    assert_eq!(
        rig.text(),
        "use std::collections::HashMap;\nfn f() {\n    let m: HashMap"
    );
    let sent = everything_sent(&rig);
    let resolve = sent
        .iter()
        .position(|message| matches!(message, Message::Request { method, .. } if method == "completionItem/resolve"))
        .expect("the item was resolved");
    // The server knew the text as it was before it was asked.
    let changed = sent
        .iter()
        .position(|message| matches!(message, Message::Notification { method, .. } if method == "textDocument/didChange"));
    assert!(changed.is_none() || changed < Some(resolve), "{sent:?}");
    let Message::Request { params, .. } = &sent[resolve] else {
        unreachable!()
    };
    assert_eq!(params["label"], json!("HashMap"));
    rig.app.state_mut().edit(|editor| editor.undo().unwrap());
    assert_eq!(rig.text(), format!("{HASH_SOURCE}h"));
}

#[test]
fn a_server_that_refuses_to_resolve_gets_the_word_written_without_the_import_and_the_user_is_told()
{
    let data = json!({ "fail": true });
    let mut rig = Rig::start(provider(), HASH_SOURCE, hash_map_with_data(data));
    rig.type_text("h");
    rig.wait_for_box();

    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "fn f() {\n    let m: HashMap");
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("did not say what to import"), "{told}");
}

#[test]
fn an_item_without_data_is_not_resolved_when_the_server_does_not_resolve() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("ve");
    rig.wait_for_box();
    everything_sent(&rig);

    rig.press(KeyCode::Tab);

    let sent = everything_sent(&rig);
    assert!(
        !sent.iter().any(|message| matches!(message, Message::Request { method, .. } if method == "completionItem/resolve")),
        "{sent:?}"
    );
}

#[test]
fn a_server_that_says_it_resolves_has_every_item_resolved() {
    let capabilities = json!({
        "textDocumentSync": 2,
        "completionProvider": { "resolveProvider": true },
    });
    let mut rig = Rig::start(capabilities, "", answering(items(&["vec"])));
    rig.type_text("ve");
    rig.wait_for_box();
    everything_sent(&rig);

    rig.press(KeyCode::Tab);

    let sent = everything_sent(&rig);
    assert!(
        sent.iter().any(|message| matches!(message, Message::Request { method, .. } if method == "completionItem/resolve")),
        "{sent:?}"
    );
    assert_eq!(rig.text(), "vec");
}

// ---- calls get their brackets, and the documentation shows

fn call_item(label: &str, snippet: &str, documentation: Option<&str>) -> Value {
    let mut item = json!({
        "label": label,
        "kind": 2,
        "detail": format!("fn {label}()"),
        "insertTextFormat": 2,
        "insertText": snippet,
    });
    if let Some(docs) = documentation {
        item["documentation"] = json!({ "kind": "markdown", "value": docs });
    }
    item
}

fn cursor_column(rig: &Rig) -> usize {
    rig.app
        .state()
        .editor()
        .selections()
        .primary()
        .head()
        .column
}

#[test]
fn a_call_with_an_argument_is_written_with_the_placeholder_selected() {
    let mut rig = Rig::with_answer(json!([call_item("push", "push(${1:value})", None)]));
    rig.type_text("pu");
    rig.wait_for_box();

    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "push(value)");
    let selection = rig.app.state().editor().selections().primary().clone();
    assert_eq!(
        (selection.anchor().column, selection.head().column),
        (5, 10)
    );
}

#[test]
fn a_call_that_takes_nothing_leaves_the_cursor_after_the_brackets() {
    let mut rig = Rig::with_answer(json!([call_item("len", "len()$0", None)]));
    rig.type_text("le");
    rig.wait_for_box();

    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "len()");
    assert_eq!(cursor_column(&rig), 5);
}

#[test]
fn typing_replaces_the_selected_placeholder_after_picking() {
    let mut rig = Rig::with_answer(json!([call_item("push", "push(${1:value})", None)]));
    rig.type_text("pu");
    rig.wait_for_box();
    rig.press(KeyCode::Tab);

    rig.press(KeyCode::Char('x'));

    assert_eq!(rig.text(), "push(x)");
}

#[test]
fn deleting_the_selected_placeholder_leaves_empty_brackets() {
    let mut rig = Rig::with_answer(json!([call_item("push", "push(${1:value})", None)]));
    rig.type_text("pu");
    rig.wait_for_box();
    rig.press(KeyCode::Tab);

    rig.press(KeyCode::Backspace);

    assert_eq!(rig.text(), "push()");
}

#[test]
fn the_documentation_of_the_highlighted_item_shows_above_the_list_as_plain_text() {
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n",
        answering(json!([
            call_item(
                "pa",
                "pa($0)",
                Some("Adds **one** `item`.\n\nPanics: never.")
            ),
            call_item("pb", "pb()$0", Some("Removes the last item.")),
        ])),
    );
    rig.type_text("p");
    rig.wait_for_box();

    let screen = rig.screen();

    assert!(screen.contains("Adds one item."), "{screen}");
    assert!(screen.contains("Panics: never."), "{screen}");
    assert!(!screen.contains("**"), "{screen}");
    let rows: Vec<&str> = screen.lines().collect();
    let docs_row = rows
        .iter()
        .position(|row| row.contains("Adds one item."))
        .unwrap();
    let list_row = rows.iter().position(|row| row.contains("m pa")).unwrap();
    assert!(docs_row < list_row, "{screen}");
}

#[test]
fn moving_the_highlight_changes_the_documentation() {
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n",
        answering(json!([
            call_item("pop", "pop()$0", Some("Removes the last item.")),
            call_item("push", "push($0)", Some("Adds an item.")),
        ])),
    );
    rig.type_text("p");
    rig.wait_for_box();
    assert!(rig.screen().contains("Removes the last item."));

    rig.press(KeyCode::Down);
    let screen = rig.screen();

    assert!(screen.contains("Adds an item."), "{screen}");
    assert!(!screen.contains("Removes the last item."), "{screen}");
}

#[test]
fn an_item_without_documentation_has_no_documentation_box() {
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n",
        answering(json!([call_item("pop", "pop()$0", None)])),
    );
    rig.type_text("p");
    rig.wait_for_box();

    let screen = rig.screen();

    // Only the list is drawn: one box of three rows.
    assert_eq!(screen.matches('╭').count(), 1, "{screen}");
}

#[test]
fn the_documentation_shows_eight_lines_and_says_how_many_there_are() {
    let long: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n\n\n\n\n\n\n",
        answering(json!([call_item("pop", "pop()$0", Some(&long.join("\n")))])),
    );
    rig.type_text("p");
    rig.wait_for_box();

    let screen = rig.screen();

    assert!(screen.contains("line 1 "), "{screen}");
    assert!(!screen.contains("line 9"), "{screen}");
    // Which lines show, of how many.
    assert!(screen.contains("1-8/20"), "{screen}");
}

#[test]
fn near_the_top_of_the_screen_the_documentation_goes_under_the_list() {
    let mut rig = Rig::start(
        provider(),
        "",
        answering(json!([call_item(
            "pop",
            "pop()$0",
            Some("Removes the last item.")
        )])),
    );
    rig.type_text("p");
    rig.wait_for_box();

    let screen = rig.screen();

    let rows: Vec<&str> = screen.lines().collect();
    let docs = rows
        .iter()
        .position(|row| row.contains("Removes the last item."))
        .unwrap();
    let list = rows.iter().position(|row| row.contains("m pop")).unwrap();
    assert!(docs > list, "{screen}");
}

#[test]
fn with_room_for_only_part_of_the_documentation_it_shows_what_fits() {
    let long: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    // The cursor is in the middle of the 20 rows, so neither side has ten rows.
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n\n\n",
        answering(json!([call_item("pop", "pop()$0", Some(&long.join("\n")))])),
    );
    // The row for the menu icon is not what is being measured.
    rig.app
        .state_mut()
        .set_shown(&crate::components::ComponentKind::MenuBar, false);
    rig.type_text("p");
    rig.wait_for_box();

    let screen = rig.screen();

    assert!(screen.contains("line 1 "), "{screen}");
    assert!(screen.contains("/20 "), "{screen}");
    assert!(!screen.contains("line 7"), "{screen}");
}

// ---- scrolling the documentation

fn long_docs() -> Value {
    let long: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    json!([call_item("pop", "pop()$0", Some(&long.join("\n")))])
}

fn wheel(rig: &mut Rig, kind: crossterm::event::MouseEventKind, column: u16, row: u16) {
    rig.app
        .handle_input(InputEvent::Mouse(crossterm::event::MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
}

/// The screen cell where `needle` starts.
fn cell_of(rig: &mut Rig, needle: &str) -> (u16, u16) {
    let screen = rig.screen();
    for (row, line) in screen.lines().enumerate() {
        let line = line.trim_matches('"');
        if let Some(byte) = line.find(needle) {
            return (line[..byte].chars().count() as u16, row as u16);
        }
    }
    panic!("the screen does not show {needle:?}: {screen}");
}

fn docs_rig() -> Rig {
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n\n\n\n\n\n\n\n",
        answering(long_docs()),
    );
    rig.type_text("p");
    rig.wait_for_box();
    rig
}

#[test]
fn the_wheel_over_the_documentation_scrolls_it_and_not_the_editor() {
    let mut rig = docs_rig();
    let (column, row) = cell_of(&mut rig, "line 2");
    let top_before = rig.app.state().editor().scroll().top;

    wheel(
        &mut rig,
        crossterm::event::MouseEventKind::ScrollDown,
        column,
        row,
    );

    let screen = rig.screen();
    assert!(screen.contains("line 4 "), "{screen}");
    assert!(!screen.contains("line 1 "), "{screen}");
    assert!(screen.contains("4-11/20"), "{screen}");
    assert_eq!(rig.app.state().editor().scroll().top, top_before);
    assert!(rig.app.state().completion_box().is_some());

    wheel(
        &mut rig,
        crossterm::event::MouseEventKind::ScrollUp,
        column,
        row,
    );
    assert!(rig.screen().contains("line 1 "));
}

#[test]
fn the_documentation_stops_at_the_last_page() {
    let mut rig = docs_rig();
    let (column, row) = cell_of(&mut rig, "line 2");

    for _ in 0..20 {
        wheel(
            &mut rig,
            crossterm::event::MouseEventKind::ScrollDown,
            column,
            row,
        );
    }

    let screen = rig.screen();
    assert!(screen.contains("line 20"), "{screen}");
    assert!(screen.contains("13-20/20"), "{screen}");
}

#[test]
fn the_page_keys_scroll_the_documentation_while_the_suggestions_are_open() {
    let mut rig = docs_rig();
    let cursor = rig.app.state().editor().selections().primary().head();
    // The box is known to be drawn after a frame.
    rig.screen();

    rig.press(KeyCode::PageDown);

    let screen = rig.screen();
    assert!(screen.contains("line 9 "), "{screen}");
    assert!(screen.contains("9-16/20"), "{screen}");
    assert!(rig.app.state().completion_box().is_some());
    assert_eq!(
        rig.app.state().editor().selections().primary().head(),
        cursor
    );
    rig.press(KeyCode::PageUp);
    assert!(rig.screen().contains("line 1 "));
}

#[test]
fn the_page_keys_are_the_editors_when_the_documentation_fits() {
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n",
        answering(json!([call_item("pop", "pop()$0", Some("Short."))])),
    );
    rig.type_text("p");
    rig.wait_for_box();
    rig.screen();

    rig.press(KeyCode::PageUp);
    rig.frame();

    // The cursor went up, out of the word: the box is gone.
    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn another_highlighted_item_starts_its_documentation_at_the_top() {
    let long: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    let text = long.join("\n");
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n\n\n\n\n\n\n\n",
        answering(json!([
            call_item("pa", "pa()$0", Some(&text)),
            call_item("pb", "pb()$0", Some(&text)),
        ])),
    );
    rig.type_text("p");
    rig.wait_for_box();
    rig.screen();
    rig.press(KeyCode::PageDown);
    assert!(rig.screen().contains("9-16/20"));

    rig.press(KeyCode::Down);

    let screen = rig.screen();
    assert!(screen.contains("1-8/20"), "{screen}");
    assert!(screen.contains("line 1 "), "{screen}");
}

// ---- the proof: the real rust-analyzer through the whole app

#[cfg(unix)]
impl Rig {
    /// An app over a temporary crate whose `src/main.rs` holds `text`, with the
    /// real `rust-analyzer`.
    pub(super) fn real(text: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("Cargo.toml"),
            "[package]\nname = \"small\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        let file = dir.path().join("src/main.rs");
        write(&file, text);
        let (_, received) = std::sync::mpsc::channel();
        Self::with_launcher(dir, file, Arc::new(crate::lsp::ProcessLauncher), received)
    }

    /// Types `text` and waits for the box to offer `wanted`. The server may still
    /// be indexing and answer with nothing at first, so a try that comes to
    /// nothing is taken back and made again, for up to two minutes.
    pub(super) fn type_until_offered(&mut self, text: &str, wanted: &str) {
        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline {
            self.type_text(text);
            let tried = Instant::now() + Duration::from_secs(15);
            while Instant::now() < tried {
                while let Ok(event) = self.events.try_recv() {
                    self.app.handle_event(event);
                }
                self.frame();
                if self.shown().iter().any(|label| label.starts_with(wanted)) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            for _ in text.chars() {
                self.press(KeyCode::Backspace);
                self.frame();
            }
            self.app.state_mut().notify("retrying");
        }
        panic!(
            "{wanted:?} was never offered; the box shows {:?}, status {:?}",
            self.shown(),
            self.app.state().lsp_status()
        );
    }

    /// Moves the highlight to the item called `label`.
    pub(super) fn highlight(&mut self, label: &str) {
        for _ in 0..self.shown().len() {
            if self
                .highlighted()
                .is_some_and(|shown| shown.starts_with(label))
            {
                return;
            }
            self.press(KeyCode::Down);
        }
        panic!("{label:?} is not among {:?}", self.shown());
    }
}

/// `Vec::ne` shows `new` and `Tab` writes it. Run by hand, since CI has no
/// `rust-analyzer`:
/// `cargo test real_rust_analyzer_offers_new -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_offers_new_after_vec_colons_ne_and_tab_writes_it() {
    let mut rig = Rig::real("fn main() {\n    let v: Vec<u32> = Vec::");

    rig.type_until_offered("ne", "new");
    eprintln!("offered: {:?}", rig.shown());
    eprintln!("{}", rig.screen());
    rig.highlight("new");
    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "fn main() {\n    let v: Vec<u32> = Vec::new()");
    assert!(rig.app.state().completion_box().is_none());
}

/// `HashMap` + `Tab` adds the `use` line, and one undo takes both away. Run by
/// hand: `cargo test real_rust_analyzer_adds_the_use -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_adds_the_use_line_for_hashmap_and_one_undo_takes_both_away() {
    let source = "fn main() {\n    let m: Hash";
    let mut rig = Rig::real(source);

    rig.type_until_offered("M", "HashMap");
    eprintln!("offered: {:?}", rig.shown());
    eprintln!("{}", rig.screen());
    rig.highlight("HashMap");
    rig.press(KeyCode::Tab);

    let written = rig.text();
    eprintln!("written:\n{written}");
    assert!(
        written.starts_with("use std::collections::HashMap;\n"),
        "{written}"
    );
    assert!(written.ends_with("let m: HashMap<>"), "{written}");

    rig.app.state_mut().edit(|editor| editor.undo().unwrap());

    assert_eq!(rig.text(), format!("{source}M"));
}

/// A method call gets its brackets with the argument selected, and the
/// documentation of the highlighted method shows. Run by hand:
/// `cargo test real_rust_analyzer_writes_the_brackets -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_writes_the_brackets_of_a_call_and_shows_its_documentation() {
    let source = "fn main() {\n\n\n\n\n    let mut v: Vec<u32> = Vec::new();\n    v.pu";
    let mut rig = Rig::real(source);

    rig.type_until_offered("s", "push");
    rig.highlight("push");
    let screen = rig.screen();
    eprintln!("{screen}");
    assert!(
        screen.contains("Appends an element"),
        "the documentation of push shows: {screen}"
    );
    rig.press(KeyCode::Tab);

    let text = rig.text();
    // rust-analyzer adds the `;` itself when the call is a statement.
    assert!(text.ends_with("v.push(value);"), "{text}");
    let selection = rig.app.state().editor().selections().primary().clone();
    assert_eq!(
        (selection.anchor().column, selection.head().column),
        ("    v.push(".len(), "    v.push(value".len())
    );
    rig.press(KeyCode::Char('7'));
    assert!(rig.text().ends_with("v.push(7);"), "{}", rig.text());
}
