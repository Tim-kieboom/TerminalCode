//! Breakpoints in the editor: set from the keyboard and the gutter, drawn, moved with the text,
//! kept when a file is closed and remembered for the next run.

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::AppState;
use crate::breakpoints::{Kind, Shown};
use crate::buffer::Position;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::event::action::Action;
use crate::files::paths::canonical;
use crate::persist::saved_state::{self, SavedState};

const SOURCE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";

struct Rig {
    app: App,
    terminal: Terminal<TestBackend>,
    root: PathBuf,
    config: tempfile::TempDir,
    _dir: tempfile::TempDir,
}

fn app_for(root: &Path, config: &Path, saved: SavedState) -> App {
    let mut state = AppState::default();
    state.open_project(root).unwrap();
    App::new(state)
        .with_config_dir(Some(config.to_path_buf()))
        .with_saved_state(saved)
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::write(root.join("main.rs"), SOURCE).unwrap();
    fs::write(root.join("other.rs"), "fn other() {}\n").unwrap();
    let config = tempfile::tempdir().unwrap();
    let app = app_for(&root, config.path(), SavedState::default());
    let mut rig = Rig {
        app,
        terminal: Terminal::new(TestBackend::new(100, 20)).unwrap(),
        root,
        config,
        _dir: dir,
    };
    rig.open("main.rs");
    rig
}

impl Rig {
    fn open(&mut self, name: &str) {
        let path = self.root.join(name);
        self.app
            .state_mut()
            .workspace_mut()
            .open_path(&path)
            .unwrap();
        self.app.handle_event(Event::TerminalChanged);
        self.draw();
    }

    fn draw(&mut self) {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
    }

    fn screen(&self) -> String {
        self.rows().join("\n")
    }

    /// The text of every row of the screen, a cell for each column.
    fn rows(&self) -> Vec<String> {
        let buffer = self.terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_owned())
                    .collect()
            })
            .collect()
    }

    /// The columns before `text` on its row: the gutter is in them, and so is what is left of
    /// the editor.
    fn gutter_of(&mut self, text: &str) -> String {
        self.draw();
        let rows = self.rows();
        let row = rows
            .iter()
            .find(|row| row.contains(text))
            .unwrap_or_else(|| panic!("{text} is not on the screen:\n{}", self.screen()));
        let at = row.find(text).unwrap();
        row[..at].to_owned()
    }

    fn go_to(&mut self, line: usize) {
        self.app
            .state_mut()
            .edit(|editor| editor.go_to(Position::new(line, 0)));
    }

    fn f9(&mut self) {
        self.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::F(9),
            KeyModifiers::NONE,
        )));
    }

    fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn path(&self, name: &str) -> PathBuf {
        canonical(&self.root.join(name))
    }

    fn lines(&self, name: &str) -> Vec<usize> {
        self.app.breakpoints_of(&self.path(name))
    }

    fn state_json(&self) -> String {
        fs::read_to_string(self.config.path().join("state.json")).unwrap_or_default()
    }

    fn document(&self) -> crate::ids::DocumentId {
        self.app.state().workspace().active_document_id().unwrap()
    }

    fn shown(&self) -> Vec<Shown> {
        self.app.state().breakpoints().shown(self.document())
    }

    /// A click at the column the marker of `line` is in.
    fn click_marker_of(&mut self, text: &str) {
        let row = self.row_of(text);
        let column = self.gutter_of(text).chars().count() - 1;
        self.click(column as u16, row);
    }

    fn row_of(&mut self, text: &str) -> u16 {
        self.draw();
        self.rows()
            .iter()
            .position(|row| row.contains(text))
            .unwrap() as u16
    }

    fn click(&mut self, column: u16, row: u16) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }
}

#[test]
fn f9_sets_a_breakpoint_on_the_line_of_the_cursor_and_a_second_takes_it_away() {
    let mut rig = rig();
    rig.go_to(2);

    rig.f9();

    assert_eq!(rig.lines("main.rs"), [2]);
    assert!(
        rig.gutter_of("let b = 2;").contains('●'),
        "{}",
        rig.screen()
    );
    assert!(!rig.gutter_of("let a = 1;").contains('●'));
    rig.f9();
    assert!(rig.lines("main.rs").is_empty());
    assert!(!rig.gutter_of("let b = 2;").contains('●'));
}

#[test]
fn a_click_on_the_marker_column_toggles_and_a_click_on_the_number_selects_the_line() {
    let mut rig = rig();

    rig.click_marker_of("    let c = 3;");
    assert_eq!(rig.lines("main.rs"), [3]);
    rig.click_marker_of("    let c = 3;");
    assert!(
        rig.lines("main.rs").is_empty(),
        "the second click takes it away"
    );

    // The number itself: the line is selected, no breakpoint.
    let row = rig.row_of("let a = 1;");
    let marker = rig.gutter_of("    let a = 1;").chars().count() - 1;
    rig.click((marker - 1) as u16, row);
    assert!(rig.lines("main.rs").is_empty());
    let selection = *rig.app.state().editor().selections().primary();
    assert_eq!(selection.start().line, 1, "the line was selected");
}

#[test]
fn a_file_with_no_name_has_no_breakpoint_and_says_so() {
    let mut rig = rig();
    rig.app.state_mut().workspace_mut().new_file();

    rig.f9();

    assert!(rig.app.state().breakpoints().is_empty());
    let said = rig.app.state().notifications().texts();
    assert!(
        said.iter().any(|told| told.contains("save this one first")),
        "{said:?}"
    );
}

