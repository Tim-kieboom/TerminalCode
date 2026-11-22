//! The list under the Open File and Open Folder prompts: the folder typed so far, matched
//! against the name being typed; `tab` completes, `enter` goes into a folder or opens a file,
//! backspace after a separator steps up.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Popup};
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::prepare_and_render;
use crate::event::action::Action;

struct Rig {
    app: App,
    dir: tempfile::TempDir,
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/components")).unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.path().join("src/components/mod.rs"), "// mod\n").unwrap();
    fs::write(dir.path().join("alpha.txt"), "alpha\n").unwrap();
    fs::write(dir.path().join("alpine.txt"), "alpine\n").unwrap();
    fs::write(dir.path().join(".hidden"), "x\n").unwrap();
    dir
}

fn rig() -> Rig {
    let dir = project();
    let mut state = AppState::new(Editor::new(Buffer::default()));
    state.open_project(dir.path()).unwrap();
    Rig {
        app: App::new(state),
        dir,
    }
}

impl Rig {
    fn key(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.key(KeyCode::Char(c));
        }
    }

    fn name(&self) -> String {
        match self.app.state().popup() {
            Popup::NewEntry(prompt) => prompt.name().to_owned(),
            other => panic!("no prompt: {other:?}"),
        }
    }

    fn screen(&mut self) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let app = &mut self.app;
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        terminal.backend().to_string()
    }

    /// The text inside the prompt's box, one line for each row of it.
    fn box_text(&mut self) -> String {
        let screen = self.screen();
        let lines: Vec<&str> = screen.lines().map(|line| line.trim_matches('"')).collect();
        let top = lines
            .iter()
            .position(|line| line.contains('╭'))
            .expect("no box");
        let left = lines[top].chars().position(|c| c == '╭').unwrap();
        let mut text = String::new();
        for line in &lines[top..] {
            let row: String = line.chars().skip(left).collect();
            text.push_str(&row);
            text.push('\n');
            if row.starts_with('╰') {
                break;
            }
        }
        text
    }

    fn told(&self) -> String {
        self.app.state().notifications().texts().join(" | ")
    }

    fn file(&self) -> Option<String> {
        self.app
            .state()
            .editor()
            .buffer()
            .path()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
    }
}

#[test]
fn opening_the_prompt_lists_the_project_folder_with_folders_first() {
    let mut rig = rig();

    rig.app.do_action(Action::OpenFile);
    let screen = rig.box_text();

    let at = |text: &str| {
        screen
            .find(text)
            .unwrap_or_else(|| panic!("{text}: {screen}"))
    };
    assert!(at("docs/") < at("src/"));
    assert!(at("src/") < at("alpha.txt"));
    assert!(
        !screen.contains(".hidden"),
        "names with a dot wait for a dot"
    );
}

#[test]
fn typing_narrows_the_list_by_the_letters_in_order() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    rig.type_text("alp");
    let screen = rig.box_text();

    assert!(
        screen.contains("alpha.txt") && screen.contains("alpine.txt"),
        "{screen}"
    );
    assert!(!screen.contains("docs/"), "{screen}");

    rig.type_text("h");
    let screen = rig.box_text();
    assert!(
        screen.contains("alpha.txt") && !screen.contains("alpine.txt"),
        "{screen}"
    );
}

#[test]
fn a_name_that_has_the_letters_but_not_the_start_matches_too() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    rig.type_text("cmps/");
    rig.key(KeyCode::Backspace);
    rig.type_text("src/cmp");
    let screen = rig.box_text();

    assert!(screen.contains("components/"), "{screen}");
}

#[test]
fn enter_on_a_folder_goes_into_it_and_on_a_file_opens_it() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    rig.type_text("sr");
    rig.key(KeyCode::Enter);
    assert_eq!(rig.name(), "src/", "into the folder");
    assert!(rig.box_text().contains("main.rs"));

    rig.type_text("ma");
    rig.key(KeyCode::Enter);

    assert_eq!(rig.file().as_deref(), Some("main.rs"));
    assert!(
        !matches!(rig.app.state().popup(), Popup::NewEntry(_)),
        "the prompt closed"
    );
}

#[test]
fn the_arrows_move_the_highlight_and_enter_takes_it() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);
    rig.type_text("al");

    rig.key(KeyCode::Down);
    rig.key(KeyCode::Enter);

    assert_eq!(rig.file().as_deref(), Some("alpine.txt"));
}

