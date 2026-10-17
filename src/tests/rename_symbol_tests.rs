//! Rename symbol, from `f2` to the files that change, against a fake server that
//! answers with the edits a test gives it.

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::completion_tests::{Answer, Rig};
use crate::buffer::Position;
use crate::lsp::Message;
use crate::lsp::uri::file_uri;

const SOURCE: &str = "fn main() {\n    let value = 1;\n    println!(\"{}\", value);\n}\n";

fn range(line: u32, character: u32, length: u32) -> Value {
    json!({
        "start": { "line": line, "character": character },
        "end": { "line": line, "character": character + length },
    })
}

fn edit(line: u32, character: u32, new: &str) -> Value {
    json!({ "range": range(line, character, 5), "newText": new })
}

/// The two uses of `value` in `SOURCE`, renamed.
fn main_edits(new: &str) -> Value {
    json!([edit(1, 8, new), edit(2, 19, new)])
}

/// Says `value` is at the cursor, and answers a rename with what `edit` makes
/// of the file asked about and the new name.
fn renaming(edit: impl Fn(&str, &str) -> Value + Send + Sync + 'static) -> Answer {
    Arc::new(move |params| match params["newName"].as_str() {
        None => json!({ "range": range(1, 8, 5), "placeholder": "value" }),
        Some(new) => edit(params["textDocument"]["uri"].as_str().unwrap(), new),
    })
}

fn only_main() -> Answer {
    renaming(|uri, new| json!({ "changes": { uri: main_edits(new) } }))
}

fn provider(prepare: bool) -> Value {
    json!({
        "textDocumentSync": 2,
        "renameProvider": if prepare { json!({ "prepareProvider": true }) } else { json!(true) },
    })
}

impl Rig {
    pub(super) fn said(&self) -> String {
        self.app.state().notifications().texts().join("|")
    }

    pub(super) fn wait_for_notice(&mut self, text: &str) {
        self.pump_until(|app| {
            app.state()
                .notifications()
                .texts()
                .iter()
                .any(|told| told.contains(text))
        });
    }

    /// The cursor on `value` (line 1, column 10), then `f2`.
    fn f2_on_value(&mut self) {
        self.app
            .state_mut()
            .edit(|editor| editor.go_to(Position::new(1, 10)));
        self.screen();
        self.press(KeyCode::F(2));
    }

    fn wait_for_prompt(&mut self) {
        self.pump_until(|app| app.state().name_prompt().is_some());
    }

    /// Clears the prompt, types `name` and presses enter.
    fn answer_prompt(&mut self, name: &str) {
        self.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
        )));
        for c in name.chars() {
            self.press(KeyCode::Char(c));
        }
        self.press(KeyCode::Enter);
    }

    fn sent_methods(&self) -> Vec<(String, Value)> {
        let mut found = Vec::new();
        while let Ok(message) = self.received.recv_timeout(Duration::from_millis(150)) {
            if let Message::Request { method, params, .. } = message
                && method.contains("ename")
            {
                found.push((method, params));
            }
        }
        found
    }

    pub(super) fn lib_path(&self) -> std::path::PathBuf {
        self._dir.path().join("src/lib.rs")
    }
}

fn rig(prepare: bool, answer: Answer) -> Rig {
    Rig::start(provider(prepare), SOURCE, answer)
}

#[test]
fn f2_asks_what_is_there_then_for_a_name_then_renames_every_place() {
    let mut rig = rig(true, only_main());

    rig.f2_on_value();
    rig.wait_for_prompt();
    assert_eq!(rig.app.state().name_prompt().unwrap().name(), "value");
    rig.answer_prompt("total");
    rig.wait_for_notice("renamed value to total");

    assert_eq!(
        rig.text(),
        "fn main() {\n    let total = 1;\n    println!(\"{}\", total);\n}\n"
    );
    let sent = rig.sent_methods();
    assert_eq!(sent[0].0, "textDocument/prepareRename");
    assert_eq!(sent[1].0, "textDocument/rename");
    assert_eq!(sent[1].1["newName"], json!("total"));
    assert!(
        rig.said()
            .contains("renamed value to total: 2 places in 1 file; 1 open file is not saved"),
        "{}",
        rig.said()
    );
}

#[test]
fn the_open_file_is_one_undo_step_and_stays_unsaved() {
    let mut rig = rig(true, only_main());
    rig.f2_on_value();
    rig.wait_for_prompt();
    rig.answer_prompt("total");
    rig.wait_for_notice("renamed");

    assert!(rig.app.state().editor().buffer().is_dirty());
    rig.app.state_mut().edit(|editor| editor.undo().unwrap());

    assert_eq!(rig.text(), SOURCE, "one undo brings every place back");
    let on_disk = fs::read_to_string(rig._dir.path().join("src/main.rs")).unwrap();
    assert_eq!(on_disk, SOURCE, "the file was never written");
}

