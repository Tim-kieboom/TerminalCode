//! Paths in the output of the terminal pane: `ctrl` underlines the one under the pointer and a
//! `ctrl+click` opens it at its line and column; a plain click still selects, a program that
//! asked for the mouse keeps the click, and only what is there in the project is a link.

#![cfg(unix)]

use crossterm::event::MouseButton::Left;
use crossterm::event::MouseEventKind::{Down, Moved, Up};
use ratatui::style::Modifier;

use super::*;

/// A shell that shows where it failed, and then waits.
const OUTPUT: &str =
    "printf 'error: bad\\n  --> src/main.rs:3:5\\nsee missing.rs:9 and src/\\n'; sleep 30";

/// The same output, then mouse reporting on.
const OUTPUT_MOUSE_ON: &str = "printf 'error: bad\\n  --> src/main.rs:3:5\\n\\033[?1000h\\033[?1006h'; stty raw -echo; cat -v";

fn project_with_a_file() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/main.rs"),
        "fn main() {\n    let a = 1;\n    let b = 22;\n}\n",
    )
    .unwrap();
    dir
}

fn shown_in(dir: &tempfile::TempDir, script: &str) -> Rig {
    let mut rig = rig_in(dir.path(), script);
    rig.toggle();
    rig.wait_for_text("src/main.rs:3:5");
    std::thread::sleep(Duration::from_millis(200));
    rig.draw();
    rig
}

impl Rig {
    /// The cell (screen column, row) `offset` columns into `text` on the shell's screen.
    fn cell_of(&self, text: &str, offset: u16) -> (u16, u16) {
        let (ox, oy) = self.text_origin();
        let screen = self
            .app
            .state()
            .terminal()
            .session()
            .unwrap()
            .with_screen(|screen| {
                screen
                    .contents()
                    .lines()
                    .position(|line| line.contains(text))
                    .map(|row| {
                        let line = screen.contents().lines().nth(row).unwrap().to_owned();
                        (row as u16, line.find(text).unwrap() as u16)
                    })
            });
        let (row, column) = screen.unwrap_or_else(|| panic!("{text} is not on the screen"));
        (ox + column + offset, oy + row)
    }

    fn move_to(&mut self, cell: (u16, u16), modifiers: KeyModifiers) {
        self.app
            .handle_input(mouse(Moved, cell.0, cell.1, modifiers));
        self.draw();
    }

    fn press_at(&mut self, cell: (u16, u16), modifiers: KeyModifiers) {
        self.app
            .handle_input(mouse(Down(Left), cell.0, cell.1, modifiers));
        self.app
            .handle_input(mouse(Up(Left), cell.0, cell.1, modifiers));
        self.draw();
    }

    fn underlined(&self, cell: (u16, u16)) -> bool {
        self.terminal.backend().buffer()[cell]
            .modifier
            .contains(Modifier::UNDERLINED)
    }

    /// The name of the file in the editor, if there is one.
    fn editor_file(&self) -> Option<String> {
        self.app
            .state()
            .editor()
            .buffer()
            .path()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
    }
}

#[test]
fn ctrl_click_on_a_path_opens_the_file_at_its_line_and_column() {
    let dir = project_with_a_file();
    let mut rig = shown_in(&dir, OUTPUT);

    let on_the_path = rig.cell_of("src/main.rs:3:5", 5);
    rig.press_at(on_the_path, KeyModifiers::CONTROL);

    assert_eq!(rig.editor_file().as_deref(), Some("main.rs"));
    let head = rig.app.state().editor().selections().primary().head();
    assert_eq!((head.line, head.column), (2, 4), "line 3, column 5 from 1");
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

#[test]
fn ctrl_underlines_the_path_under_the_pointer_and_only_that() {
    let dir = project_with_a_file();
    let mut rig = shown_in(&dir, OUTPUT);
    let start = rig.cell_of("src/main.rs:3:5", 0);
    let inside = (start.0 + 4, start.1);

    rig.move_to(inside, KeyModifiers::CONTROL);
    assert!(rig.underlined(start), "the first character");
    assert!(
        rig.underlined((start.0 + 14, start.1)),
        "the last, with its place"
    );
    assert!(
        !rig.underlined((start.0 - 2, start.1)),
        "not the arrow before it"
    );

    // Without `ctrl` the underline goes.
    rig.move_to(inside, KeyModifiers::NONE);
    assert!(!rig.underlined(start));
}

#[test]
fn a_plain_click_on_a_path_selects_and_opens_nothing() {
    let dir = project_with_a_file();
    let mut rig = shown_in(&dir, OUTPUT);

    let on_the_path = rig.cell_of("src/main.rs:3:5", 5);
    rig.press_at(on_the_path, KeyModifiers::NONE);

    assert_eq!(rig.editor_file(), None);
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
}

#[test]
fn a_path_that_is_not_there_is_not_a_link() {
    let dir = project_with_a_file();
    let mut rig = shown_in(&dir, OUTPUT);
    let on_it = rig.cell_of("missing.rs:9", 3);

    rig.move_to(on_it, KeyModifiers::CONTROL);
    rig.press_at(on_it, KeyModifiers::CONTROL);

    assert!(!rig.underlined(on_it));
    assert_eq!(rig.editor_file(), None);
}

#[test]
fn a_folder_is_shown_in_the_explorer() {
    let dir = project_with_a_file();
    let mut rig = shown_in(&dir, OUTPUT);
    let on_the_folder = rig.cell_of("and src/", 5);

    rig.press_at(on_the_folder, KeyModifiers::CONTROL);

    let selected = rig
        .app
        .state()
        .explorer()
        .selected_row()
        .map(|row| row.path.clone());
    assert_eq!(selected, Some(dir.path().join("src")));
    assert_eq!(rig.editor_file(), None, "a folder opens no tab");
}

#[test]
fn a_program_that_asked_for_the_mouse_keeps_the_click_unless_shift_is_held() {
    let dir = project_with_a_file();
    let mut rig = rig_in(dir.path(), OUTPUT_MOUSE_ON);
    rig.toggle();
    rig.wait_for_text("src/main.rs:3:5");
    rig.wait_for("mouse reporting", |rig| {
        rig.app.state().terminal().session().is_some_and(|session| {
            session.with_screen(|screen| {
                screen.mouse_protocol_mode() != vt100::MouseProtocolMode::None
            })
        })
    });
    std::thread::sleep(Duration::from_millis(200));
    let on_the_path = rig.cell_of("src/main.rs:3:5", 5);

    rig.press_at(on_the_path, KeyModifiers::CONTROL);
    assert_eq!(rig.editor_file(), None, "the program got the click");
    rig.move_to(on_the_path, KeyModifiers::CONTROL);
    assert!(!rig.underlined(on_the_path));

    rig.press_at(on_the_path, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    assert_eq!(rig.editor_file().as_deref(), Some("main.rs"));
}
