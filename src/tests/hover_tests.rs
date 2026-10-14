//! What the server says about the word under the mouse pointer or the cursor,
//! from the pointer's resting to the box on the screen, against a fake server.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};

use super::completion_tests::{Answer, Rig, answering};
use crate::components::prepare_and_render;
use crate::lsp::Message;

/// Longer than the rest the pointer needs on a word.
const LONG_ENOUGH: Duration = Duration::from_millis(450);

fn hover_provider() -> Value {
    json!({ "textDocumentSync": 2, "hoverProvider": true })
}

/// A server that says `text` about everything it is asked about.
fn saying(text: &'static str) -> Answer {
    Arc::new(move |_| json!({ "contents": { "kind": "markdown", "value": text } }))
}

const SOURCE: &str = "fn main() {\n    let value = other(1);\n}\n";

/// An app over `SOURCE` with a server that says `text` when asked.
fn rig_saying(text: &'static str) -> Rig {
    let mut rig = Rig::start(hover_provider(), SOURCE, saying(text));
    rig.screen();
    rig
}

impl Rig {
    /// The screen cell where `needle` starts, from a drawn frame.
    fn cell_of(&mut self, needle: &str) -> (u16, u16) {
        let screen = self.screen();
        for (row, line) in screen.lines().enumerate() {
            let line = line.trim_matches('"');
            if let Some(byte) = line.find(needle) {
                return (line[..byte].chars().count() as u16, row as u16);
            }
        }
        panic!("the screen does not show {needle:?}: {screen}");
    }

    fn pointer_to(&mut self, column: u16, row: u16) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }

    /// The pointer rests on `needle` until the server is asked.
    fn rest_on(&mut self, needle: &str) {
        let (column, row) = self.cell_of(needle);
        self.pointer_to(column + 1, row);
        std::thread::sleep(LONG_ENOUGH);
        self.app.hover_due();
    }

    fn hover_text(&self) -> Option<String> {
        self.app
            .state()
            .hover_box()
            .map(|hover| hover.text().to_owned())
    }

    /// The hover requests the server was sent.
    fn hover_requests(&self) -> Vec<Value> {
        let mut found = Vec::new();
        while let Ok(message) = self.received.recv_timeout(Duration::from_millis(150)) {
            if let Message::Request { method, params, .. } = message
                && method == "textDocument/hover"
            {
                found.push(params);
            }
        }
        found
    }
}

#[test]
fn a_pointer_resting_on_a_word_asks_the_server_and_shows_what_it_says() {
    let mut rig = rig_saying("```rust\nfn other(n: u32) -> u32\n```\n\n---\n\nDoes **things**.");

    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    assert_eq!(
        rig.hover_text().unwrap(),
        "fn other(n: u32) -> u32\n\nDoes things."
    );
    let screen = rig.screen();
    assert!(screen.contains("fn other(n: u32) -> u32"), "{screen}");
    assert!(screen.contains("Does things."), "{screen}");
}

#[test]
fn the_request_is_about_the_word_under_the_pointer() {
    let mut rig = rig_saying("x");

    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    let requests = rig.hover_requests();
    assert_eq!(requests.len(), 1);
    // `other` starts at column 16 of line 1; the pointer was one cell into it.
    assert_eq!(requests[0]["position"]["line"], json!(1));
    assert_eq!(requests[0]["position"]["character"], json!(16 + 1));
}

#[test]
fn the_box_hangs_under_the_pointer() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    let screen = rig.screen();

    let rows: Vec<&str> = screen.lines().collect();
    let word = rows
        .iter()
        .position(|row| row.contains("let value"))
        .unwrap();
    let said = rows.iter().position(|row| row.contains("said")).unwrap();
    assert!(said > word, "{screen}");
}

#[test]
fn a_pointer_that_moves_on_before_it_rests_asks_nothing() {
    let mut rig = rig_saying("said");
    let (column, row) = rig.cell_of("other");
    rig.pointer_to(column + 1, row);

    rig.app.hover_due();
    rig.pointer_to(column + 3, row + 1);
    std::thread::sleep(LONG_ENOUGH);
    // Over an empty cell: nothing to wait for.
    rig.app.hover_due();

    assert!(rig.hover_requests().is_empty());
    assert!(rig.hover_text().is_none());
}

