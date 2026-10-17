use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::AppState;
use crate::buffer::Position;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::files::watcher::FsWatcher;
use crate::initial_state;

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn ctrl(app: &mut App, c: char) {
    app.handle_input(key(KeyCode::Char(c), KeyModifiers::CONTROL));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_input(key(KeyCode::Char(c), KeyModifiers::NONE));
    }
}

fn changed(app: &mut App, path: &Path) {
    app.handle_event(Event::FilesChanged(vec![path.to_path_buf()]));
}

/// An app with `a.txt` (containing `text`) open, and the file's path.
fn app_with_file(text: &str) -> (App, tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, text).unwrap();
    let app = App::new(initial_state(std::slice::from_ref(&path)).unwrap());
    (app, dir, path)
}

fn text(app: &App) -> String {
    app.state().editor().buffer().text()
}

fn notifications(app: &App) -> usize {
    app.state().notifications().len()
}

fn latest(app: &App) -> String {
    app.state()
        .latest_notification()
        .unwrap_or_default()
        .to_owned()
}

fn app_with_project(dir: &Path) -> App {
    let mut state = AppState::default();
    state.open_project(dir).unwrap();
    App::new(state)
}

fn app_with_watcher(app: App) -> (App, mpsc::Receiver<Event>) {
    let (tx, rx) = mpsc::channel(64);
    let watcher = FsWatcher::new(tx).unwrap();
    (app.with_watcher(watcher), rx)
}

fn canon(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap()
}

fn next_tab(app: &mut App) {
    app.handle_input(key(KeyCode::Tab, KeyModifiers::CONTROL));
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

/// `a.txt` open first, then a second untitled tab in front.
fn app_with_a_tab_behind(text: &str) -> (App, tempfile::TempDir, PathBuf) {
    let (mut app, dir, path) = app_with_file(text);
    ctrl(&mut app, 'n');
    (app, dir, path)
}

fn set_read_only(path: &Path, read_only: bool) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(read_only);
    fs::set_permissions(path, permissions).unwrap();
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(key(code, KeyModifiers::NONE));
}

fn app_with_read_only_file(text: &str) -> (App, tempfile::TempDir, PathBuf) {
    let (app, dir, path) = app_with_file(text);
    drop(app);
    set_read_only(&path, true);
    let app = App::new(initial_state(std::slice::from_ref(&path)).unwrap());
    (app, dir, path)
}

mod basics;
mod deleted_while_open;
mod nothing_reported;
mod read_only_file;
mod read_only_undo;
