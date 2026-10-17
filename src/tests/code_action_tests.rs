//! Code actions, from `ctrl+.` to the edit made, against a fake server that
//! answers with the actions a test gives it.

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::completion_tests::{Answer, Rig};
use crate::buffer::Position;
use crate::lsp::Message;
use crate::lsp::Severity;

const SOURCE: &str = "fn main() {\n    let m = HashMap::new();\n}\n";
const IMPORT: &str = "use std::collections::HashMap;\n";

fn provider() -> Value {
    json!({
        "textDocumentSync": 2,
        "codeActionProvider": { "resolveProvider": true },
        "executeCommandProvider": { "commands": ["fix.all"] },
    })
}

fn insert_at_top(text: &str) -> Value {
    json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 0 },
        },
        "newText": text,
    })
}

/// An edit that inserts `text` at the top of the file with this URI.
fn edit_of(uri: &str, text: &str) -> Value {
    json!({ "changes": { uri: [insert_at_top(text)] } })
}

fn sibling(uri: &str, name: &str) -> String {
    uri.replace("main.rs", name)
}

/// Answers a question about actions with `actions(uri)`; other requests
/// (`resolve`, `executeCommand`) with `other`.
fn offering(
    actions: impl Fn(&str) -> Value + Send + Sync + 'static,
    other: impl Fn(&Value) -> Value + Send + Sync + 'static,
) -> Answer {
    Arc::new(move |params| match params.get("context") {
        Some(_) => actions(params["textDocument"]["uri"].as_str().unwrap()),
        None => other(params),
    })
}

fn only(actions: impl Fn(&str) -> Value + Send + Sync + 'static) -> Answer {
    offering(actions, |_| Value::Null)
}

fn import_action(uri: &str) -> Value {
    json!({ "title": "Import `HashMap`", "kind": "quickfix", "edit": edit_of(uri, IMPORT) })
}

impl Rig {
    /// The cursor on `HashMap`, then `ctrl+.`.
    fn ctrl_dot(&mut self) {
        self.app
            .state_mut()
            .edit(|editor| editor.go_to(Position::new(1, 14)));
        self.screen();
        self.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char('.'),
            KeyModifiers::CONTROL,
        )));
    }

    fn wait_for_list(&mut self) {
        self.pump_until(|app| app.state().action_list().is_some());
    }

    /// Every request and notification the server was sent so far, in order.
    fn everything(&self) -> Vec<(String, Value)> {
        let mut found = Vec::new();
        while let Ok(message) = self.received.recv_timeout(Duration::from_millis(150)) {
            match message {
                Message::Request { method, params, .. }
                | Message::Notification { method, params } => found.push((method, params)),
                Message::Response { .. } => {}
            }
        }
        found
    }

    fn titles(&self) -> Vec<String> {
        self.app
            .state()
            .action_list()
            .map(|list| list.entries().iter().map(|e| e.title.clone()).collect())
            .unwrap_or_default()
    }

    fn main_text(&self) -> String {
        self.app.state().editor().buffer().text()
    }
}

fn rig(answer: Answer) -> Rig {
    Rig::start(provider(), SOURCE, answer)
}

fn methods(sent: &[(String, Value)], wanted: &str) -> usize {
    sent.iter().filter(|(method, _)| method == wanted).count()
}

#[test]
fn ctrl_dot_lists_the_actions_and_enter_makes_the_edit_as_one_undo_step() {
    let mut rig = rig(only(|uri| json!([import_action(uri)])));

    rig.ctrl_dot();
    rig.wait_for_list();
    assert_eq!(rig.titles(), ["Import `HashMap`"]);
    rig.press(KeyCode::Enter);

    assert_eq!(rig.main_text(), format!("{IMPORT}{SOURCE}"));
    assert!(rig.app.state().action_list().is_none());
    assert!(
        rig.said().contains("Import `HashMap`: 1 place in 1 file"),
        "{}",
        rig.said()
    );
    rig.app.state_mut().edit(|editor| editor.undo().unwrap());
    assert_eq!(rig.main_text(), SOURCE, "one undo takes it back");
}

#[test]
fn the_request_carries_the_problems_that_touch_the_cursor() {
    let mut rig = rig(only(|uri| json!([import_action(uri)])));
    let document = rig.app.state().workspace().active_document_id().unwrap();
    // `HashMap` is bytes 20..27 of the file.
    let near = crate::lsp::Diagnostic {
        range: 20..27,
        severity: Severity::Error,
        message: "cannot find `HashMap`".to_owned(),
        source: Some("rustc".to_owned()),
        code: Some("E0433".to_owned()),
    };
    let far = crate::lsp::Diagnostic {
        range: 0..2,
        severity: Severity::Warning,
        message: "elsewhere".to_owned(),
        source: None,
        code: None,
    };
    rig.app
        .state_mut()
        .diagnostics_mut()
        .set(document, vec![far, near]);

    rig.ctrl_dot();
    rig.wait_for_list();

    let sent = rig.everything();
    let (_, params) = sent
        .iter()
        .find(|(method, _)| method == "textDocument/codeAction")
        .unwrap();
    let problems = params["context"]["diagnostics"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0]["message"], json!("cannot find `HashMap`"));
    assert_eq!(problems[0]["code"], json!("E0433"));
    assert_eq!(
        params["range"]["start"],
        json!({ "line": 1, "character": 14 })
    );
}