#[test]
fn moving_within_the_word_keeps_the_wait_and_to_another_word_starts_it_again() {
    let mut rig = rig_saying("said");
    let (column, row) = rig.cell_of("other");
    rig.pointer_to(column, row);
    std::thread::sleep(Duration::from_millis(250));
    rig.pointer_to(column + 2, row);
    std::thread::sleep(Duration::from_millis(250));

    // Half a second on the one word, in two goes.
    rig.app.hover_due();
    rig.pump_until(|app| app.state().hover_box().is_some());
    assert_eq!(rig.hover_requests().len(), 1);

    let (value_column, value_row) = rig.cell_of("value");
    rig.pointer_to(value_column, value_row);

    assert!(
        rig.hover_text().is_none(),
        "moving to another word takes the box away"
    );
}

#[test]
fn moving_off_the_text_takes_the_box_away() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    rig.pointer_to(100, 15);

    assert!(rig.hover_text().is_none());
}

#[test]
fn a_key_takes_the_box_away_and_does_its_work() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    rig.press(KeyCode::Char('x'));

    assert!(rig.hover_text().is_none());
    assert!(rig.text().ends_with('x'), "{}", rig.text());
}

#[test]
fn escape_takes_the_box_away_and_does_nothing_else() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());
    let before = rig.text();

    rig.press(KeyCode::Esc);

    assert!(rig.hover_text().is_none());
    assert_eq!(rig.text(), before);
}

#[test]
fn a_click_and_the_wheel_take_the_box_away() {
    for kind in [
        MouseEventKind::Down(crossterm::event::MouseButton::Left),
        MouseEventKind::ScrollDown,
    ] {
        let mut rig = rig_saying("said");
        rig.rest_on("other");
        rig.pump_until(|app| app.state().hover_box().is_some());

        rig.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind,
            column: 60,
            row: 12,
            modifiers: KeyModifiers::NONE,
        }));

        assert!(rig.hover_text().is_none(), "{kind:?}");
    }
}

#[test]
fn a_change_to_the_text_takes_the_box_away() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    rig.app
        .state_mut()
        .edit(|editor| editor.insert_text("y").unwrap());
    rig.frame();
    rig.app.hover_step();

    assert!(rig.hover_text().is_none());
}

#[test]
fn nothing_to_say_shows_nothing() {
    let mut rig = Rig::start(hover_provider(), SOURCE, answering(Value::Null));
    rig.screen();

    rig.rest_on("other");
    rig.pump_until(|_| true);
    std::thread::sleep(Duration::from_millis(200));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }

    assert_eq!(rig.hover_requests().len(), 1);
    assert!(rig.hover_text().is_none());
}

#[test]
fn a_server_that_does_not_answer_hover_is_not_asked() {
    let mut rig = Rig::start(json!({ "textDocumentSync": 2 }), SOURCE, saying("said"));
    rig.screen();

    rig.rest_on("other");

    assert!(rig.hover_requests().is_empty());
    assert!(rig.hover_text().is_none());
}

#[test]
fn an_answer_that_comes_after_the_pointer_moved_on_is_dropped() {
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let wait = Mutex::new(wait);
    let answer: Answer = Arc::new(move |_| {
        let _ = wait.lock().unwrap().recv();
        json!({ "contents": "late" })
    });
    let mut rig = Rig::start(hover_provider(), SOURCE, answer);
    rig.screen();
    rig.rest_on("other");
    let (column, row) = rig.cell_of("value");

    rig.pointer_to(column, row);
    release.send(()).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }

    assert!(rig.hover_text().is_none());
}

#[test]
fn f6_shows_what_the_server_says_about_the_word_at_the_cursor() {
    let mut rig = rig_saying("about the cursor word");
    // The cursor on `value`: from the start of the text, down a line and right.
    rig.app.state_mut().edit(|editor| {
        use crate::buffer::Position;
        editor.go_to(Position::new(1, 10));
    });
    rig.screen();

    rig.press(KeyCode::F(6));
    rig.pump_until(|app| app.state().hover_box().is_some());

    assert_eq!(rig.hover_text().as_deref(), Some("about the cursor word"));
    let requests = rig.hover_requests();
    assert_eq!(
        requests[0]["position"],
        json!({ "line": 1, "character": 10 })
    );
}