#[test]
fn the_cursor_stays_on_its_text() {
    let mut rig = rig(true, only_main());
    rig.f2_on_value();
    rig.wait_for_prompt();
    rig.answer_prompt("total_amount");
    rig.wait_for_notice("renamed");

    // Column 10 was inside `value`, which became longer: the cursor is after it.
    let head = rig.app.state().editor().selections().primary().head();
    assert_eq!(head.line, 1);
    assert_eq!(head.column, "    let total_amount".len());
}

#[test]
fn a_file_that_is_not_open_is_written_and_an_open_one_is_not() {
    let answer = renaming(|uri, new| {
        let lib = uri.replace("main.rs", "lib.rs");
        json!({ "documentChanges": [
            { "textDocument": { "uri": uri, "version": 1 }, "edits": main_edits(new) },
            { "textDocument": { "uri": lib, "version": null }, "edits": [edit(0, 4, new)] },
        ]})
    });
    let mut rig = rig(true, answer);
    fs::write(rig.lib_path(), "use value;\n").unwrap();

    rig.f2_on_value();
    rig.wait_for_prompt();
    rig.answer_prompt("total");
    rig.wait_for_notice("renamed");

    assert_eq!(fs::read_to_string(rig.lib_path()).unwrap(), "use total;\n");
    assert!(rig.text().contains("let total"));
    let main = fs::read_to_string(rig._dir.path().join("src/main.rs")).unwrap();
    assert_eq!(main, SOURCE, "the open file waits in its tab");
    assert!(
        rig.said()
            .contains("3 places in 2 files; 1 open file is not saved"),
        "{}",
        rig.said()
    );
}

#[test]
fn a_dirty_open_file_takes_the_edit() {
    let mut rig = rig(true, only_main());
    rig.app
        .state_mut()
        .edit(|editor| editor.insert_text("// end\n").unwrap());
    rig.frame();

    rig.f2_on_value();
    rig.wait_for_prompt();
    rig.answer_prompt("total");
    rig.wait_for_notice("renamed");

    assert!(rig.text().contains("let total") && rig.text().ends_with("// end\n"));
}

#[test]
fn a_file_that_does_not_hold_the_name_refuses_the_whole_rename() {
    let answer = renaming(|uri, new| {
        let lib = uri.replace("main.rs", "lib.rs");
        json!({ "changes": { uri: main_edits(new), lib: [edit(0, 0, new)] } })
    });
    let mut rig = rig(true, answer);
    fs::write(rig.lib_path(), "changed on disk\n").unwrap();

    rig.f2_on_value();
    rig.wait_for_prompt();
    rig.answer_prompt("total");
    rig.wait_for_notice("positions do not fit");

    assert_eq!(rig.text(), SOURCE, "the open file is untouched too");
    assert_eq!(
        fs::read_to_string(rig.lib_path()).unwrap(),
        "changed on disk\n"
    );
    assert!(rig.said().contains("nothing was changed"), "{}", rig.said());
}

#[test]
fn typing_while_the_server_works_aborts_and_says_why() {
    let mut rig = rig(true, only_main());
    rig.f2_on_value();
    rig.wait_for_prompt();
    rig.answer_prompt("total");
    // The answer is on its way; this keystroke comes first.
    rig.app
        .state_mut()
        .edit(|editor| editor.insert_text("x").unwrap());
    rig.wait_for_notice("changed while the language server");

    assert!(!rig.text().contains("total"));
    assert!(rig.said().contains("src/main.rs"), "{}", rig.said());
}

#[test]
fn an_edit_that_asks_to_move_a_file_is_refused_and_nothing_changes() {
    let answer = renaming(|uri, new| {
        json!({ "documentChanges": [
            { "textDocument": { "uri": uri, "version": 1 }, "edits": main_edits(new) },
            { "kind": "rename", "oldUri": "file:///p/value.rs", "newUri": "file:///p/total.rs" },
        ]})
    });
    let mut rig = rig(true, answer);

    rig.f2_on_value();
    rig.wait_for_prompt();
    rig.answer_prompt("total");
    rig.wait_for_notice("cannot do");

    assert_eq!(rig.text(), SOURCE);
    assert!(
        rig.said().contains("rename file:///p/value.rs"),
        "{}",
        rig.said()
    );
}

