//! Open Recent: what is noted, what the menu lists, and what its items do.

use std::fs;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::AppState;
use crate::components::popups::menu::Entry;
use crate::components::prepare_and_render;
use crate::event::action::Action;
use crate::paths::canonical;
use crate::recent;

struct Rig {
    app: App,
    project: tempfile::TempDir,
    config: tempfile::TempDir,
}

fn rig() -> Rig {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("a.txt"), "alpha\n").unwrap();
    fs::write(project.path().join("b.txt"), "beta\n").unwrap();
    fs::create_dir(project.path().join("sub")).unwrap();
    let config = tempfile::tempdir().unwrap();
    let mut state = AppState::default();
    state.open_project(project.path()).unwrap();
    let app = App::new(state).with_config_dir(Some(config.path().to_path_buf()));
    Rig {
        app,
        project,
        config,
    }
}

impl Rig {
    /// Gives the app a look at what is open, as the loop does after every input.
    fn look(&mut self) {
        self.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::F(24),
            KeyModifiers::NONE,
        )));
    }

    fn open(&mut self, name: &str) {
        let path = self.project.path().join(name);
        self.app
            .state_mut()
            .workspace_mut()
            .open_path(&path)
            .unwrap();
        self.look();
    }

    fn saved(&self) -> recent::Recent {
        recent::load(Some(self.config.path())).0
    }

    fn open_menu(&mut self) {
        self.app.do_action(Action::OpenMenu);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|frame| prepare_and_render(frame, self.app.state_mut()))
            .unwrap();
    }

    /// The labels of the Open Recent menu.
    fn recent_labels(&mut self) -> Vec<String> {
        self.open_menu();
        let menu = self.app.state().menu().unwrap();
        let Entry::Group { entries, .. } = file_group(menu) else {
            unreachable!()
        };
        entries.iter().map(label).collect()
    }
}

fn label(entry: &Entry) -> String {
    match entry {
        Entry::Item { label, .. } | Entry::Group { label, .. } => label.to_string(),
        Entry::Separator => "---".to_owned(),
    }
}

fn file_group(menu: &crate::components::popups::menu::Menu) -> &Entry {
    let Entry::Group { entries, .. } = &menu.entries()[0] else {
        panic!("the first entry is the File group")
    };
    entries
        .iter()
        .find(|entry| matches!(entry, Entry::Group { label, .. } if label == "Open Recent"))
        .unwrap()
}

fn recent_action(app: &App, wanted: &str) -> Action {
    let menu = app.state().menu().unwrap();
    let Entry::Group { entries, .. } = file_group(menu) else {
        unreachable!()
    };
    entries
        .iter()
        .find_map(|entry| match entry {
            Entry::Item { label, action, .. } if label.ends_with(wanted) => Some(action.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no recent entry ends with {wanted}"))
}

#[test]
fn the_project_folder_and_the_files_opened_are_noted_and_written() {
    let mut rig = rig();
    rig.look();
    rig.open("a.txt");
    rig.open("b.txt");

    let list = rig.app.recent_list();

    assert_eq!(list.folders(), [canonical(rig.project.path())]);
    assert_eq!(
        list.files()[0],
        canonical(&rig.project.path().join("b.txt"))
    );
    assert_eq!(
        list.files()[1],
        canonical(&rig.project.path().join("a.txt"))
    );
    assert_eq!(&rig.saved(), list, "and the file has the same");
}

#[test]
fn a_file_closed_and_opened_again_is_not_listed_twice() {
    let mut rig = rig();
    rig.open("a.txt");
    rig.open("b.txt");

    rig.open("a.txt");

    let files = rig.app.recent_list().files();
    assert_eq!(files.len(), 2);
}

#[test]
fn the_menu_lists_folders_then_files_then_clear_recent() {
    let mut rig = rig();
    rig.open("a.txt");

    let labels = rig.recent_labels();

    assert_eq!(labels.len(), 5, "{labels:?}");
    assert!(labels[0].ends_with(rig.project.path().file_name().unwrap().to_str().unwrap()));
    assert_eq!(labels[1], "---");
    assert!(labels[2].ends_with("a.txt"));
    assert_eq!(labels[3], "---");
    assert_eq!(labels[4], "Clear Recent");
}

#[test]
fn an_empty_list_says_so() {
    let mut rig = rig();
    rig.app.do_action(Action::ClearRecent);

    assert_eq!(rig.recent_labels(), ["Nothing opened yet"]);
}

#[test]
fn a_file_that_is_gone_is_dropped_when_the_menu_opens() {
    let mut rig = rig();
    rig.open("a.txt");
    rig.open("b.txt");
    fs::remove_file(rig.project.path().join("a.txt")).unwrap();

    let labels = rig.recent_labels();

    assert!(
        !labels.iter().any(|label| label.ends_with("a.txt")),
        "{labels:?}"
    );
    assert!(labels.iter().any(|label| label.ends_with("b.txt")));
    assert_eq!(
        rig.saved().files().len(),
        1,
        "and it is gone from the file too"
    );
}

#[test]
fn a_recent_file_opens_as_a_tab() {
    let mut rig = rig();
    rig.open("a.txt");
    rig.open("b.txt");
    rig.open_menu();
    let action = recent_action(&rig.app, "a.txt");

    rig.app.do_action(action);

    assert_eq!(rig.app.state().editor().buffer().text(), "alpha\n");
}

#[test]
fn a_recent_folder_starts_the_editor_again_there() {
    let mut rig = rig();
    rig.look();
    rig.open_menu();
    let action = recent_action(&rig.app, "");
    assert!(matches!(action, Action::OpenRecentFolder(_)));

    rig.app.do_action(action);

    assert!(rig.app.should_quit());
    assert_eq!(
        rig.app.restart_target(),
        Some(canonical(rig.project.path()).as_path())
    );
}

#[test]
fn clear_recent_forgets_everything_and_the_file_with_it() {
    let mut rig = rig();
    rig.open("a.txt");
    assert!(!rig.saved().is_empty());

    rig.app.do_action(Action::ClearRecent);

    assert!(rig.app.recent_list().is_empty());
    assert!(rig.saved().is_empty());
}

#[test]
fn the_list_comes_back_from_the_file_at_the_next_start() {
    let mut rig = rig();
    rig.open("a.txt");
    let saved = rig.saved();

    let second = App::new(AppState::default()).with_recent(saved.clone());

    assert_eq!(second.recent_list(), &saved);
}

#[test]
fn without_a_config_folder_nothing_is_written_and_nothing_fails() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("a.txt"), "").unwrap();
    let mut state = AppState::default();
    state.open_project(project.path()).unwrap();
    let mut app = App::new(state);

    app.state_mut()
        .workspace_mut()
        .open_path(&project.path().join("a.txt"))
        .unwrap();
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::F(24),
        KeyModifiers::NONE,
    )));

    assert_eq!(app.recent_list().files().len(), 1);
    assert!(app.state().notifications().texts().is_empty());
}