#[test]
fn with_nothing_highlighted_enter_uses_what_was_typed() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    rig.type_text("nope.txt");
    rig.key(KeyCode::Enter);

    assert!(rig.told().contains("nope.txt"), "{}", rig.told());
    assert_eq!(rig.name(), "nope.txt", "the prompt stays");
}

#[test]
fn tab_completes_what_the_matches_share_and_then_the_highlighted_entry() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    rig.type_text("al");
    rig.key(KeyCode::Tab);
    assert_eq!(rig.name(), "alp", "alpha.txt and alpine.txt share alp");

    rig.key(KeyCode::Tab);
    rig.key(KeyCode::Tab);
    assert_eq!(
        rig.name(),
        "alpha.txt",
        "nothing more shared: the first is highlighted, then taken"
    );
}

#[test]
fn tab_on_a_folder_writes_its_name_with_a_slash() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    rig.type_text("sr");
    rig.key(KeyCode::Tab);
    assert_eq!(rig.name(), "src");
    rig.key(KeyCode::Tab);

    assert_eq!(rig.name(), "src/");
}

#[test]
fn backspace_after_a_separator_steps_up_a_folder() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);
    rig.type_text("src/components/");

    rig.key(KeyCode::Backspace);
    assert_eq!(rig.name(), "src/");
    rig.key(KeyCode::Backspace);
    assert_eq!(rig.name(), "");
    assert!(
        rig.box_text().contains("docs/"),
        "back at the project folder"
    );

    rig.type_text("ab");
    rig.key(KeyCode::Backspace);
    assert_eq!(rig.name(), "a", "inside a name it deletes a character");
}

#[test]
fn dot_dot_goes_up_from_the_project_folder() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    rig.type_text("../");
    let list = rig.box_text();

    assert!(
        !list.contains("docs/") && !list.contains("alpha.txt"),
        "not the project's own: {list}"
    );
    assert!(list.contains('/'), "{list}");
}

#[test]
fn a_folder_that_is_not_there_shows_why_in_the_list() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    rig.type_text("gone/");
    let screen = rig.box_text();

    assert!(screen.contains("not a folder that exists"), "{screen}");
}

#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_read_shows_why() {
    use std::os::unix::fs::PermissionsExt;
    let mut rig = rig();
    let locked = rig.dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        return; // running as root
    }
    rig.app.do_action(Action::OpenFile);

    rig.type_text("locked/");
    let screen = rig.box_text();

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(screen.contains("cannot read"), "{screen}");
}

#[test]
fn open_folder_lists_only_folders_and_enter_on_a_part_goes_in_before_it_opens() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);
    let screen = rig.box_text();
    assert!(
        screen.contains("docs/") && !screen.contains("alpha.txt"),
        "{screen}"
    );

    rig.type_text("sr");
    rig.key(KeyCode::Enter);
    assert_eq!(rig.name(), "src/");
    assert!(!rig.app.should_quit(), "not opened yet");

    rig.key(KeyCode::Enter);
    assert!(rig.app.should_quit(), "the folder shown is the one meant");
}

#[test]
fn open_folder_with_the_whole_name_typed_opens_it_at_once() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);

    rig.type_text("docs");
    rig.key(KeyCode::Enter);

    assert!(rig.app.should_quit());
}

#[test]
fn ctrl_enter_takes_what_is_typed_whatever_is_highlighted() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);
    rig.type_text("do");

    rig.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::CONTROL,
    )));

    assert!(!rig.app.should_quit(), "`do` is no folder");
    assert!(rig.told().contains("do"), "{}", rig.told());
}

#[test]
fn a_click_on_a_row_does_what_enter_does_on_it() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);
    rig.type_text("src/");
    let screen = rig.screen();
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("main.rs"))
        .unwrap();
    let column = line.trim_matches('"').find("main.rs").unwrap();

    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: column as u16 + 1,
        row: row as u16,
        modifiers: KeyModifiers::NONE,
    }));

    assert_eq!(rig.file().as_deref(), Some("main.rs"));
}

