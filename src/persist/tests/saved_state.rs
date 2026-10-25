use std::fs;

use crate::persist::saved_state::{SavedState, load, path_in, save};

#[test]
fn a_corrupt_file_is_kept_aside_and_the_next_save_does_not_destroy_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(dir.path());
    fs::write(&path, "{ not json").unwrap();

    let (state, warning) = load(Some(dir.path()));
    assert_eq!(state, SavedState::default());
    let warning = warning.expect("a warning").to_string();
    assert!(warning.contains("state.json.corrupt"), "{warning}");

    save(dir.path(), &state).unwrap();
    let kept = dir.path().join("state.json.corrupt");
    assert_eq!(fs::read_to_string(kept).unwrap(), "{ not json");
}

#[test]
fn an_earlier_backup_is_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(dir.path());
    fs::write(dir.path().join("state.json.corrupt"), "first").unwrap();
    fs::write(&path, "second").unwrap();

    let (_, warning) = load(Some(dir.path()));
    assert!(warning.is_some());
    let first = fs::read_to_string(dir.path().join("state.json.corrupt")).unwrap();
    let second = fs::read_to_string(dir.path().join("state.json.corrupt.1")).unwrap();
    assert_eq!((first.as_str(), second.as_str()), ("first", "second"));
}

#[test]
fn a_missing_or_valid_file_is_not_moved() {
    let dir = tempfile::tempdir().unwrap();
    assert!(load(Some(dir.path())).1.is_none());

    save(dir.path(), &SavedState::default()).unwrap();
    assert!(load(Some(dir.path())).1.is_none());
    assert!(path_in(dir.path()).exists());
    assert!(!dir.path().join("state.json.corrupt").exists());
}