#[test]
fn a_file_outside_the_project_is_refused() {
    let outside = tempfile::tempdir().unwrap();
    let path = outside.path().join("dep.rs");
    fs::write(&path, "value\n").unwrap();
    let dep = file_uri(&path).unwrap();
    let answer = renaming(
        move |uri, new| json!({ "changes": { uri: main_edits(new), dep.clone(): [edit(0, 0, new)] } }),
    );
    let mut rig = rig(true, answer);

    rig.f2_on_value();
    rig.wait_for_prompt();
    rig.answer_prompt("total");
    rig.wait_for_notice("outside the project");

    assert_eq!(rig.text(), SOURCE);
    assert_eq!(fs::read_to_string(&path).unwrap(), "value\n");
}

#[test]
fn nothing_to_rename_is_said_and_no_prompt_opens() {
    let mut rig = rig(true, Arc::new(|_| Value::Null));

    rig.f2_on_value();
    rig.wait_for_notice("nothing to rename");

    assert!(rig.app.state().name_prompt().is_none());
}

#[test]
fn an_error_of_the_server_is_shown() {
    let answer = renaming(|_, _| json!({ "__error": "cannot rename this" }));
    let mut rig = rig(true, answer);

    rig.f2_on_value();
    rig.wait_for_prompt();
    rig.answer_prompt("total");
    rig.wait_for_notice("cannot rename this");

    assert_eq!(rig.text(), SOURCE);
}

#[test]
fn a_server_without_prepare_starts_from_the_word() {
    let mut rig = rig(false, only_main());

    rig.f2_on_value();
    rig.wait_for_prompt();

    assert_eq!(rig.app.state().name_prompt().unwrap().name(), "value");
    rig.answer_prompt("total");
    rig.wait_for_notice("renamed value to total");
    let sent = rig.sent_methods();
    assert!(
        sent.iter()
            .all(|(method, _)| method != "textDocument/prepareRename")
    );
}

#[test]
fn a_server_that_does_not_rename_says_so() {
    let mut rig = Rig::start(
        json!({ "textDocumentSync": 2 }),
        SOURCE,
        Arc::new(|_| Value::Null),
    );

    rig.f2_on_value();

    assert!(
        rig.said().contains("no language server renames symbols"),
        "{}",
        rig.said()
    );
    assert!(rig.app.state().name_prompt().is_none());
}

#[test]
fn the_same_name_asks_the_server_nothing() {
    let mut rig = rig(true, only_main());

    rig.f2_on_value();
    rig.wait_for_prompt();
    rig.press(KeyCode::Enter);

    assert!(rig.said().contains("unchanged"), "{}", rig.said());
    let sent = rig.sent_methods();
    assert!(
        sent.iter()
            .all(|(method, _)| method != "textDocument/rename")
    );
}

#[test]
fn escape_cancels_and_an_empty_name_is_refused() {
    let mut rig = rig(true, only_main());
    rig.f2_on_value();
    rig.wait_for_prompt();

    rig.answer_prompt("");
    assert!(
        rig.app.state().name_prompt().is_some(),
        "an empty name stays in the prompt"
    );

    rig.press(KeyCode::Esc);
    assert!(rig.app.state().name_prompt().is_none());
    assert_eq!(rig.text(), SOURCE);
    let sent = rig.sent_methods();
    assert!(
        sent.iter()
            .all(|(method, _)| method != "textDocument/rename")
    );
}

#[test]
fn the_palette_lists_rename_symbol() {
    assert!(
        crate::event::action::Action::palette_actions()
            .iter()
            .any(|action| action.title() == Some("Language: Rename Symbol"))
    );
}

/// Against the real `rust-analyzer`, which has to be on the PATH:
/// `cargo test real_rust_analyzer_renames -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_renames_a_function_in_a_file_that_is_not_open() {
    let mut rig = Rig::real("mod util;\n\nfn main() {\n    util::helper();\n}\n");
    fs::write(
        rig.lib_path().with_file_name("util.rs"),
        "pub fn helper() {}\n",
    )
    .unwrap();
    rig.app
        .state_mut()
        .edit(|editor| editor.go_to(Position::new(3, 12)));

    // The server is still loading the project at first: ask until it answers.
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    loop {
        rig.press(KeyCode::F(2));
        let asked = std::time::Instant::now();
        while asked.elapsed() < Duration::from_secs(5) && rig.app.state().name_prompt().is_none() {
            while let Ok(event) = rig.events.try_recv() {
                rig.app.handle_event(event);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if rig.app.state().name_prompt().is_some() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{}", rig.said());
    }
    assert_eq!(rig.app.state().name_prompt().unwrap().name(), "helper");

    rig.answer_prompt("assist");
    rig.wait_for_notice("renamed helper to assist");

    assert!(rig.text().contains("util::assist();"), "{}", rig.text());
    let util = fs::read_to_string(rig.lib_path().with_file_name("util.rs")).unwrap();
    assert_eq!(util, "pub fn assist() {}\n");
    eprintln!("{}", rig.said());
}