#[test]
fn the_list_is_read_on_a_thread_and_a_key_typed_before_it_arrives_is_not_lost() {
    let dir = project();
    let mut state = AppState::new(Editor::new(Buffer::default()));
    state.open_project(dir.path()).unwrap();
    let (tx, mut events) = mpsc::channel(64);
    let mut app = App::new(state).with_events(tx);
    app.do_action(Action::OpenFile);

    // Not drained yet: the list is empty, and enter uses what was typed.
    for c in "alpha.txt".chars() {
        app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )));
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        while let Ok(event) = events.try_recv() {
            app.handle_event(event);
        }
        let shown = {
            let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
            terminal
                .draw(|frame| prepare_and_render(frame, app.state_mut()))
                .unwrap();
            terminal.backend().to_string()
        };
        if shown.matches("alpha.txt").count() >= 2 {
            break; // the typed name and the list's row
        }
        assert!(Instant::now() < deadline, "the list never came: {shown}");
        std::thread::sleep(Duration::from_millis(5));
    }
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));

    assert_eq!(
        app.state()
            .editor()
            .buffer()
            .path()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .as_deref(),
        Some("alpha.txt")
    );
}

#[test]
fn an_answer_for_a_folder_the_name_has_left_is_dropped() {
    let dir = project();
    let mut state = AppState::new(Editor::new(Buffer::default()));
    state.open_project(dir.path()).unwrap();
    let (tx, mut events) = mpsc::channel(64);
    let mut app = App::new(state).with_events(tx);
    app.do_action(Action::OpenFile);
    for c in "src/".chars() {
        app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )));
    }

    // Wait for the answers of both reads: the project folder's (late, stale) and src's.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut taken = 0;
    while taken < 2 && Instant::now() < deadline {
        while let Ok(event) = events.try_recv() {
            app.handle_event(event);
            taken += 1;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    let screen = terminal.backend().to_string();

    assert!(screen.contains("main.rs"), "{screen}");
    assert!(
        !screen.contains("docs/"),
        "the project folder's list is not shown for src/: {screen}"
    );
}

#[test]
fn a_folder_of_thousands_of_files_keeps_typing_quick() {
    let mut rig = rig();
    let big = rig.dir.path().join("big");
    fs::create_dir(&big).unwrap();
    for n in 0..5_000 {
        fs::write(big.join(format!("file_{n:05}.txt")), "").unwrap();
    }
    rig.app.do_action(Action::OpenFile);
    rig.type_text("big/");

    let start = Instant::now();
    rig.type_text("file_0");
    let screen = rig.box_text();
    let took = start.elapsed();

    assert!(screen.contains("file_0"), "{screen}");
    assert!(screen.contains("of 5000"), "the rest is counted: {screen}");
    assert!(took < Duration::from_secs(2), "{took:?}");
}

// ---- the Move prompt has the list too, of folders, with no highlight until the arrows

impl Rig {
    /// Opens the Move prompt for `relative` (a path from the project folder).
    fn move_prompt_for(&mut self, relative: &str) {
        self.app
            .state_mut()
            .set_focus(crate::app::state::Focus::Explorer);
        let path = self.dir.path().join(relative);
        self.app.state_mut().explorer_mut().reveal(&path).unwrap();
        self.app.do_action(Action::MoveEntry);
    }
}

#[test]
fn the_move_prompt_lists_the_folders_of_the_folder_typed_so_far() {
    let mut rig = rig();
    rig.move_prompt_for("alpha.txt");

    let list = rig.box_text();
    assert!(list.contains("docs/") && list.contains("src/"), "{list}");
    assert!(!list.contains("alpine.txt"), "files are not listed: {list}");

    for _ in 0..rig.name().len() {
        rig.key(KeyCode::Backspace);
    }
    rig.type_text("src/");
    assert!(rig.box_text().contains("components/"));
}

#[test]
fn a_new_name_typed_in_the_move_prompt_is_not_taken_for_a_folder_that_matches_it() {
    let mut rig = rig();
    rig.move_prompt_for("alpha.txt");
    for _ in 0..rig.name().len() {
        rig.key(KeyCode::Backspace);
    }

    // `doc` matches the folder `docs`, but enter is the user's name, not a step into it.
    rig.type_text("doc");
    rig.key(KeyCode::Enter);

    assert!(
        rig.dir.path().join("doc").exists(),
        "alpha.txt was moved to the name doc"
    );
    assert!(!rig.dir.path().join("alpha.txt").exists());
}

#[test]
fn down_and_enter_in_the_move_prompt_go_into_the_folder() {
    let mut rig = rig();
    rig.move_prompt_for("alpha.txt");
    for _ in 0..rig.name().len() {
        rig.key(KeyCode::Backspace);
    }

    rig.key(KeyCode::Down);
    rig.key(KeyCode::Enter);

    assert_eq!(rig.name(), "docs/", "the first folder, gone into");
    rig.key(KeyCode::Enter);
    assert!(
        rig.dir.path().join("docs/alpha.txt").exists(),
        "enter with the folder typed moves into it"
    );
}

#[test]
fn tab_in_the_move_prompt_completes_a_folder_name() {
    let mut rig = rig();
    rig.move_prompt_for("alpha.txt");
    for _ in 0..rig.name().len() {
        rig.key(KeyCode::Backspace);
    }

    rig.type_text("sr");
    rig.key(KeyCode::Tab);

    assert_eq!(rig.name(), "src");
}

// ---- the list scrolls, and the folder can be browsed as a tree

impl Rig {
    fn ctrl(&mut self, c: char) {
        self.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::CONTROL,
        )));
    }

    fn wheel(&mut self, kind: MouseEventKind, column: u16, row: u16) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }

    /// Where `text` is on the screen: column and row.
    fn find_on_screen(&mut self, text: &str) -> (u16, u16) {
        let screen = self.screen();
        let (row, line) = screen
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains(text))
            .unwrap_or_else(|| panic!("no {text}: {screen}"));
        (
            line.trim_matches('"').find(text).unwrap() as u16,
            row as u16,
        )
    }

    fn many_folders(&self) {
        for n in 0..30 {
            fs::create_dir(self.dir.path().join(format!("d{n:02}"))).unwrap();
        }
    }
}