#[test]
fn f6_just_after_a_word_asks_about_that_word() {
    let mut rig = rig_saying("said");
    // `value` ends at column 13; the cursor is right after it.
    rig.app.state_mut().edit(|editor| {
        use crate::buffer::Position;
        editor.go_to(Position::new(1, 13));
    });
    rig.screen();

    rig.press(KeyCode::F(6));
    rig.pump_until(|app| app.state().hover_box().is_some());

    let requests = rig.hover_requests();
    assert_eq!(
        requests[0]["position"],
        json!({ "line": 1, "character": 12 })
    );
}

#[test]
fn f6_away_from_any_word_says_so() {
    let mut rig = rig_saying("said");
    rig.app.state_mut().edit(|editor| {
        use crate::buffer::Position;
        editor.go_to(Position::new(0, 11));
    });

    rig.press(KeyCode::F(6));

    assert!(rig.hover_text().is_none());
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("no word or problem at the cursor"), "{told}");
}

#[test]
fn no_box_is_drawn_while_a_popup_is_open() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    rig.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::F(1),
        KeyModifiers::NONE,
    )));
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, rig.app.state_mut()))
        .unwrap();

    assert!(!terminal.backend().to_string().contains("said"));
}

// ---- f6 keeps the box, and a long text scrolls

fn long_text() -> String {
    (1..=30)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A server that says a long text about everything.
fn saying_long() -> Answer {
    let text = long_text();
    Arc::new(move |_| json!({ "contents": text.clone() }))
}

fn rig_with_long_text() -> Rig {
    let mut rig = Rig::start(hover_provider(), SOURCE, saying_long());
    rig.screen();
    rig
}

fn wheel_at(rig: &mut Rig, kind: MouseEventKind, column: u16, row: u16) {
    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

#[test]
fn pressing_f6_while_the_box_is_up_keeps_it_when_the_pointer_moves_away() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    rig.press(KeyCode::F(6));
    rig.pointer_to(100, 15);

    assert!(rig.hover_text().is_some());
    assert!(rig.app.state().hover_box().unwrap().is_pinned());
}

#[test]
fn a_box_that_was_not_pinned_goes_when_the_pointer_moves_away() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    rig.pointer_to(100, 15);

    assert!(rig.hover_text().is_none());
}

#[test]
fn f6_without_a_box_asks_and_the_answer_stays_whatever_the_pointer_does() {
    let mut rig = rig_saying("about the cursor word");
    rig.app.state_mut().edit(|editor| {
        use crate::buffer::Position;
        editor.go_to(Position::new(1, 10));
    });
    rig.screen();
    rig.press(KeyCode::F(6));
    rig.pump_until(|app| app.state().hover_box().is_some());

    rig.pointer_to(100, 15);
    let (column, row) = rig.cell_of("other");
    rig.pointer_to(column, row);
    std::thread::sleep(LONG_ENOUGH);
    rig.app.hover_due();

    assert_eq!(rig.hover_text().as_deref(), Some("about the cursor word"));
    assert!(rig.app.state().hover_box().unwrap().is_pinned());
}

#[test]
fn a_pinned_box_stays_until_a_key_and_the_key_does_its_work() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());
    rig.press(KeyCode::F(6));

    rig.press(KeyCode::Char('x'));

    assert!(rig.hover_text().is_none());
    assert!(rig.text().ends_with('x'), "{}", rig.text());
}

#[test]
fn escape_takes_a_pinned_box_away_and_does_nothing_else() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());
    rig.press(KeyCode::F(6));
    let before = rig.text();

    rig.press(KeyCode::Esc);

    assert!(rig.hover_text().is_none());
    assert_eq!(rig.text(), before);
}

#[test]
fn a_click_takes_a_pinned_box_away() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());
    rig.press(KeyCode::F(6));

    wheel_at(
        &mut rig,
        MouseEventKind::Down(crossterm::event::MouseButton::Left),
        90,
        14,
    );

    assert!(rig.hover_text().is_none());
}

