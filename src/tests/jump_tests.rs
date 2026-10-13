//! Back and forward through the places the cursor has jumped from.

use std::fs;
use std::path::PathBuf;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::buffer::Position;
use crate::components::prepare_and_render;
use crate::initial_state;

/// `count` lines, each `line N`.
fn lines(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

struct Rig {
    app: App,
    dir: tempfile::TempDir,
    files: Vec<PathBuf>,
}

fn rig(texts: &[&str]) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let mut files = Vec::new();
    for (number, text) in texts.iter().enumerate() {
        let path = dir.path().join(format!("f{}.txt", number + 1));
        fs::write(&path, text).unwrap();
        files.push(path);
    }
    let mut state = initial_state(&files).unwrap();
    state.open_project(dir.path()).unwrap();
    let mut rig = Rig {
        app: App::new(state),
        dir,
        files,
    };
    rig.draw();
    rig
}

impl Rig {
    fn draw(&mut self) {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let app = &mut self.app;
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
    }

    fn key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
    }

    fn press(&mut self, code: KeyCode) {
        self.key(code, KeyModifiers::NONE);
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c));
        }
    }

    /// `ctrl+g`, the line, `enter`.
    fn go_to_line(&mut self, line: usize) {
        self.key(KeyCode::Char('g'), KeyModifiers::CONTROL);
        self.type_text(&line.to_string());
        self.press(KeyCode::Enter);
    }

    fn back(&mut self) {
        self.key(KeyCode::Char(','), KeyModifiers::ALT);
    }

    fn forward(&mut self) {
        self.key(KeyCode::Char('.'), KeyModifiers::ALT);
    }

    fn line(&self) -> usize {
        self.app.state().editor().selections().primary().head().line + 1
    }

    fn file_name(&self) -> String {
        self.app.state().editor().display_name()
    }

    fn told(&self) -> String {
        self.app.state().notifications().texts().join("|")
    }

    fn click_line(&mut self, line: usize) {
        self.draw();
        let area = self.app.state().editor().text_area().unwrap();
        let row =
            area.y + u16::try_from(line - 1 - self.app.state().editor().scroll().top).unwrap();
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 1,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }
}

#[test]
fn go_to_line_records_where_it_left_and_back_returns_there_and_forward_again() {
    let mut rig = rig(&[&lines(100)]);
    rig.go_to_line(40);
    assert_eq!(rig.line(), 40);

    rig.back();

    assert_eq!(rig.line(), 1);
    rig.forward();
    assert_eq!(rig.line(), 40);
}

#[test]
fn several_jumps_are_walked_back_in_order() {
    let mut rig = rig(&[&lines(100)]);
    rig.go_to_line(20);
    rig.go_to_line(50);
    rig.go_to_line(80);

    rig.back();
    assert_eq!(rig.line(), 50);
    rig.back();
    assert_eq!(rig.line(), 20);
    rig.back();
    assert_eq!(rig.line(), 1);
    rig.forward();
    rig.forward();
    rig.forward();
    assert_eq!(rig.line(), 80);
}

#[test]
fn with_nothing_recorded_back_and_forward_say_so() {
    let mut rig = rig(&[&lines(10)]);

    rig.back();
    assert!(rig.told().contains("no earlier place"), "{}", rig.told());
    rig.forward();
    assert!(rig.told().contains("no later place"), "{}", rig.told());
    assert_eq!(rig.line(), 1);
}

#[test]
fn moving_typing_and_paging_record_nothing() {
    let mut rig = rig(&[&lines(100)]);

    rig.press(KeyCode::Down);
    rig.press(KeyCode::PageDown);
    rig.press(KeyCode::PageDown);
    rig.key(KeyCode::End, KeyModifiers::CONTROL);
    rig.type_text("x");
    rig.back();

    assert!(rig.told().contains("no earlier place"), "{}", rig.told());
}

#[test]
fn another_file_in_front_records_the_place_it_left() {
    let mut rig = rig(&[&lines(100), &lines(10)]);
    rig.go_to_line(30);
    // The second file is the one in front after the next tab.
    rig.key(KeyCode::PageDown, KeyModifiers::CONTROL);
    assert_eq!(rig.file_name(), "f2.txt");

    rig.back();

    assert_eq!(rig.file_name(), "f1.txt");
    assert_eq!(rig.line(), 30);
}

