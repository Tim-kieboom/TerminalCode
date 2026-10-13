//! Go to definition, from `f12` and `ctrl+click` to the file that opens,
//! against a fake server that points at the places a test gives it.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use serde_json::{Value, json};

use super::completion_tests::{Answer, Rig};
use crate::buffer::Position;
use crate::lsp::Message;
use crate::lsp::uri::file_uri;

fn provider() -> Value {
    json!({ "textDocumentSync": 2, "definitionProvider": true })
}

fn range(line: u32, character: u32, length: u32) -> Value {
    json!({
        "start": { "line": line, "character": character },
        "end": { "line": line, "character": character + length },
    })
}

/// Points at `line` and `character` of the file that was asked about.
fn same_file(line: u32, character: u32) -> Answer {
    Arc::new(
        move |params| json!({ "uri": params["textDocument"]["uri"], "range": range(line, character, 4) }),
    )
}

/// Points at a file next to the one that was asked about.
fn sibling(name: &'static str, line: u32) -> Answer {
    Arc::new(move |params| {
        let uri = params["textDocument"]["uri"].as_str().unwrap();
        let (folder, _) = uri.rsplit_once('/').unwrap();
        json!({ "uri": format!("{folder}/{name}"), "range": range(line, 0, 4) })
    })
}

/// Points at a file that is somewhere else, at `line`.
fn elsewhere(path: &Path, line: u32) -> Answer {
    let uri = file_uri(path).unwrap();
    Arc::new(move |_| json!({ "uri": uri, "range": range(line, 0, 4) }))
}

fn answer_with(value: Value) -> Answer {
    Arc::new(move |_| value.clone())
}

const SOURCE: &str = "fn main() {\n    let value = other(1);\n}\n";

fn forty_lines() -> String {
    (1..=40).map(|n| format!("line {n}\n")).collect()
}

/// An app over `SOURCE` whose server answers definition requests with `answer`,
/// and a file `lib.rs` of forty lines next to it.
fn rig(answer: Answer) -> Rig {
    let mut rig = Rig::start(provider(), SOURCE, answer);
    fs::write(rig._dir.path().join("src/lib.rs"), forty_lines()).unwrap();
    rig.screen();
    rig
}

impl Rig {
    fn definition_requests(&self) -> Vec<Value> {
        let mut found = Vec::new();
        while let Ok(message) = self.received.recv_timeout(Duration::from_millis(150)) {
            if let Message::Request { method, params, .. } = message
                && method == "textDocument/definition"
            {
                found.push(params);
            }
        }
        found
    }

    fn head(&self) -> Position {
        self.app.state().editor().selections().primary().head()
    }

    fn name(&self) -> String {
        self.app.state().editor().display_name()
    }

    fn told(&self) -> String {
        self.app.state().notifications().texts().join("|")
    }

    /// The cursor on `value` (line 1, column 10), then `f12`.
    fn f12_on_value(&mut self) {
        self.app
            .state_mut()
            .edit(|editor| editor.go_to(Position::new(1, 10)));
        self.screen();
        self.press(KeyCode::F(12));
    }

    fn key_ctrl(&mut self, c: char) {
        self.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::CONTROL,
        )));
    }

    fn alt(&mut self, c: char) {
        self.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::ALT,
        )));
    }

    /// The screen cell where `needle` starts.
    fn cell_at(&mut self, needle: &str) -> (u16, u16) {
        let screen = self.screen();
        for (row, line) in screen.lines().enumerate() {
            let line = line.trim_matches('"');
            if let Some(byte) = line.find(needle) {
                return (line[..byte].chars().count() as u16, row as u16);
            }
        }
        panic!("the screen does not show {needle:?}: {screen}");
    }
}

#[test]
fn f12_asks_about_the_word_at_the_cursor_and_goes_to_a_place_in_the_same_file() {
    let mut rig = rig(same_file(0, 3));

    rig.f12_on_value();
    rig.pump_until(|app| app.state().editor().selections().primary().head().line == 0);

    assert_eq!(rig.head(), Position::new(0, 3));
    let requests = rig.definition_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]["position"],
        json!({ "line": 1, "character": 10 })
    );
}

#[test]
fn a_definition_in_another_file_of_the_project_opens_it_and_back_returns() {
    let mut rig = rig(sibling("lib.rs", 29));

    rig.f12_on_value();
    rig.pump_until(|app| app.state().editor().display_name() == "lib.rs");

    assert_eq!(rig.head(), Position::new(29, 0));
    assert!(!rig.app.state().editor().buffer().is_read_only());
    rig.alt(',');
    assert_eq!(rig.name(), "main.rs");
    assert_eq!(rig.head(), Position::new(1, 10));
    rig.alt('.');
    assert_eq!(rig.name(), "lib.rs");
}

#[test]
fn a_file_that_is_open_already_is_switched_to_not_opened_again() {
    let mut rig = rig(sibling("lib.rs", 5));
    rig.f12_on_value();
    rig.pump_until(|app| app.state().editor().display_name() == "lib.rs");
    rig.alt(',');

    rig.press(KeyCode::F(12));
    rig.pump_until(|app| app.state().editor().display_name() == "lib.rs");

    let tabs = rig.app.state().workspace().tab_count_for_test();
    assert_eq!(tabs, 2, "main.rs and lib.rs once each");
}