#[test]
fn the_wheel_elsewhere_leaves_a_pinned_box_up() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());
    rig.press(KeyCode::F(6));

    wheel_at(&mut rig, MouseEventKind::ScrollDown, 90, 14);

    assert!(rig.hover_text().is_some());
}

#[test]
fn a_long_text_shows_fourteen_lines_and_says_which() {
    let mut rig = rig_with_long_text();
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    let screen = rig.screen();

    assert!(screen.contains("line 1 "), "{screen}");
    assert!(!screen.contains("line 9 "), "{screen}");
    assert!(screen.contains("1-14/59"), "{screen}");
}

#[test]
fn the_wheel_over_the_box_scrolls_it_and_the_box_stays_with_the_pointer_on_it() {
    let mut rig = rig_with_long_text();
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());
    let (column, row) = rig.cell_of("line 2");
    // The pointer goes down into the box: that does not take it away.
    rig.pointer_to(column, row);
    assert!(rig.hover_text().is_some());

    wheel_at(&mut rig, MouseEventKind::ScrollDown, column, row);

    let screen = rig.screen();
    // One notch is three lines: the top is now the blank line before `line 3`.
    assert!(screen.contains("line 3 "), "{screen}");
    assert!(!screen.contains("line 2 "), "{screen}");
    assert!(screen.contains("4-17/59"), "{screen}");
    wheel_at(&mut rig, MouseEventKind::ScrollUp, column, row);
    assert!(rig.screen().contains("1-14/59"));
}

#[test]
fn the_wheel_away_from_an_unpinned_box_takes_it_away() {
    let mut rig = rig_with_long_text();
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    wheel_at(&mut rig, MouseEventKind::ScrollDown, 90, 3);

    assert!(rig.hover_text().is_none());
}

#[test]
fn the_arrow_and_page_keys_scroll_a_pinned_box_and_do_not_move_the_cursor() {
    let mut rig = rig_with_long_text();
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());
    rig.press(KeyCode::F(6));
    rig.screen();
    let cursor = rig.app.state().editor().selections().primary().head();

    rig.press(KeyCode::Down);
    assert!(rig.screen().contains("2-15/59"));
    rig.press(KeyCode::PageDown);
    assert!(rig.screen().contains("16-29/59"));
    rig.press(KeyCode::PageUp);
    rig.press(KeyCode::Up);
    assert!(rig.screen().contains("1-14/59"));

    assert!(rig.hover_text().is_some());
    assert_eq!(
        rig.app.state().editor().selections().primary().head(),
        cursor
    );
}

#[test]
fn an_arrow_key_takes_a_box_that_is_not_pinned_away_and_moves_the_cursor() {
    let mut rig = rig_with_long_text();
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());
    let cursor = rig.app.state().editor().selections().primary().head();

    rig.press(KeyCode::Up);

    assert!(rig.hover_text().is_none());
    assert_ne!(
        rig.app.state().editor().selections().primary().head(),
        cursor
    );
}

#[test]
fn f6_on_a_box_that_is_pinned_already_takes_it_away() {
    let mut rig = rig_saying("said");
    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());
    rig.press(KeyCode::F(6));
    assert!(rig.app.state().hover_box().unwrap().is_pinned());

    rig.press(KeyCode::F(6));

    assert!(rig.hover_text().is_none());
}

// ---- the proof: the real rust-analyzer

#[cfg(unix)]
impl Rig {
    /// The pointer rests on `needle` and the box shows up, trying again while the
    /// server is still loading and has nothing to say yet, for up to two minutes.
    fn hover_until_shown(&mut self, needle: &str) {
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        while std::time::Instant::now() < deadline {
            self.rest_on(needle);
            let tried = std::time::Instant::now() + Duration::from_secs(8);
            while std::time::Instant::now() < tried {
                while let Ok(event) = self.events.try_recv() {
                    self.app.handle_event(event);
                }
                if self.hover_text().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            self.pointer_to(100, 15);
        }
        panic!("the server never said anything about {needle:?}");
    }
}

/// The real `rust-analyzer` says what `push` is when the pointer rests on it, and
/// what `Vec` is when `f6` is pressed with the cursor on it. Run by hand:
/// `cargo test real_rust_analyzer_says -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_says_what_a_method_is_under_the_pointer_and_what_a_type_is_at_the_cursor() {
    let source = "fn main() {\n    let mut v: Vec<u32> = Vec::new();\n    v.push(1);\n}\n";
    let mut rig = Rig::real(source);
    rig.screen();