#[test]
fn the_highlight_scrolls_the_list_past_its_last_row() {
    let mut rig = rig();
    rig.many_folders();
    rig.app.do_action(Action::OpenFolder);
    let first = rig.box_text();
    assert!(first.contains("d00/") && !first.contains("d12/"), "{first}");

    for _ in 0..13 {
        rig.key(KeyCode::Down);
    }
    let scrolled = rig.box_text();

    assert!(
        scrolled.contains("d12/"),
        "the highlight is on screen: {scrolled}"
    );
    assert!(
        !scrolled.contains("d00/"),
        "the first rows went off the top: {scrolled}"
    );
    assert!(
        scrolled.contains("of 32"),
        "the place in the list is said: {scrolled}"
    );
    rig.key(KeyCode::Enter);
    assert_eq!(
        rig.name(),
        "d12/",
        "enter takes the row that is highlighted"
    );
}

#[test]
fn page_down_and_the_wheel_scroll_the_list() {
    let mut rig = rig();
    rig.many_folders();
    rig.app.do_action(Action::OpenFolder);

    rig.key(KeyCode::PageDown);
    assert!(rig.box_text().contains("d10/"));

    let (column, row) = rig.find_on_screen("d10/");
    for _ in 0..3 {
        rig.wheel(MouseEventKind::ScrollDown, column, row);
    }
    let scrolled = rig.box_text();
    assert!(
        scrolled.contains("d19/") && !scrolled.contains("d09/"),
        "{scrolled}"
    );
    assert_eq!(rig.name(), "", "the wheel does not touch the name");

    // A click on a row after the wheel is a click on the row that is there.
    let (column, row) = rig.find_on_screen("d15/");
    rig.wheel(MouseEventKind::Down(MouseButton::Left), column + 1, row);
    assert_eq!(rig.name(), "d15/");
}

#[test]
fn ctrl_b_shows_the_folders_as_a_tree_and_enter_opens_the_one_selected() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);

    rig.ctrl('b');
    let tree = rig.box_text();
    assert!(
        tree.contains("docs") && tree.contains("src") && !tree.contains("alpha.txt"),
        "folders only: {tree}"
    );
    assert!(tree.contains("ctrl+b: type"), "{tree}");
    assert!(
        rig.name().ends_with(std::path::is_separator),
        "the root: {}",
        rig.name()
    );

    rig.key(KeyCode::Down);
    rig.key(KeyCode::Down);
    rig.key(KeyCode::Right);
    let tree = rig.box_text();
    assert!(tree.contains("components"), "src opened: {tree}");
    assert!(
        rig.name().ends_with(std::path::is_separator) && Path::new(&rig.name()).ends_with("src"),
        "{}",
        rig.name()
    );

    rig.key(KeyCode::Enter);

    assert!(rig.app.should_quit());
    assert!(
        rig.app
            .restart_target()
            .is_some_and(|dir| dir.ends_with("src"))
    );
}

