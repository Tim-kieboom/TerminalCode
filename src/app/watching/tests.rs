use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use super::Watching;
use crate::watcher::FsWatcher;

fn attached() -> Watching {
    let (tx, _rx) = mpsc::channel(16);
    let mut watching = Watching::default();
    watching.attach(FsWatcher::new(tx).unwrap());
    watching
}

fn set(path: &Path) -> BTreeSet<PathBuf> {
    BTreeSet::from([path.to_path_buf()])
}

#[test]
fn without_a_watcher_nothing_is_synced_and_the_closure_is_not_asked() {
    let mut watching = Watching::default();
    let mut asked = false;

    let error = watching.sync((1, 1), || {
        asked = true;
        BTreeSet::new()
    });

    assert!(error.is_none());
    assert!(!asked);
    assert!(watching.watched().is_empty());
}

#[test]
fn what_to_watch_is_only_worked_out_when_the_versions_changed() {
    let dir = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(dir.path()).unwrap();
    let mut watching = attached();
    let mut asked = 0;
    let mut sync = |watching: &mut Watching, versions| {
        watching.sync(versions, || {
            asked += 1;
            set(&path)
        })
    };

    sync(&mut watching, (1, 1));
    sync(&mut watching, (1, 1));
    sync(&mut watching, (1, 2));
    sync(&mut watching, (2, 2));
    sync(&mut watching, (2, 2));

    assert_eq!(asked, 3);
    assert_eq!(watching.watched(), vec![path]);
}

#[test]
fn forgetting_a_replaced_directory_makes_the_next_sync_look_again_even_at_the_same_versions() {
    let dir = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(dir.path()).unwrap();
    let mut watching = attached();
    let mut asked = 0;
    watching.sync((1, 1), || {
        asked += 1;
        set(&path)
    });

    watching.forget(std::slice::from_ref(&path));
    assert!(watching.watched().is_empty());
    watching.sync((1, 1), || {
        asked += 1;
        set(&path)
    });

    assert_eq!(asked, 2);
    assert_eq!(watching.watched(), vec![path]);
}

#[test]
fn forgetting_paths_that_are_not_watched_directories_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(dir.path()).unwrap();
    let mut watching = attached();
    let mut asked = 0;
    watching.sync((1, 1), || set(&path));

    watching.forget(&[path.join("file.txt")]);
    watching.sync((1, 1), || {
        asked += 1;
        set(&path)
    });

    assert_eq!(
        asked, 0,
        "the versions still match, so nothing was looked at"
    );
}

#[test]
fn forgetting_without_a_watcher_is_harmless() {
    let mut watching = Watching::default();

    watching.forget(&[PathBuf::from("/anywhere")]);

    assert!(watching.watched().is_empty());
}