#[test]
fn a_definition_outside_the_project_opens_read_only_and_says_so() {
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("std_source.rs");
    fs::write(&file, forty_lines()).unwrap();
    let mut rig = rig(elsewhere(&file, 12));

    rig.f12_on_value();
    rig.pump_until(|app| app.state().editor().display_name() == "std_source.rs");

    assert_eq!(rig.head(), Position::new(12, 0));
    assert!(rig.app.state().editor().buffer().is_read_only());
    assert!(rig.told().contains("read-only"), "{}", rig.told());
    // Typing does nothing to the text.
    let before = rig.app.state().editor().buffer().text();
    rig.press(KeyCode::Char('x'));
    assert_eq!(rig.app.state().editor().buffer().text(), before);
}

#[test]
fn a_file_outside_the_project_stays_locked_whatever_its_permissions() {
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("std_source.rs");
    fs::write(&file, forty_lines()).unwrap();
    let mut rig = rig(elsewhere(&file, 3));
    rig.f12_on_value();
    rig.pump_until(|app| app.state().editor().display_name() == "std_source.rs");

    // The file system says it can be written, and the app has checked again.
    rig.app.handle_input(InputEvent::FocusGained);
    rig.press(KeyCode::Char('x'));

    assert!(rig.app.state().editor().buffer().is_read_only());
    assert!(!rig.app.state().editor().buffer().text().starts_with('x'));
    // And saving it is refused.
    rig.key_ctrl('s');
    assert_eq!(fs::read_to_string(&file).unwrap(), forty_lines());
}

#[test]
fn a_file_outside_the_project_is_told_to_the_server_that_pointed_at_it() {
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("std_source.rs");
    fs::write(&file, forty_lines()).unwrap();
    let mut rig = rig(elsewhere(&file, 3));
    rig.f12_on_value();
    rig.pump_until(|app| app.state().editor().display_name() == "std_source.rs");
    rig.screen();
    rig.app.lsp_sync();

    // The next definition is asked about that file, by the name the server gave it.
    rig.press(KeyCode::F(12));
    let mut opened = false;
    let mut asked = None;
    while let Ok(message) = rig.received.recv_timeout(Duration::from_millis(300)) {
        match message {
            Message::Notification { method, params } if method == "textDocument/didOpen" => {
                opened |= params["textDocument"]["uri"] == json!(file_uri(&file).unwrap());
            }
            Message::Request { method, params, .. } if method == "textDocument/definition" => {
                asked = Some(params["textDocument"]["uri"].clone());
            }
            _ => {}
        }
    }

    assert!(opened, "the server was told about the file");
    assert_eq!(asked, Some(json!(file_uri(&file).unwrap())));
}

#[test]
fn no_definition_says_so_and_stays_put() {
    let mut rig = rig(answer_with(Value::Null));

    rig.f12_on_value();
    rig.pump_until(|app| {
        app.state()
            .notifications()
            .texts()
            .join("|")
            .contains("no definition")
    });

    assert_eq!(rig.head(), Position::new(1, 10));
}

#[test]
fn a_server_that_does_not_answer_definition_is_not_asked() {
    let mut rig = Rig::start(
        json!({ "textDocumentSync": 2 }),
        SOURCE,
        answer_with(Value::Null),
    );
    rig.screen();

    rig.f12_on_value();

    assert!(rig.definition_requests().is_empty());
    assert!(rig.told().contains("no language server"), "{}", rig.told());
}

#[test]
fn f12_away_from_any_word_says_so() {
    let mut rig = rig(same_file(0, 0));
    rig.app
        .state_mut()
        .edit(|editor| editor.go_to(Position::new(0, 11)));
    rig.screen();

    rig.press(KeyCode::F(12));

    assert!(
        rig.told().contains("no word at the cursor"),
        "{}",
        rig.told()
    );
    assert!(rig.definition_requests().is_empty());
}

#[test]
fn ctrl_click_on_a_word_asks_about_it_and_the_cursor_stays_until_the_answer() {
    let mut rig = rig(sibling("lib.rs", 7));
    let (column, row) = rig.cell_at("other");
    let cursor = rig.head();

    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: column + 1,
        row,
        modifiers: KeyModifiers::CONTROL,
    }));
    assert_eq!(rig.head(), cursor, "the cursor did not move to the word");
    rig.pump_until(|app| app.state().editor().display_name() == "lib.rs");

    assert_eq!(rig.head(), Position::new(7, 0));
    let requests = rig.definition_requests();
    // `other` starts at column 16 of line 1; the click was one cell into it.
    assert_eq!(
        requests[0]["position"],
        json!({ "line": 1, "character": 17 })
    );
}

#[test]
fn a_plain_click_is_not_a_jump_to_a_definition() {
    let mut rig = rig(sibling("lib.rs", 7));
    let (column, row) = rig.cell_at("other");

    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: column + 1,
        row,
        modifiers: KeyModifiers::NONE,
    }));

    assert!(rig.definition_requests().is_empty());
    assert_eq!(rig.name(), "main.rs");
}

#[test]
fn ctrl_click_away_from_a_word_does_nothing() {
    let mut rig = rig(sibling("lib.rs", 7));
    let (column, row) = rig.cell_at("let value");

    // The space between `let` and `value`.
    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: column + 3,
        row,
        modifiers: KeyModifiers::CONTROL,
    }));

    assert!(rig.definition_requests().is_empty());
}

#[test]
fn several_places_go_to_the_first_until_there_is_a_list() {
    let answer = answer_with(json!([
        { "uri": "file:///nowhere/a.rs", "range": range(0, 0, 1) },
    ]));
    let mut rig = rig(answer);

    rig.f12_on_value();
    rig.pump_until(|app| !app.state().notifications().texts().is_empty());

    // A file that is not there cannot be opened: the user is told, the cursor stays.
    assert_eq!(rig.name(), "main.rs");
}
