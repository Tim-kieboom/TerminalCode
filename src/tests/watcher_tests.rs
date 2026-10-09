use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::event::Event;
use crate::watcher::FsWatcher;

fn watcher() -> (FsWatcher, mpsc::Receiver<Event>) {
    let (tx, rx) = mpsc::channel(256);
    (FsWatcher::new(tx).unwrap(), rx)
}

fn set(paths: &[&Path]) -> BTreeSet<PathBuf> {
    paths.iter().map(|path| path.to_path_buf()).collect()
}

/// Whether a change event naming a path that ends with `name` arrives within
/// `wait`.
fn event_for(rx: &mut mpsc::Receiver<Event>, name: &str, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        while let Ok(event) = rx.try_recv() {
            if let Event::FilesChanged(paths) = event
                && paths.iter().any(|path| path.ends_with(name))
            {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

#[test]
fn forgetting_a_watched_directory_lets_watch_only_start_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(dir.path()).unwrap();
    let (mut watcher, _rx) = watcher();
    watcher.watch_only(&set(&[&path])).unwrap();
    assert!(watcher.watched().contains(&path));

    assert!(watcher.forget(std::slice::from_ref(&path)));
    assert!(watcher.watched().is_empty());

    watcher.watch_only(&set(&[&path])).unwrap();
    assert!(watcher.watched().contains(&path));
}

#[test]
fn forgetting_paths_that_are_not_watched_directories_does_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(dir.path()).unwrap();
    let (mut watcher, _rx) = watcher();
    watcher.watch_only(&set(&[&path])).unwrap();

    let forgot = watcher.forget(&[path.join("a_file.txt"), PathBuf::from("/elsewhere")]);

    assert!(!forgot);
    assert!(watcher.watched().contains(&path));
}

#[test]
fn a_missing_directory_is_not_an_error_and_is_picked_up_when_it_appears() {
    let dir = tempfile::tempdir().unwrap();
    let later = fs::canonicalize(dir.path()).unwrap().join("later");
    let (mut watcher, _rx) = watcher();

    watcher.watch_only(&set(&[&later])).unwrap();
    assert!(watcher.watched().is_empty(), "nothing to watch yet");

    fs::create_dir(&later).unwrap();
    watcher.watch_only(&set(&[&later])).unwrap();
    assert!(watcher.watched().contains(&later));
}

#[test]
fn directories_that_are_no_longer_wanted_stop_being_watched() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (a, b) = (root.join("a"), root.join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    let (mut watcher, _rx) = watcher();
    watcher.watch_only(&set(&[&a, &b])).unwrap();

    watcher.watch_only(&set(&[&b])).unwrap();

    assert_eq!(watcher.watched(), &set(&[&b]));
}

#[test]
fn changes_keep_arriving_after_a_watched_directory_is_deleted_and_made_again() {
    let dir = tempfile::tempdir().unwrap();
    let build = fs::canonicalize(dir.path()).unwrap().join("build");
    fs::create_dir(&build).unwrap();
    let (mut watcher, mut rx) = watcher();
    watcher.watch_only(&set(&[&build])).unwrap();
    fs::write(build.join("before.txt"), "").unwrap();
    assert!(
        event_for(&mut rx, "before.txt", Duration::from_secs(10)),
        "the baseline works"
    );

    // The operating system drops the watch with the directory. What the
    // report names is the directory itself.
    fs::remove_dir_all(&build).unwrap();
    fs::create_dir(&build).unwrap();
    assert!(watcher.forget(std::slice::from_ref(&build)));
    watcher.watch_only(&set(&[&build])).unwrap();
    fs::write(build.join("after.txt"), "").unwrap();

    assert!(
        event_for(&mut rx, "after.txt", Duration::from_secs(10)),
        "no change event from the directory that was made again"
    );
}