#[test]
fn lines_typed_above_move_the_breakpoint_down_with_its_code() {
    let mut rig = rig();
    rig.go_to(2);
    rig.f9();

    rig.go_to(0);
    rig.press(KeyCode::Enter);

    assert_eq!(rig.lines("main.rs"), [3]);
    assert!(rig.gutter_of("let b = 2;").contains('●'));
    assert!(!rig.gutter_of("let a = 1;").contains('●'));
}

#[test]
fn enter_at_the_start_of_the_line_takes_the_breakpoint_down_with_the_code() {
    let mut rig = rig();
    rig.go_to(2);
    rig.f9();

    rig.go_to(2);
    rig.press(KeyCode::Enter);

    assert_eq!(rig.lines("main.rs"), [3]);
    assert!(rig.gutter_of("let b = 2;").contains('●'));
}

#[test]
fn an_edit_that_an_event_made_moves_the_breakpoint_too() {
    let mut rig = rig();
    rig.go_to(2);
    rig.f9();

    // Not an input: what a format that came back from a language server does.
    rig.app.state_mut().edit(|editor| {
        editor.go_to(Position::new(0, 0));
        let _ = editor.insert_text("// header\n");
    });
    rig.app.handle_event(Event::TerminalChanged);

    assert_eq!(rig.lines("main.rs"), [3]);
}

#[test]
fn closing_a_file_keeps_its_breakpoints_and_opening_it_again_shows_them() {
    let mut rig = rig();
    rig.go_to(1);
    rig.f9();
    rig.go_to(0);
    rig.press(KeyCode::Enter);
    assert_eq!(rig.lines("main.rs"), [2]);
    // Saved first: a tab with changes asks before it closes.
    rig.app.run_action(Action::Save);

    rig.app.run_action(Action::CloseTab);
    rig.app.handle_event(Event::TerminalChanged);
    assert!(rig.app.state().breakpoints().attached().is_empty());
    assert_eq!(rig.lines("main.rs"), [2], "kept where it was");
    // What is on disk has the new first line, so the text of the line is the same.
    rig.open("main.rs");

    assert_eq!(
        rig.shown(),
        [Shown {
            line: 2,
            kind: Kind::Set
        }]
    );
}

#[test]
fn breakpoints_are_written_to_the_state_file_by_the_file_inside_the_project() {
    let mut rig = rig();
    rig.go_to(2);
    rig.f9();

    let json = rig.state_json();

    assert!(json.contains("\"breakpoints\""), "{json}");
    assert!(json.contains("\"file\": \"main.rs\""), "{json}");
    assert!(json.contains("\"line\": 2"), "{json}");
    assert!(json.contains("let b = 2;"), "{json}");
}

#[test]
fn the_next_run_of_the_editor_has_them_on_the_same_lines_when_the_text_is_the_same() {
    let mut rig = rig();
    rig.go_to(2);
    rig.f9();
    let saved = saved_state::load(Some(rig.config.path())).0;

    let app = app_for(&rig.root, rig.config.path(), saved);
    rig.app = app;
    rig.open("main.rs");

    assert_eq!(rig.lines("main.rs"), [2]);
    assert_eq!(
        rig.shown(),
        [Shown {
            line: 2,
            kind: Kind::Set
        }]
    );
    assert!(rig.gutter_of("let b = 2;").contains('●'));
}

#[test]
fn a_file_that_changed_outside_the_editor_shows_the_breakpoint_dim_on_its_line() {
    let mut rig = rig();
    rig.go_to(2);
    rig.f9();
    let saved = saved_state::load(Some(rig.config.path())).0;
    // Another program rewrote the file: line 2 says something else now.
    fs::write(
        rig.root.join("main.rs"),
        "fn main() {\n    let a = 1;\n    let renamed = 2;\n}\n",
    )
    .unwrap();

    let app = app_for(&rig.root, rig.config.path(), saved);
    rig.app = app;
    rig.open("main.rs");

    assert_eq!(
        rig.shown(),
        [Shown {
            line: 2,
            kind: Kind::Moved
        }]
    );
}

#[test]
fn remove_all_breakpoints_takes_them_from_every_file_and_the_state_file() {
    let mut rig = rig();
    rig.go_to(1);
    rig.f9();
    rig.open("other.rs");
    rig.go_to(0);
    rig.f9();
    assert_eq!(rig.app.state().breakpoints().len(), 2);

    rig.app.run_action(Action::ClearBreakpoints);

    assert!(rig.app.state().breakpoints().is_empty());
    assert!(
        !rig.state_json().contains("\"breakpoints\""),
        "{}",
        rig.state_json()
    );
    let said = rig.app.state().notifications().texts();
    assert!(
        said.iter()
            .any(|told| told.contains("removed 2 breakpoints")),
        "{said:?}"
    );
}

#[test]
fn the_palette_has_the_two_commands() {
    let titles: Vec<&str> = Action::palette_actions()
        .iter()
        .filter_map(Action::title)
        .collect();

    assert!(titles.contains(&"Debug: Toggle Breakpoint"));
    assert!(titles.contains(&"Debug: Remove All Breakpoints"));
}
