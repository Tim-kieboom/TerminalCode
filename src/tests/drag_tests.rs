//! Dragging a file or folder in the explorer onto a folder, with the mouse.

use std::fs;
use std::path::Path;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::AppState;
use crate::components::prepare_and_render;

struct Screen {
    app: App,
    terminal: Terminal<TestBackend>,
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
    fs::write(dir.path().join("b.txt"), "beta\n").unwrap();
    dir
}

fn screen_in(root: &Path) -> Screen {
    let mut state = AppState::default();
    state.open_project(root).unwrap();
    let mut screen = Screen {
        app: App::new(state),
        terminal: Terminal::new(TestBackend::new(100, 30)).unwrap(),
    };
    screen.draw();
    screen
}

impl Screen {
    fn draw(&mut self) {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
    }

    fn mouse(&mut self, kind: MouseEventKind, (column, row): (u16, u16)) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
        self.draw();
    }

    fn press(&mut self, at: (u16, u16)) {
        self.mouse(MouseEventKind::Down(MouseButton::Left), at);
    }

    fn drag(&mut self, at: (u16, u16)) {
        self.mouse(MouseEventKind::Drag(MouseButton::Left), at);
    }

    fn release(&mut self, at: (u16, u16)) {
        self.mouse(MouseEventKind::Up(MouseButton::Left), at);
    }

    /// The cell of the row called `name`.
    fn row(&self, name: &str) -> (u16, u16) {
        let explorer = self.app.state().explorer();
        let index = explorer
            .rows()
            .iter()
            .position(|row| row.name == name)
            .unwrap_or_else(|| panic!("no row {name}"));
        let body = explorer.body_for_tests();
        (body.x + 4, body.y + index as u16)
    }

    /// Drags the row `from` onto the row `onto` and lets go.
    fn drag_and_drop(&mut self, from: &str, onto: &str) {
        let (start, end) = (self.row(from), self.row(onto));
        self.press(start);
        self.drag(end);
        self.release(end);
    }

    fn notified(&self, text: &str) -> bool {
        self.app
            .state()
            .notifications()
            .texts()
            .iter()
            .any(|shown| shown.contains(text))
    }

    fn tab_names(&self) -> Vec<String> {
        self.app.state().workspace().tab_names().0
    }
}

#[test]
fn dropping_a_file_on_a_folder_moves_it_there() {
    let dir = project();
    let mut screen = screen_in(dir.path());

    screen.drag_and_drop("a.txt", "docs");

    assert!(!dir.path().join("a.txt").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("docs/a.txt")).unwrap(),
        "alpha\n"
    );
    assert!(screen.notified("moved a.txt into docs"));
}

#[test]
fn the_moved_file_is_selected_in_its_new_place() {
    let dir = project();
    let mut screen = screen_in(dir.path());

    screen.drag_and_drop("a.txt", "docs");

    let explorer = screen.app.state().explorer();
    assert_eq!(
        explorer.selected_row().unwrap().path,
        dir.path().join("docs/a.txt")
    );
}

#[test]
fn dropping_a_folder_moves_it_with_what_is_in_it() {
    let dir = project();
    let mut screen = screen_in(dir.path());

    screen.drag_and_drop("src", "docs");

    assert!(!dir.path().join("src").exists());
    assert!(dir.path().join("docs/src/main.rs").is_file());
}

#[test]
fn a_press_and_drag_opens_no_tab() {
    let dir = project();
    let mut screen = screen_in(dir.path());
    let before = screen.tab_names();

    screen.drag_and_drop("a.txt", "docs");

    assert_eq!(screen.tab_names(), before);
}

#[test]
fn a_plain_click_still_opens_the_file() {
    let dir = project();
    let mut screen = screen_in(dir.path());
    let at = screen.row("a.txt");
    let before = screen.tab_names();

    screen.press(at);
    assert_eq!(screen.tab_names(), before, "opens on release");
    screen.release(at);

    assert_ne!(screen.tab_names(), before);
    assert_eq!(screen.app.state().editor().buffer().text(), "alpha\n");
}