#[test]
fn a_click_more_than_a_screen_away_is_a_jump_and_a_near_one_is_not() {
    let mut rig = rig(&[&lines(300)]);
    rig.click_line(5);
    rig.back();
    assert!(rig.told().contains("no earlier place"), "{}", rig.told());

    // The cursor goes far down, and the view is scrolled back to the top with the
    // wheel: a click there is far from the cursor.
    rig.go_to_line(250);
    rig.draw();
    for _ in 0..120 {
        rig.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 50,
            row: 10,
            modifiers: KeyModifiers::NONE,
        }));
    }
    rig.click_line(3);
    assert_eq!(rig.line(), 3);

    rig.back();

    assert_eq!(rig.line(), 250);
}

#[test]
fn places_move_with_edits() {
    let mut rig = rig(&[&lines(100)]);
    rig.go_to_line(40);
    rig.go_to_line(80);
    // Three lines are added at the very top.
    rig.key(KeyCode::Home, KeyModifiers::CONTROL);
    for _ in 0..3 {
        rig.press(KeyCode::Enter);
    }

    rig.go_to_line(90);
    rig.back();
    assert_eq!(
        rig.line(),
        3 + 1,
        "the cursor was there when the jump left it"
    );
    rig.back();
    assert_eq!(rig.line(), 40 + 3, "what was on line 40 is on line 43");
    rig.back();
    assert_eq!(rig.line(), 1);
}

#[test]
fn a_new_jump_after_going_back_drops_what_was_ahead() {
    let mut rig = rig(&[&lines(100)]);
    rig.go_to_line(20);
    rig.go_to_line(50);
    rig.back();
    assert_eq!(rig.line(), 20);

    rig.go_to_line(70);
    rig.forward();

    assert_eq!(rig.line(), 70);
    assert!(rig.told().contains("no later place"), "{}", rig.told());
}

#[test]
fn a_closed_file_is_opened_again_by_back() {
    let mut rig = rig(&[&lines(100), &lines(10)]);
    rig.go_to_line(30);
    rig.key(KeyCode::PageDown, KeyModifiers::CONTROL);
    // Close the first file's tab from the second: go to it, close it, come back.
    rig.key(KeyCode::PageUp, KeyModifiers::CONTROL);
    rig.key(KeyCode::Char('w'), KeyModifiers::CONTROL);
    assert_eq!(rig.file_name(), "f2.txt");

    rig.back();

    assert_eq!(rig.file_name(), "f1.txt");
    assert_eq!(rig.line(), 30);
}

#[test]
fn a_place_whose_file_is_gone_is_skipped_with_a_word() {
    let mut rig = rig(&[&lines(10), &lines(10), &lines(10)]);
    // f1, then f2, then f3 in front: f1 and f2 are places left.
    rig.key(KeyCode::PageDown, KeyModifiers::CONTROL);
    rig.key(KeyCode::PageDown, KeyModifiers::CONTROL);
    assert_eq!(rig.file_name(), "f3.txt");
    // Close f2 and take its file away: its place cannot be gone to.
    rig.key(KeyCode::PageUp, KeyModifiers::CONTROL);
    assert_eq!(rig.file_name(), "f2.txt");
    rig.key(KeyCode::Char('w'), KeyModifiers::CONTROL);
    fs::remove_file(&rig.files[1]).unwrap();

    for _ in 0..4 {
        rig.back();
        if rig.told().contains("skipped") {
            break;
        }
    }

    assert!(rig.told().contains("skipped one place"), "{}", rig.told());
    assert_ne!(rig.file_name(), "f2.txt");
    let _ = &rig.dir;
}

#[test]
fn going_back_and_forward_records_nothing_of_its_own() {
    let mut rig = rig(&[&lines(100)]);
    rig.go_to_line(20);
    rig.go_to_line(50);

    for _ in 0..3 {
        rig.back();
        rig.forward();
    }

    assert_eq!(rig.line(), 50);
    rig.back();
    assert_eq!(rig.line(), 20);
    rig.back();
    assert_eq!(rig.line(), 1);
    rig.back();
    assert!(rig.told().contains("no earlier place"), "{}", rig.told());
    let _ = Position::new(0, 0);
}