    rig.hover_until_shown("push");

    let said = rig.hover_text().unwrap();
    eprintln!("under the pointer:\n{said}");
    eprintln!("{}", rig.screen());
    assert!(said.contains("push"), "{said}");
    assert!(said.contains("Vec"), "{said}");
    assert!(!said.contains("```"), "{said}");

    rig.press(KeyCode::Esc);
    assert!(rig.hover_text().is_none());
    rig.app.state_mut().edit(|editor| {
        use crate::buffer::Position;
        editor.go_to(Position::new(1, 27));
    });
    rig.press(KeyCode::F(6));
    rig.pump_until(|app| app.state().hover_box().is_some());
    let said = rig.hover_text().unwrap();
    eprintln!("at the cursor:\n{said}");
    assert!(said.contains("Vec"), "{said}");
}

// ---- diagnostics in the box

use crate::lsp::{Diagnostic, Severity};

fn problem(range: std::ops::Range<usize>, severity: Severity, message: &str) -> Diagnostic {
    Diagnostic {
        range,
        severity,
        message: message.to_owned(),
        source: Some("rustc".to_owned()),
        code: Some("E0308".to_owned()),
    }
}

/// `value` of `SOURCE` is bytes 20 to 25 and the `;` is byte 36.
const VALUE: std::ops::Range<usize> = 20..25;
const SEMICOLON: std::ops::Range<usize> = 36..37;

impl Rig {
    fn set_problems(&mut self, problems: Vec<Diagnostic>) {
        let document = self.app.state().workspace().active_document_id().unwrap();
        self.app
            .state_mut()
            .diagnostics_mut()
            .set(document, problems);
    }

    /// The pointer rests exactly on the cell of `needle`'s first character.
    fn rest_exactly_on(&mut self, needle: &str) {
        let (column, row) = self.cell_of(needle);
        self.pointer_to(column, row);
        std::thread::sleep(LONG_ENOUGH);
        self.app.hover_due();
    }

    fn problem_headers(&self) -> Vec<String> {
        self.app
            .state()
            .hover_box()
            .map(|hover| hover.problems().iter().map(|e| e.header.clone()).collect())
            .unwrap_or_default()
    }
}

#[test]
fn a_word_with_a_problem_and_documentation_shows_the_problem_first_then_a_rule_then_the_docs() {
    let mut rig = rig_saying("the type of value");
    rig.set_problems(vec![problem(VALUE, Severity::Error, "mismatched types")]);

    rig.rest_on("value");
    rig.pump_until(|app| app.state().hover_box().is_some());

    assert_eq!(rig.problem_headers(), ["error[E0308] (rustc)"]);
    assert_eq!(rig.hover_text().as_deref(), Some("the type of value"));
    let lines = rig.app.state().hover_box().unwrap().drawn_lines(40);
    let row = |wanted: &dyn Fn(&str) -> bool| lines.iter().position(|line| wanted(line)).unwrap();
    let header = row(&|line| line == "error[E0308] (rustc)");
    let message = row(&|line| line == "mismatched types");
    let rule = row(&|line| !line.is_empty() && line.chars().all(|c| c == '─'));
    let docs = row(&|line| line == "the type of value");
    assert!(
        header < message && message < rule && rule < docs,
        "{lines:?}"
    );
}

#[test]
fn a_pointer_outside_the_range_of_the_problem_shows_only_the_docs() {
    let mut rig = rig_saying("about other");
    rig.set_problems(vec![problem(VALUE, Severity::Error, "mismatched types")]);

    rig.rest_on("other");
    rig.pump_until(|app| app.state().hover_box().is_some());

    assert!(rig.problem_headers().is_empty());
    assert_eq!(rig.hover_text().as_deref(), Some("about other"));
}

