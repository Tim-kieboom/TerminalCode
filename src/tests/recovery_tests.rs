//! What is unsaved when the terminal goes away: kept in recovery files at the hangup,
//! offered back at the next start in the same project, put back without overwriting
//! a file that changed.

use std::fs;
use std::path::PathBuf;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::state::Popup;
use crate::event::Event;
use crate::files::paths::canonical;
use crate::persist::recovery::{self, FOLDER};

/// A project folder with a file and a config folder, as one run of the editor sees them.
struct Setup {
    config: tempfile::TempDir,
    project: tempfile::TempDir,
}

impl Setup {
    fn new() -> Self {
        let setup = Self {
            config: tempfile::tempdir().unwrap(),
            project: tempfile::tempdir().unwrap(),
        };
        fs::write(setup.file("a.txt"), "one\n").unwrap();
        setup
    }

    fn file(&self, name: &str) -> PathBuf {
        self.project.path().join(name)
    }

    /// A run of the editor in this project with `a.txt` open.
    fn run(&self) -> App {
        let mut app = App::default().with_config_dir(Some(self.config.path().to_path_buf()));
        app.state_mut().open_project(self.project.path()).unwrap();
        app.state_mut()
            .workspace_mut()
            .open_path(&self.file("a.txt"))
            .unwrap();
        app
    }

    fn kept(&self) -> usize {
        let project = canonical(self.project.path());
        recovery::find(self.config.path(), &project).found.len()
    }
}

fn type_text(app: &mut App, text: &str) {
    app.state_mut()
        .workspace_mut()
        .with_editor(|editor| editor.insert_text(text))
        .unwrap();
}

