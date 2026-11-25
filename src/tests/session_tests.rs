//! The tabs and panes of a project folder are kept when the editor ends and come back
//! when it starts there again.

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::state::{AppState, Popup};
use crate::buffer::Position;
use crate::event::Event;
use crate::files::paths::canonical;
use crate::persist::session::{self, Session};
use crate::settings::RestoreSession;
use crate::startup;
use crate::ui::layout::Axis;

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
        for name in ["a.txt", "b.txt", "c.txt"] {
            fs::write(setup.file(name), "one\ntwo\nthree\n").unwrap();
        }
        setup
    }

    fn file(&self, name: &str) -> PathBuf {
        self.project.path().join(name)
    }

    fn project(&self) -> PathBuf {
        canonical(self.project.path())
    }

    /// A run of the editor in this project, with nothing open.
    fn app(&self) -> App {
        let mut app = App::default().with_config_dir(Some(self.config.path().to_path_buf()));
        app.state_mut().open_project(self.project.path()).unwrap();
        app
    }

    /// The start of a run: a state in this project and what `startup` does with the session.
    fn start(&self, mode: RestoreSession) -> (AppState, Option<Session>) {
        let mut state = AppState::default();
        state.open_project(self.project.path()).unwrap();
        let pending = startup::restore_session(&mut state, Some(self.config.path()), mode);
        (state, pending)
    }

    fn kept(&self) -> Option<Session> {
        session::load(self.config.path(), &self.project()).unwrap()
    }
}

fn open(app: &mut App, path: &Path) {
    app.state_mut().workspace_mut().open_path(path).unwrap();
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn files_of(state: &AppState, project: &Path) -> Vec<String> {
    let session = state.workspace().session(project).unwrap();
    session
        .files()
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn the_tabs_come_back_in_order_with_the_one_in_front_and_pinned_ones() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    open(&mut app, &setup.file("b.txt"));
    app.state_mut().workspace_mut().toggle_pin_active();
    open(&mut app, &setup.file("c.txt"));
    app.save_session();

    let (state, pending) = setup.start(RestoreSession::Always);

    assert!(pending.is_none());
    assert_eq!(
        files_of(&state, &setup.project()),
        ["b.txt", "a.txt", "c.txt"]
    );
    assert_eq!(
        state
            .workspace()
            .active_editor()
            .buffer()
            .path()
            .unwrap()
            .file_name()
            .unwrap(),
        "c.txt"
    );
    assert_eq!(state.workspace().session(&setup.project()), setup.kept());
}

#[test]
fn the_cursor_comes_back_where_it_was() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    app.state_mut()
        .workspace_mut()
        .with_editor(|editor| editor.go_to(Position::new(2, 3)));
    app.save_session();

    let (state, _) = setup.start(RestoreSession::Always);

    let head = state
        .workspace()
        .active_editor()
        .selections()
        .primary()
        .head();
    assert_eq!(head, Position::new(2, 3));
}

#[test]
fn split_panes_come_back_with_their_tabs_the_focus_and_the_ratio() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    app.state_mut().workspace_mut().split(Axis::Horizontal);
    open(&mut app, &setup.file("b.txt"));
    app.state_mut().workspace_mut().split(Axis::Vertical);
    open(&mut app, &setup.file("c.txt"));
    app.save_session();

    let (state, _) = setup.start(RestoreSession::Always);

    assert_eq!(state.workspace().pane_count(), 3);
    assert_eq!(state.workspace().session(&setup.project()), setup.kept());
    assert_eq!(
        state
            .workspace()
            .active_editor()
            .buffer()
            .path()
            .unwrap()
            .file_name()
            .unwrap(),
        "c.txt",
        "the pane that had the keyboard has it"
    );
}

#[test]
fn a_file_that_is_gone_is_skipped_with_one_note() {
    let setup = Setup::new();
    let mut app = setup.app();
    for name in ["a.txt", "b.txt", "c.txt"] {
        open(&mut app, &setup.file(name));
    }
    app.save_session();
    fs::remove_file(setup.file("a.txt")).unwrap();
    fs::remove_file(setup.file("b.txt")).unwrap();

    let (state, _) = setup.start(RestoreSession::Always);

    assert_eq!(files_of(&state, &setup.project()), ["c.txt"]);
    let notes = state.notifications().texts();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("a.txt") && notes[0].contains("b.txt"));
}

#[test]
fn when_every_file_is_gone_the_start_is_as_without_a_session() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    app.save_session();
    fs::remove_file(setup.file("a.txt")).unwrap();

    let (state, _) = setup.start(RestoreSession::Always);

    assert_eq!(state.workspace().document_count(), 1);
    assert!(state.workspace().active_editor().buffer().path().is_none());
}