#[test]
fn a_tab_of_the_moved_file_follows_it_with_its_unsaved_text() {
    let dir = project();
    let mut screen = screen_in(dir.path());
    let at = screen.row("a.txt");
    screen.press(at);
    screen.release(at);
    screen
        .app
        .state_mut()
        .workspace_mut()
        .with_editor(|editor| editor.insert_text("x").unwrap());

    screen.drag_and_drop("a.txt", "docs");

    assert!(screen.tab_names().iter().any(|name| name.contains("a.txt")));
    assert_eq!(screen.app.state().editor().buffer().text(), "xalpha\n");
    assert_eq!(
        screen.app.state().editor().buffer().path(),
        Some(dir.path().join("docs/a.txt").as_path())
    );
    assert!(!dir.path().join("a.txt").exists());
}

#[test]
fn tabs_of_files_inside_a_moved_folder_follow() {
    let dir = project();
    let mut screen = screen_in(dir.path());
    screen
        .app
        .state_mut()
        .workspace_mut()
        .open_path(&dir.path().join("src/main.rs"))
        .unwrap();
    screen.draw();

    screen.drag_and_drop("src", "docs");

    assert_eq!(
        screen.app.state().editor().buffer().path(),
        Some(dir.path().join("docs/src/main.rs").as_path())
    );
}

#[test]
fn a_name_that_is_taken_refuses_the_move_and_says_so() {
    let dir = project();
    fs::write(dir.path().join("docs/a.txt"), "other\n").unwrap();
    let mut screen = screen_in(dir.path());

    screen.drag_and_drop("a.txt", "docs");

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "alpha\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("docs/a.txt")).unwrap(),
        "other\n"
    );
    assert!(screen.notified("already exists"));
    assert!(screen.app.state().notifications().has_errors());
}

#[test]
fn a_row_that_vanished_while_dragging_is_reported_not_a_crash() {
    let dir = project();
    let mut screen = screen_in(dir.path());
    let (start, end) = (screen.row("a.txt"), screen.row("docs"));
    screen.press(start);
    screen.drag(end);
    fs::remove_file(dir.path().join("a.txt")).unwrap();

    screen.release(end);

    assert!(screen.app.state().notifications().has_errors());
    assert!(!dir.path().join("docs/a.txt").exists());
}

#[test]
fn escape_gives_up_the_drag() {
    let dir = project();
    let mut screen = screen_in(dir.path());
    let (start, end) = (screen.row("a.txt"), screen.row("docs"));
    screen.press(start);
    screen.drag(end);

    screen.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::NONE,
    )));
    screen.release(end);

    assert!(dir.path().join("a.txt").exists());
    assert!(!dir.path().join("docs/a.txt").exists());
    assert!(screen.app.state().explorer().drop_target().is_none());
}

#[test]
fn letting_go_outside_the_explorer_moves_nothing() {
    let dir = project();
    let mut screen = screen_in(dir.path());
    let start = screen.row("a.txt");
    let before = screen.tab_names();
    screen.press(start);
    screen.drag((60, 10));

    screen.release((60, 10));

    assert!(dir.path().join("a.txt").exists());
    assert_eq!(screen.tab_names(), before);
}

#[test]
fn the_folder_under_the_pointer_is_drawn_as_the_drop_target() {
    let dir = project();
    let mut screen = screen_in(dir.path());
    let (start, end) = (screen.row("a.txt"), screen.row("docs"));
    screen.press(start);

    screen.drag(end);

    let buffer = screen.terminal.backend().buffer().clone();
    let style = buffer[(end.0, end.1)].style();
    let theme = screen.app.state().theme();
    assert_eq!(style.bg, theme.style("explorer.drop").bg);
    // And the rows around it are not.
    let other = screen.row("b.txt");
    assert_ne!(
        buffer[(other.0, other.1)].style().bg,
        theme.style("explorer.drop").bg
    );
}

#[test]
fn after_the_drop_nothing_is_highlighted_any_more() {
    let dir = project();
    let mut screen = screen_in(dir.path());
    screen.drag_and_drop("a.txt", "docs");

    assert!(screen.app.state().explorer().drop_target().is_none());
    assert!(!screen.app.state().explorer().is_dragging());
}

#[test]
fn the_editor_gets_no_selection_from_a_drag_that_started_in_the_explorer() {
    let dir = project();
    let mut screen = screen_in(dir.path());
    let at = screen.row("a.txt");
    screen.press(at);
    screen.release(at);
    let before = screen.app.state().editor().selections().primary().head();

    screen.press(screen.row("b.txt"));
    screen.drag((60, 5));
    screen.release((60, 5));

    assert_eq!(
        screen.app.state().editor().selections().primary().head(),
        before
    );
}
