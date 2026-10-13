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

// ---- several places: the list

/// Points at several files next to the one that was asked about.
fn siblings(places: &'static [(&'static str, u32)]) -> Answer {
    Arc::new(move |params| {
        let uri = params["textDocument"]["uri"].as_str().unwrap();
        let (folder, _) = uri.rsplit_once('/').unwrap();
        Value::Array(
            places
                .iter()
                .map(|(name, line)| json!({ "uri": format!("{folder}/{name}"), "range": range(*line, 0, 4) }))
                .collect(),
        )
    })
}

const TWO: &[(&str, u32)] = &[("lib.rs", 4), ("util.rs", 9)];

fn rig_with_two() -> Rig {
    let rig = rig(siblings(TWO));
    let util: String = (1..=20).map(|n| format!("    util {n}\n")).collect();
    fs::write(rig._dir.path().join("src/util.rs"), util).unwrap();
    rig
}

impl Rig {
    fn list_open(&self) -> bool {
        self.app.state().definition_list().is_some()
    }

    fn list_selected(&self) -> usize {
        self.app.state().definition_list().unwrap().selected()
    }
}

#[test]
fn several_places_open_a_list_and_the_cursor_stays_until_one_is_chosen() {
    let mut rig = rig_with_two();

    rig.f12_on_value();
    rig.pump_until(|app| app.state().definition_list().is_some());

    let list = rig.app.state().definition_list().unwrap();
    assert_eq!(list.entries().len(), 2);
    assert_eq!(rig.name(), "main.rs");
    assert_eq!(rig.head(), Position::new(1, 10));
}

#[test]
fn each_row_has_the_file_the_line_and_the_text_of_the_line() {
    let mut rig = rig_with_two();
    rig.f12_on_value();
    rig.pump_until(|app| app.state().definition_list().is_some());

    let screen = rig.screen();

    assert!(screen.contains("Definitions (2)"), "{screen}");
    assert!(screen.contains("src/lib.rs:5"), "{screen}");
    assert!(screen.contains("line 5"), "{screen}");
    assert!(screen.contains("src/util.rs:10"), "{screen}");
    // The text of the line has no indent.
    assert!(screen.contains("util 10"), "{screen}");
    let entries = rig.app.state().definition_list().unwrap().entries();
    assert_eq!(entries[1].text, "util 10");
}

#[test]
fn enter_goes_to_the_selected_place_and_back_returns() {
    let mut rig = rig_with_two();
    rig.f12_on_value();
    rig.pump_until(|app| app.state().definition_list().is_some());

    rig.press(KeyCode::Down);
    rig.press(KeyCode::Enter);

    assert!(!rig.list_open());
    assert_eq!(rig.name(), "util.rs");
    assert_eq!(rig.head(), Position::new(9, 0));
    rig.alt(',');
    assert_eq!(rig.name(), "main.rs");
    assert_eq!(rig.head(), Position::new(1, 10));
}

#[test]
fn up_and_down_go_round_and_escape_closes_without_going() {
    let mut rig = rig_with_two();
    rig.f12_on_value();
    rig.pump_until(|app| app.state().definition_list().is_some());

    rig.press(KeyCode::Up);
    assert_eq!(rig.list_selected(), 1);
    rig.press(KeyCode::Down);
    assert_eq!(rig.list_selected(), 0);
    rig.press(KeyCode::Esc);

    assert!(!rig.list_open());
    assert_eq!(rig.name(), "main.rs");
    assert_eq!(rig.head(), Position::new(1, 10));
}

#[test]
fn a_click_on_a_row_goes_to_it_and_one_outside_closes_the_list() {
    let mut rig = rig_with_two();
    rig.f12_on_value();
    rig.pump_until(|app| app.state().definition_list().is_some());
    let (column, row) = rig.cell_at("src/util.rs:10");

    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: column + 2,
        row,
        modifiers: KeyModifiers::NONE,
    }));

    assert!(!rig.list_open());
    assert_eq!(rig.name(), "util.rs");

    // Again, and a click far from the box.
    rig.alt(',');
    rig.f12_on_value();
    rig.pump_until(|app| app.state().definition_list().is_some());
    rig.screen();
    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 98,
        row: 28,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(!rig.list_open());
    assert_eq!(rig.name(), "main.rs");
}