fn active_text(app: &App) -> String {
    app.state().workspace().active_editor().buffer().text()
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn asking(app: &App) -> bool {
    matches!(app.state().popup(), Popup::Confirm(_))
}

#[test]
fn a_hangup_keeps_the_unsaved_text_and_ends_the_loop() {
    let setup = Setup::new();
    let mut app = setup.run();
    type_text(&mut app, "typed ");

    app.handle_event(Event::Hangup);

    assert!(app.should_quit());
    assert_eq!(setup.kept(), 1);
}

#[test]
fn a_hangup_with_nothing_unsaved_writes_nothing() {
    let setup = Setup::new();
    let mut app = setup.run();

    app.handle_event(Event::Hangup);

    assert!(app.should_quit());
    assert!(!setup.config.path().join(FOLDER).exists());
}

#[test]
fn a_hangup_without_a_config_folder_still_ends_the_loop() {
    let mut app = App::default();
    type_text(&mut app, "typed");

    app.handle_event(Event::Hangup);

    assert!(app.should_quit());
}

#[test]
fn the_next_start_asks_and_yes_brings_the_text_back_as_an_unsaved_edit() {
    let setup = Setup::new();
    let mut before = setup.run();
    type_text(&mut before, "typed ");
    before.handle_event(Event::Hangup);

    let mut after = setup.run();
    after.offer_recovery();
    assert!(asking(&after));
    press(&mut after, KeyCode::Char('y'));

    assert!(!asking(&after));
    assert_eq!(active_text(&after), "typed one\n");
    assert!(
        after
            .state()
            .workspace()
            .active_editor()
            .buffer()
            .is_dirty()
    );
    assert_eq!(
        after.state().workspace().document_count(),
        2,
        "the file was open, plus the untitled start tab"
    );
    assert_eq!(fs::read_to_string(setup.file("a.txt")).unwrap(), "one\n");
    assert_eq!(setup.kept(), 0);
}

#[test]
fn the_cursor_is_where_it_was() {
    let setup = Setup::new();
    let mut before = setup.run();
    type_text(&mut before, "typed ");
    let at = before
        .state()
        .workspace()
        .active_editor()
        .selections()
        .primary()
        .head();
    before.handle_event(Event::Hangup);

    let mut after = setup.run();
    after.offer_recovery();
    press(&mut after, KeyCode::Char('y'));

    let head = after
        .state()
        .workspace()
        .active_editor()
        .selections()
        .primary()
        .head();
    assert_eq!(head, at);
}

#[test]
fn a_file_that_changed_on_disk_is_not_overwritten_the_text_opens_as_a_copy() {
    let setup = Setup::new();
    let mut before = setup.run();
    type_text(&mut before, "typed ");
    before.handle_event(Event::Hangup);
    fs::write(setup.file("a.txt"), "changed elsewhere\n").unwrap();

    let mut after = setup.run();
    after.offer_recovery();
    press(&mut after, KeyCode::Char('y'));

    assert_eq!(active_text(&after), "typed one\n");
    let (names, _) = after.state().workspace().tab_names();
    assert!(
        names.iter().any(|name| name.contains("recovered: a.txt")),
        "{names:?}"
    );
    assert_eq!(
        fs::read_to_string(setup.file("a.txt")).unwrap(),
        "changed elsewhere\n"
    );
}

#[test]
fn a_text_that_never_had_a_file_comes_back_in_a_tab_named_recovered() {
    let setup = Setup::new();
    let mut before = setup.run();
    before.state_mut().workspace_mut().new_file();
    type_text(&mut before, "scratch");
    before.handle_event(Event::Hangup);

    let mut after = setup.run();
    after.offer_recovery();
    press(&mut after, KeyCode::Char('y'));

    assert_eq!(active_text(&after), "scratch");
    let (names, _) = after.state().workspace().tab_names();
    assert!(
        names
            .iter()
            .any(|name| name.contains("recovered: [no name]")),
        "{names:?}"
    );
}

#[test]
fn n_throws_the_kept_text_away_and_esc_keeps_it_for_next_time() {
    let setup = Setup::new();
    let mut before = setup.run();
    type_text(&mut before, "typed ");
    before.handle_event(Event::Hangup);

    let mut later = setup.run();
    later.offer_recovery();
    press(&mut later, KeyCode::Esc);
    assert!(!asking(&later));
    assert_eq!(active_text(&later), "one\n");
    assert_eq!(setup.kept(), 1, "asked again next start");

    let mut last = setup.run();
    last.offer_recovery();
    press(&mut last, KeyCode::Char('n'));
    assert!(!asking(&last));
    assert_eq!(active_text(&last), "one\n");
    assert_eq!(setup.kept(), 0);
}

#[test]
fn another_project_is_not_asked_about_these_files() {
    let setup = Setup::new();
    let mut before = setup.run();
    type_text(&mut before, "typed ");
    before.handle_event(Event::Hangup);

    let other = tempfile::tempdir().unwrap();
    let mut app = App::default().with_config_dir(Some(setup.config.path().to_path_buf()));
    app.state_mut().open_project(other.path()).unwrap();
    app.offer_recovery();

    assert!(!asking(&app));
    assert_eq!(setup.kept(), 1);
}

#[test]
fn a_recovery_file_that_cannot_be_read_is_reported_and_left_alone() {
    let setup = Setup::new();
    let folder = setup.config.path().join(FOLDER);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("broken.json"), "{").unwrap();

    let mut app = setup.run();
    app.offer_recovery();

    assert!(!asking(&app));
    assert!(
        app.state()
            .notifications()
            .texts()
            .join("|")
            .contains("recovery file")
    );
    assert!(folder.join("broken.json").exists());
}

#[tokio::test]
async fn the_terminal_closing_ends_the_loop_after_keeping_the_text() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let setup = Setup::new();
    let mut app = setup.run();
    type_text(&mut app, "typed ");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (input_tx, input) = tokio::sync::mpsc::channel(8);
    let (_events_tx, events) = tokio::sync::mpsc::channel(8);
    let sources = crate::app::Sources { input, events };
    drop(input_tx);

    let result = app.run(&mut terminal, sources).await;

    assert!(result.is_ok());
    assert_eq!(setup.kept(), 1);
}

#[test]
fn the_file_is_kept_by_its_whole_path_whatever_way_it_was_opened() {
    let setup = Setup::new();
    fs::create_dir(setup.file("sub")).unwrap();
    let mut app = setup.run();
    let roundabout = setup.file("sub").join("..").join("a.txt");
    app.state_mut()
        .workspace_mut()
        .open_path(&roundabout)
        .unwrap();
    type_text(&mut app, "typed ");

    app.handle_event(Event::Hangup);

    let project = canonical(setup.project.path());
    let found = recovery::find(setup.config.path(), &project).found;
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].recovery.file,
        Some(canonical(&setup.file("a.txt")))
    );
}