#[test]
fn left_on_the_root_of_the_tree_goes_up_and_ctrl_b_goes_back_to_typing() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);
    rig.ctrl('b');
    let root = rig.name();

    rig.key(KeyCode::Left);

    let up = rig.name();
    assert!(
        root.starts_with(&up) && up.len() < root.len(),
        "{root} -> {up}"
    );
    let parent = Path::new(up.trim_end_matches(std::path::is_separator))
        .file_name()
        .unwrap();
    let tree = rig.box_text();
    assert!(
        tree.contains(parent.to_str().unwrap()),
        "the folder above is the root: {tree}"
    );

    rig.ctrl('b');
    let list = rig.box_text();
    assert!(list.contains("browse"), "the list is back: {list}");
    rig.type_text("x");
    assert_eq!(
        rig.name(),
        format!("{up}x"),
        "typing goes on from the tree's path"
    );
}

#[test]
fn the_tree_hides_dot_folders_until_a_dot_is_pressed() {
    let mut rig = rig();
    fs::create_dir(rig.dir.path().join(".dots")).unwrap();
    rig.app.do_action(Action::OpenFolder);
    rig.ctrl('b');
    assert!(!rig.box_text().contains(".dots"));

    rig.key(KeyCode::Char('.'));

    assert!(rig.box_text().contains(".dots"));
}

#[test]
fn open_file_browses_files_too_and_enter_on_a_file_opens_it() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);
    rig.ctrl('b');

    // From the root: docs, src, then the files.
    for _ in 0..3 {
        rig.key(KeyCode::Down);
    }
    rig.key(KeyCode::Enter);

    assert_eq!(rig.file().as_deref(), Some("alpha.txt"));
}

#[test]
fn escape_in_the_tree_closes_the_prompt() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);
    rig.ctrl('b');

    rig.key(KeyCode::Esc);

    assert!(!matches!(rig.app.state().popup(), Popup::NewEntry(_)));
    assert!(!rig.app.should_quit());
}

// ---- the system's own file dialog

impl Rig {
    /// What the thread of the dialog does when it is closed, after the prompt was waiting.
    fn dialog_closed(&mut self, path: Option<std::path::PathBuf>) {
        self.dialog_ended(path, false);
    }

    fn dialog_ended(&mut self, path: Option<std::path::PathBuf>, failed: bool) {
        let prompt = self.app.state_mut().name_prompt_mut().unwrap();
        assert!(prompt.begin_dialog(), "no dialog was open yet");
        self.app
            .handle_event(crate::event::Event::SystemPick { path, failed });
    }
}

#[test]
fn a_folder_chosen_in_the_system_dialog_is_opened() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);
    let docs = rig.dir.path().join("docs");

    rig.dialog_closed(Some(docs));

    assert!(rig.app.should_quit());
    assert!(
        rig.app
            .restart_target()
            .is_some_and(|dir| dir.ends_with("docs"))
    );
}

#[test]
fn a_file_chosen_in_the_system_dialog_is_opened_as_a_tab() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    rig.dialog_closed(Some(rig.dir.path().join("alpha.txt")));

    assert_eq!(rig.file().as_deref(), Some("alpha.txt"));
}

#[test]
fn a_cancelled_system_dialog_leaves_the_prompt_as_it_was() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);
    rig.type_text("do");

    rig.dialog_closed(None);

    assert_eq!(rig.name(), "do");
    assert!(!rig.app.should_quit());
}

#[test]
fn an_answer_of_a_dialog_the_prompt_was_not_waiting_for_is_dropped() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);

    rig.app.handle_event(crate::event::Event::SystemPick {
        path: Some(rig.dir.path().join("docs")),
        failed: false,
    });

    assert!(!rig.app.should_quit());
    assert_eq!(rig.name(), "");
}

#[test]
fn ctrl_e_says_so_when_the_editor_has_no_event_loop_to_hear_the_dialog() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);

    rig.ctrl('e');

    assert!(rig.told().contains("dialog"), "{}", rig.told());
}

#[test]
fn a_system_dialog_that_never_opened_says_so_and_leaves_the_prompt() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);
    rig.type_text("do");

    rig.dialog_ended(None, true);

    assert!(rig.told().contains("did not open"), "{}", rig.told());
    assert_eq!(rig.name(), "do");
    assert!(!rig.app.should_quit());
}