#[test]
fn a_number_picks_that_action() {
    let mut rig = rig(only(|uri| {
        json!([
            { "title": "First", "edit": edit_of(uri, "// first\n") },
            { "title": "Second", "edit": edit_of(uri, "// second\n") },
        ])
    }));

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Char('2'));

    assert!(rig.main_text().starts_with("// second\n"));
}

#[test]
fn down_and_enter_pick_the_next_and_escape_closes_with_no_change() {
    let mut rig = rig(only(|uri| {
        json!([
            { "title": "First", "edit": edit_of(uri, "// first\n") },
            { "title": "Second", "edit": edit_of(uri, "// second\n") },
        ])
    }));

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Esc);
    assert!(rig.app.state().action_list().is_none());
    assert_eq!(rig.main_text(), SOURCE);

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Down);
    rig.press(KeyCode::Enter);
    assert!(rig.main_text().starts_with("// second\n"));
}

#[test]
fn no_actions_is_a_message_and_no_box() {
    let mut rig = rig(only(|_| json!([])));

    rig.ctrl_dot();
    rig.wait_for_notice("no code actions here");

    assert!(rig.app.state().action_list().is_none());
}

#[test]
fn a_disabled_action_is_not_offered() {
    let mut rig = rig(only(|uri| {
        json!([
            { "title": "Not now", "disabled": { "reason": "no" }, "edit": edit_of(uri, "x") },
            import_action(uri),
        ])
    }));

    rig.ctrl_dot();
    rig.wait_for_list();

    assert_eq!(rig.titles(), ["Import `HashMap`"]);
}

#[test]
fn a_server_without_code_actions_says_so() {
    let mut rig = Rig::start(
        json!({ "textDocumentSync": 2 }),
        SOURCE,
        Arc::new(|_| Value::Null),
    );

    rig.ctrl_dot();

    assert!(
        rig.said()
            .contains("no language server offers code actions"),
        "{}",
        rig.said()
    );
}

#[test]
fn an_action_without_its_edit_is_resolved_when_picked() {
    // The server keeps what it needs for the edit in `data`, as real ones do.
    let answer = offering(
        |uri| json!([{ "title": "Add arms", "kind": "quickfix", "data": { "uri": uri } }]),
        |params| {
            let mut resolved = params.clone();
            let uri = params["data"]["uri"].as_str().unwrap().to_owned();
            resolved["edit"] = edit_of(&uri, "// arms\n");
            resolved
        },
    );
    let mut rig = rig(answer);

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Enter);
    rig.pump_until(|app| {
        app.state()
            .editor()
            .buffer()
            .text()
            .starts_with("// arms\n")
    });

    let sent = rig.everything();
    assert_eq!(methods(&sent, "codeAction/resolve"), 1);
}

#[test]
fn a_command_only_action_runs_the_command() {
    let answer = offering(
        |_| {
            json!([{
                "title": "Fix all",
                "command": { "title": "Fix", "command": "fix.all", "arguments": [1] },
            }])
        },
        |_| Value::Null,
    );
    let mut rig = rig(answer);

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Enter);
    rig.pump_until(|app| app.state().action_list().is_none());

    let sent = rig.everything();
    let (_, params) = sent
        .iter()
        .find(|(method, _)| method == "workspace/executeCommand")
        .expect("the command was sent");
    assert_eq!(params["command"], json!("fix.all"));
    assert_eq!(params["arguments"], json!([1]));
    assert_eq!(rig.main_text(), SOURCE);
}

#[test]
fn an_error_of_the_server_is_shown() {
    let mut rig = rig(only(|_| json!({ "__error": "no actions today" })));

    rig.ctrl_dot();
    rig.wait_for_notice("no actions today");

    assert!(rig.app.state().action_list().is_none());
}

#[test]
fn an_operation_on_a_file_is_refused_and_nothing_changes() {
    let mut rig = rig(only(|uri| {
        json!([{ "title": "Move it", "edit": { "documentChanges": [
            { "textDocument": { "uri": uri, "version": 1 }, "edits": [insert_at_top("x")] },
            { "kind": "rename", "oldUri": "file:///p/a.rs", "newUri": "file:///p/b.rs" },
        ] } }])
    }));

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Enter);
    rig.wait_for_notice("cannot do");

    assert_eq!(rig.main_text(), SOURCE);
}