#[test]
fn closing_every_tab_forgets_the_session() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    app.save_session();
    assert!(setup.kept().is_some());

    app.state_mut().workspace_mut().close_tab();
    app.save_session();

    assert!(setup.kept().is_none());
}

#[test]
fn tabs_without_a_file_are_not_kept() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    app.state_mut().workspace_mut().new_file();
    app.save_session();

    assert_eq!(setup.kept().unwrap().files().len(), 1);
}

#[test]
fn never_keeps_nothing_and_restores_nothing() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    app.save_session();
    assert!(setup.kept().is_some());

    app.settings_mut().restore_session = RestoreSession::Never;
    app.state_mut().workspace_mut().close_tab();
    app.save_session();
    let (state, pending) = setup.start(RestoreSession::Never);

    assert!(setup.kept().is_some(), "the old session is left as it was");
    assert!(pending.is_none());
    assert!(state.workspace().active_editor().buffer().path().is_none());
}

#[test]
fn ask_holds_the_session_until_the_question_is_answered() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    open(&mut app, &setup.file("b.txt"));
    app.save_session();

    let (state, pending) = setup.start(RestoreSession::Ask);
    assert!(pending.is_some());
    assert!(state.workspace().active_editor().buffer().path().is_none());
    let mut next = App::new(state)
        .with_config_dir(Some(setup.config.path().to_path_buf()))
        .with_pending_session(pending);
    next.offer_session();
    assert!(matches!(next.state().popup(), Popup::Confirm(_)));
    press(&mut next, KeyCode::Char('y'));

    assert!(!matches!(next.state().popup(), Popup::Confirm(_)));
    assert_eq!(files_of(next.state(), &setup.project()), ["a.txt", "b.txt"]);
}

#[test]
fn no_to_the_question_starts_empty_and_keeps_the_old_session_until_the_run_ends() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    app.save_session();

    let (state, pending) = setup.start(RestoreSession::Ask);
    let mut next = App::new(state)
        .with_config_dir(Some(setup.config.path().to_path_buf()))
        .with_pending_session(pending);
    next.offer_session();
    press(&mut next, KeyCode::Char('n'));

    assert!(
        next.state()
            .workspace()
            .active_editor()
            .buffer()
            .path()
            .is_none()
    );
    assert!(setup.kept().is_some());
}

#[test]
fn quitting_while_the_question_is_open_keeps_the_session_it_asked_about() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    app.save_session();

    let (state, pending) = setup.start(RestoreSession::Ask);
    let mut next = App::new(state)
        .with_config_dir(Some(setup.config.path().to_path_buf()))
        .with_pending_session(pending);
    next.offer_session();
    next.save_session();

    assert!(setup.kept().is_some());
}

#[test]
fn a_session_does_not_take_over_tabs_that_are_already_open() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    open(&mut app, &setup.file("b.txt"));
    app.save_session();

    let mut state = AppState::default();
    state.open_project(setup.project.path()).unwrap();
    state
        .workspace_mut()
        .open_path(&setup.file("c.txt"))
        .unwrap();
    let session = setup.kept().unwrap();
    let skipped = state.workspace_mut().restore_session(&session);

    assert!(skipped.is_empty());
    assert_eq!(files_of(&state, &setup.project()), ["c.txt"]);
}

#[test]
fn a_hangup_keeps_the_session_too() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));

    app.handle_event(Event::Hangup);

    assert_eq!(setup.kept().unwrap().files().len(), 1);
}

#[test]
fn another_project_has_its_own_session() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    app.save_session();
    let other = tempfile::tempdir().unwrap();
    let mut state = AppState::default();
    state.open_project(other.path()).unwrap();

    let pending = startup::restore_session(
        &mut state,
        Some(setup.config.path()),
        RestoreSession::Always,
    );

    assert!(pending.is_none());
    assert!(state.workspace().active_editor().buffer().path().is_none());
}

#[test]
fn a_session_file_that_cannot_be_read_is_reported_and_the_start_is_plain() {
    let setup = Setup::new();
    let mut app = setup.app();
    open(&mut app, &setup.file("a.txt"));
    app.save_session();
    let folder = setup.config.path().join(session::FOLDER);
    let file = fs::read_dir(&folder)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&file, "{").unwrap();

    let (state, _) = setup.start(RestoreSession::Always);

    assert!(state.workspace().active_editor().buffer().path().is_none());
    assert!(state.notifications().texts().join("|").contains("session"));
    assert!(file.exists());
}