#[test]
fn a_problem_alone_is_shown_when_the_server_has_nothing_to_say() {
    let mut rig = Rig::start(hover_provider(), SOURCE, answering(Value::Null));
    rig.screen();
    rig.set_problems(vec![problem(VALUE, Severity::Warning, "unused variable")]);

    rig.rest_on("value");
    rig.pump_until(|app| app.state().hover_box().is_some());

    assert_eq!(rig.problem_headers(), ["warning[E0308] (rustc)"]);
    assert_eq!(rig.hover_text().as_deref(), Some(""));
    let lines = rig.app.state().hover_box().unwrap().drawn_lines(40);
    assert!(
        !lines.iter().any(|line| line.contains('─')),
        "no rule without docs: {lines:?}"
    );
}

#[test]
fn a_problem_is_shown_without_asking_when_the_server_does_not_answer_hover() {
    let mut rig = Rig::start(json!({ "textDocumentSync": 2 }), SOURCE, saying("said"));
    rig.screen();
    rig.set_problems(vec![problem(VALUE, Severity::Error, "mismatched types")]);

    rig.rest_on("value");

    assert!(rig.hover_requests().is_empty());
    assert_eq!(rig.problem_headers().len(), 1);
}

#[test]
fn a_spot_that_is_not_a_word_but_is_in_a_problem_still_shows_it() {
    let mut rig = Rig::start(hover_provider(), SOURCE, answering(Value::Null));
    rig.screen();
    rig.set_problems(vec![problem(
        SEMICOLON,
        Severity::Error,
        "expected something",
    )]);

    rig.rest_exactly_on(";");
    rig.pump_until(|app| app.state().hover_box().is_some());

    assert_eq!(rig.problem_headers().len(), 1);
}

#[test]
fn problems_that_overlap_are_ordered_worst_first() {
    let mut rig = Rig::start(hover_provider(), SOURCE, answering(Value::Null));
    rig.screen();
    rig.set_problems(vec![
        problem(VALUE, Severity::Warning, "a warning"),
        problem(21..24, Severity::Error, "an error"),
        problem(20..25, Severity::Hint, "a hint"),
    ]);

    rig.rest_on("value");
    rig.pump_until(|app| app.state().hover_box().is_some());

    assert_eq!(
        rig.problem_headers(),
        [
            "error[E0308] (rustc)",
            "warning[E0308] (rustc)",
            "hint[E0308] (rustc)"
        ]
    );
}

#[test]
fn neither_a_problem_nor_docs_shows_nothing() {
    let mut rig = Rig::start(hover_provider(), SOURCE, answering(Value::Null));
    rig.screen();
    rig.set_problems(vec![problem(VALUE, Severity::Error, "mismatched types")]);

    rig.rest_on("other");
    std::thread::sleep(Duration::from_millis(200));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }

    assert!(rig.app.state().hover_box().is_none());
}

#[test]
fn f6_on_a_problem_pins_a_box_with_the_problem_and_the_docs() {
    let mut rig = rig_saying("docs");
    rig.set_problems(vec![problem(VALUE, Severity::Error, "mismatched types")]);
    rig.app.state_mut().edit(|editor| {
        editor.go_to(crate::buffer::Position::new(1, 10));
    });
    rig.screen();

    rig.press(KeyCode::F(6));
    rig.pump_until(|app| app.state().hover_box().is_some());

    assert_eq!(rig.problem_headers().len(), 1);
    assert!(rig.app.state().hover_box().unwrap().is_pinned());
}

#[test]
fn f6_on_a_problem_that_is_not_on_a_word_shows_it() {
    let mut rig = Rig::start(hover_provider(), SOURCE, answering(Value::Null));
    rig.set_problems(vec![problem(
        SEMICOLON,
        Severity::Error,
        "expected something",
    )]);
    rig.app.state_mut().edit(|editor| {
        editor.go_to(crate::buffer::Position::new(1, 24));
    });
    rig.screen();

    rig.press(KeyCode::F(6));
    rig.pump_until(|app| app.state().hover_box().is_some());

    assert_eq!(rig.problem_headers().len(), 1);
}