#[test]
fn a_file_outside_the_project_is_refused() {
    let outside = tempfile::tempdir().unwrap();
    let path = outside.path().join("dep.rs");
    fs::write(&path, "x\n").unwrap();
    let dep = crate::lsp::uri::file_uri(&path).unwrap();
    let mut rig = rig(only(move |uri| {
        json!([{ "title": "Both", "edit": { "changes": {
            uri: [insert_at_top("y")],
            dep.clone(): [insert_at_top("y")],
        } } }])
    }));

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Enter);
    rig.wait_for_notice("outside the project");

    assert_eq!(rig.main_text(), SOURCE);
    assert_eq!(fs::read_to_string(&path).unwrap(), "x\n");
}

#[test]
fn typing_before_the_pick_aborts_the_edit() {
    let mut rig = rig(only(|uri| json!([import_action(uri)])));
    rig.ctrl_dot();
    rig.wait_for_list();
    rig.app
        .state_mut()
        .edit(|editor| editor.insert_text("x").unwrap());

    rig.press(KeyCode::Enter);

    assert!(
        rig.said().contains("changed while the language server"),
        "{}",
        rig.said()
    );
    assert!(!rig.main_text().contains("use std"));
}

// ---- a file the edit names that is not open

fn names(rig: &Rig) -> Vec<String> {
    let mut names = Vec::new();
    let workspace = rig.app.state().workspace();
    for path in workspace.open_paths() {
        names.push(path.file_name().unwrap().to_string_lossy().into_owned());
    }
    names.sort();
    names
}

fn lib_action(uri: &str) -> Value {
    let lib = sibling(uri, "lib.rs");
    json!({ "title": "Fix in lib", "kind": "quickfix", "edit": edit_of(&lib, "// fixed\n") })
}

#[test]
fn a_closed_file_is_opened_as_a_tab_and_the_server_is_asked_again() {
    let mut rig = rig(only(|uri| json!([lib_action(uri)])));
    fs::write(rig.lib_path(), "pub fn lib() {}\n").unwrap();

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Enter);
    rig.wait_for_notice("opened src/lib.rs");
    // The second answer, then the edit.
    rig.pump_until(|app| {
        app.state()
            .notifications()
            .texts()
            .iter()
            .any(|told| told.contains("Fix in lib:"))
    });

    assert_eq!(names(&rig), ["lib.rs", "main.rs"]);
    assert_eq!(
        rig.app.state().editor().display_name(),
        "main.rs",
        "main stays in front"
    );
    assert_eq!(
        fs::read_to_string(rig.lib_path()).unwrap(),
        "pub fn lib() {}\n",
        "never written"
    );
    let lib = rig
        .app
        .state()
        .workspace()
        .document_of_path(&rig.lib_path())
        .unwrap();
    let (_, _, buffer) = rig.app.state().workspace().document_info(lib).unwrap();
    assert_eq!(buffer.text(), "// fixed\npub fn lib() {}\n");
    assert!(buffer.is_dirty());

    let sent = rig.everything();
    assert_eq!(methods(&sent, "textDocument/codeAction"), 2);
    let did_open = sent
        .iter()
        .position(|(method, params)| {
            method == "textDocument/didOpen"
                && params["textDocument"]["uri"]
                    .as_str()
                    .unwrap()
                    .ends_with("lib.rs")
        })
        .expect("the server was told about lib.rs");
    let second = sent
        .iter()
        .rposition(|(method, _)| method == "textDocument/codeAction")
        .unwrap();
    assert!(
        did_open < second,
        "didOpen comes before the second question"
    );
}

#[test]
fn an_edit_in_open_files_only_makes_no_second_request() {
    let mut rig = rig(only(|uri| json!([import_action(uri)])));

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Enter);

    let sent = rig.everything();
    assert_eq!(methods(&sent, "textDocument/codeAction"), 1);
}

#[test]
fn an_action_that_is_gone_from_the_second_answer_shows_the_list_again() {
    let asked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&asked);
    let mut rig = rig(only(move |uri| {
        match counter.fetch_add(1, Ordering::SeqCst) {
            0 => json!([lib_action(uri)]),
            _ => json!([{ "title": "Something else", "edit": edit_of(uri, "// other\n") }]),
        }
    }));
    fs::write(rig.lib_path(), "pub fn lib() {}\n").unwrap();

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Enter);
    rig.wait_for_notice("the actions changed");
    rig.wait_for_list();

    assert_eq!(rig.titles(), ["Something else"]);
    assert_eq!(rig.main_text(), SOURCE, "nothing was applied");
    let lib = rig
        .app
        .state()
        .workspace()
        .document_of_path(&rig.lib_path())
        .unwrap();
    let (_, _, buffer) = rig.app.state().workspace().document_info(lib).unwrap();
    assert_eq!(buffer.text(), "pub fn lib() {}\n");
}