#[test]
fn keys_do_not_reach_the_editor_while_the_list_is_open() {
    let mut rig = rig_with_two();
    rig.f12_on_value();
    rig.pump_until(|app| app.state().definition_list().is_some());
    let before = rig.app.state().editor().buffer().text();

    rig.press(KeyCode::Char('x'));

    assert_eq!(rig.app.state().editor().buffer().text(), before);
    assert!(rig.list_open());
}

#[test]
fn many_places_scroll_and_a_page_stops_at_the_ends() {
    let many: Vec<(&'static str, u32)> = (0..30).map(|n| ("lib.rs", n)).collect();
    let many: &'static [(&'static str, u32)] = Box::leak(many.into_boxed_slice());
    let mut rig = rig(siblings(many));
    rig.f12_on_value();
    rig.pump_until(|app| app.state().definition_list().is_some());

    rig.press(KeyCode::PageDown);
    assert_eq!(rig.list_selected(), 12);
    for _ in 0..5 {
        rig.press(KeyCode::PageDown);
    }
    assert_eq!(rig.list_selected(), 29);
    let screen = rig.screen();
    assert!(screen.contains("src/lib.rs:30"), "{screen}");
    assert!(!screen.contains("src/lib.rs:1 "), "{screen}");
}

// ---- the proof: the real rust-analyzer

#[cfg(unix)]
impl Rig {
    /// Presses `f12` until `done`, since the server may still be loading and say
    /// nothing at first; for up to two minutes.
    fn f12_until(&mut self, done: impl Fn(&Rig) -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        while std::time::Instant::now() < deadline {
            self.screen();
            self.press(KeyCode::F(12));
            let tried = std::time::Instant::now() + Duration::from_secs(6);
            while std::time::Instant::now() < tried {
                while let Ok(event) = self.events.try_recv() {
                    self.app.handle_event(event);
                }
                self.app.lsp_sync();
                if done(self) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            // The list of several places or a message is in the way of a new try.
            self.press(KeyCode::Esc);
        }
        panic!(
            "go to definition never arrived; status {:?}, told {}",
            self.app.state().lsp_status(),
            self.told()
        );
    }

    fn go_to_text(&mut self, needle: &str, extra: usize) {
        let text = self.app.state().editor().buffer().text();
        let byte = text
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} is not in the text"));
        let position = self
            .app
            .state()
            .editor()
            .buffer()
            .byte_to_position(byte + extra)
            .unwrap();
        self.app.state_mut().edit(|editor| editor.go_to(position));
    }

    fn path(&self) -> String {
        self.app
            .state()
            .editor()
            .buffer()
            .path()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    }
}

/// The real `rust-analyzer`: a definition in the project, one in the standard
/// library (opened read-only), a jump from inside it, and back through all of
/// them. Run by hand:
/// `cargo test real_rust_analyzer_goes_to -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_goes_to_definitions_in_the_project_and_in_the_standard_library_and_back() {
    let source = "fn main() {\n    let v: Vec<u32> = Vec::new();\n    helper(v);\n}\n\nfn helper(v: Vec<u32>) {}\n";
    let mut rig = Rig::real(source);

    // In the project: `helper` on line 2.
    rig.go_to_text("helper(v);", 2);
    rig.f12_until(|rig| rig.head().line == 5);
    assert_eq!(rig.name(), "main.rs");
    assert!(!rig.app.state().editor().buffer().is_read_only());

    rig.alt(',');
    assert_eq!(rig.head().line, 2);

    // In the standard library: `new` of `Vec::new()`.
    rig.go_to_text("new()", 1);
    rig.f12_until(|rig| rig.name() != "main.rs");
    let first = rig.path();
    eprintln!("the standard library: {first} at {:?}", rig.head());
    assert!(first.contains("vec"), "{first}");
    assert!(rig.app.state().editor().buffer().is_read_only());
    let line = rig
        .app
        .state()
        .editor()
        .buffer()
        .line_content(rig.head().line)
        .unwrap()
        .to_string();
    assert!(line.contains("fn new"), "{line}");

    // From inside it: the same server goes on to the next definition.
    rig.go_to_text("RawVec", 2);
    rig.f12_until(|rig| rig.path() != first);
    let second = rig.path();
    eprintln!("and on: {second} at {:?}", rig.head());
    assert!(rig.app.state().editor().buffer().is_read_only());

    // Back through all of them.
    rig.alt(',');
    assert_eq!(rig.path(), first);
    rig.alt(',');
    assert_eq!(rig.name(), "main.rs");
    assert_eq!(rig.head().line, 1);
}
