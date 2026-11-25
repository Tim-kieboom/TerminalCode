use std::fs;
use std::path::Path;

use crate::persist::session::{self, Direction, FOLDER, Layout, PaneSession, Session, TabSession};

fn tab(path: &str, line: usize) -> TabSession {
    TabSession {
        path: path.into(),
        pinned: false,
        line,
        column: 2,
        top: 1,
        left: 0,
    }
}

fn pane(tabs: Vec<TabSession>) -> Layout {
    Layout::Pane(PaneSession {
        tabs,
        active: 0,
        focused: false,
    })
}

fn two_panes() -> Layout {
    Layout::Split {
        direction: Direction::Horizontal,
        ratio: 400,
        first: Box::new(pane(vec![tab("/p/a.rs", 3), tab("/p/b.rs", 0)])),
        second: Box::new(Layout::Pane(PaneSession {
            tabs: vec![TabSession {
                pinned: true,
                ..tab("/p/c.rs", 9)
            }],
            active: 0,
            focused: true,
        })),
    }
}

#[test]
fn a_session_is_loaded_as_it_was_saved() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/p");
    let saved = Session::new(project, two_panes());

    session::save(dir.path(), project, Some(&saved)).unwrap();

    assert_eq!(session::load(dir.path(), project), Ok(Some(saved)));
}

#[test]
fn a_project_without_a_session_loads_nothing() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(session::load(dir.path(), Path::new("/p")), Ok(None));
}

#[test]
fn each_project_has_its_own_session() {
    let dir = tempfile::tempdir().unwrap();
    let (one, two) = (Path::new("/one"), Path::new("/two"));
    session::save(
        dir.path(),
        one,
        Some(&Session::new(one, pane(vec![tab("/one/a", 0)]))),
    )
    .unwrap();
    session::save(
        dir.path(),
        two,
        Some(&Session::new(two, pane(vec![tab("/two/b", 0)]))),
    )
    .unwrap();

    let loaded = session::load(dir.path(), two).unwrap().unwrap();

    assert_eq!(loaded.files(), [Path::new("/two/b")]);
}

#[test]
fn saving_nothing_forgets_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/p");
    let saved = Session::new(project, pane(vec![tab("/p/a.rs", 0)]));
    session::save(dir.path(), project, Some(&saved)).unwrap();

    session::save(dir.path(), project, None).unwrap();
    session::save(dir.path(), project, None).unwrap();

    assert_eq!(session::load(dir.path(), project), Ok(None));
}

#[test]
fn the_files_are_listed_in_reading_order() {
    let saved = Session::new(Path::new("/p"), two_panes());

    assert_eq!(
        saved.files(),
        [
            Path::new("/p/a.rs"),
            Path::new("/p/b.rs"),
            Path::new("/p/c.rs")
        ]
    );
}

#[test]
fn a_file_that_cannot_be_read_is_a_problem_and_stays() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/p");
    session::save(
        dir.path(),
        project,
        Some(&Session::new(project, pane(vec![tab("/p/a", 0)]))),
    )
    .unwrap();
    let file = fs::read_dir(dir.path().join(FOLDER))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&file, "{ nope").unwrap();

    let problem = session::load(dir.path(), project).unwrap_err();

    assert!(problem.contains(&*file.to_string_lossy()));
    assert!(file.exists());
}

#[test]
fn a_session_of_a_newer_layout_is_a_problem() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/p");
    session::save(
        dir.path(),
        project,
        Some(&Session::new(project, pane(vec![tab("/p/a", 0)]))),
    )
    .unwrap();
    let file = fs::read_dir(dir.path().join(FOLDER))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = fs::read_to_string(&file)
        .unwrap()
        .replace("\"version\":1", "\"version\":99");
    fs::write(&file, text).unwrap();

    let problem = session::load(dir.path(), project).unwrap_err();

    assert!(problem.contains("newer version"));
}