#[test]
fn two_actions_with_one_title_are_told_apart_by_their_files() {
    let mut rig = rig(only(|uri| {
        let lib = sibling(uri, "lib.rs");
        let util = sibling(uri, "util.rs");
        json!([
            { "title": "Import Foo", "kind": "quickfix", "edit": edit_of(&lib, "// from lib\n") },
            { "title": "Import Foo", "kind": "quickfix", "edit": edit_of(&util, "// from util\n") },
        ])
    }));
    fs::write(rig.lib_path(), "pub fn lib() {}\n").unwrap();
    fs::write(
        rig.lib_path().with_file_name("util.rs"),
        "pub fn util() {}\n",
    )
    .unwrap();

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Char('2'));
    rig.pump_until(|app| {
        app.state()
            .notifications()
            .texts()
            .iter()
            .any(|told| told.contains("Import Foo:"))
    });

    let util = rig
        .app
        .state()
        .workspace()
        .document_of_path(&rig.lib_path().with_file_name("util.rs"))
        .unwrap();
    let (_, _, buffer) = rig.app.state().workspace().document_info(util).unwrap();
    assert_eq!(buffer.text(), "// from util\npub fn util() {}\n");
    assert!(
        rig.app
            .state()
            .workspace()
            .document_of_path(&rig.lib_path())
            .is_none()
    );
}

#[test]
fn a_second_edit_naming_another_closed_file_opens_it_and_shows_the_list_without_a_third_request() {
    let asked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&asked);
    let mut rig = rig(only(move |uri| {
        let action = |files: &[&str]| {
            let changes: serde_json::Map<String, Value> = files
                .iter()
                .map(|name| (sibling(uri, name), json!([insert_at_top("// x\n")])))
                .collect();
            json!([{ "title": "Spread", "kind": "quickfix", "edit": { "changes": changes } }])
        };
        match counter.fetch_add(1, Ordering::SeqCst) {
            0 => action(&["lib.rs"]),
            _ => action(&["lib.rs", "util.rs"]),
        }
    }));
    fs::write(rig.lib_path(), "pub fn lib() {}\n").unwrap();
    fs::write(
        rig.lib_path().with_file_name("util.rs"),
        "pub fn util() {}\n",
    )
    .unwrap();

    rig.ctrl_dot();
    rig.wait_for_list();
    rig.press(KeyCode::Enter);
    rig.wait_for_notice("names more files");
    rig.wait_for_list();

    assert_eq!(names(&rig), ["lib.rs", "main.rs", "util.rs"]);
    let sent = rig.everything();
    assert_eq!(
        methods(&sent, "textDocument/codeAction"),
        2,
        "never a third"
    );
    assert_eq!(rig.titles(), ["Spread"]);
}

#[test]
fn the_palette_lists_code_action() {
    assert!(
        crate::event::action::Action::palette_actions()
            .iter()
            .any(|action| action.title() == Some("Language: Code Action"))
    );
}

/// Against the real `rust-analyzer`, which has to be on the PATH:
/// `cargo test real_rust_analyzer_imports -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_imports_a_missing_name() {
    let mut rig = Rig::real("fn main() {\n    let m: HashMap<u8, u8> = HashMap::new();\n}\n");
    rig.app
        .state_mut()
        .edit(|editor| editor.go_to(Position::new(1, 12)));

    // The server is still loading the project at first: ask until it offers the import.
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    loop {
        rig.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char('.'),
            KeyModifiers::CONTROL,
        )));
        let asked = std::time::Instant::now();
        while asked.elapsed() < Duration::from_secs(5) && rig.app.state().action_list().is_none() {
            while let Ok(event) = rig.events.try_recv() {
                rig.app.handle_event(event);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let offered = rig.titles();
        if offered.iter().any(|title| title.contains("HashMap")) {
            eprintln!("offered: {offered:?}");
            break;
        }
        rig.press(KeyCode::Esc);
        assert!(
            std::time::Instant::now() < deadline,
            "{offered:?} {}",
            rig.said()
        );
    }
    let at = rig
        .titles()
        .iter()
        .position(|title| title.contains("HashMap"))
        .unwrap();
    rig.press(KeyCode::Char(char::from_digit(at as u32 + 1, 10).unwrap()));
    rig.pump_until(|app| {
        app.state()
            .editor()
            .buffer()
            .text()
            .contains("use std::collections::HashMap;")
    });

    eprintln!("{}", rig.main_text());
}
