use std::fs;

use crate::initial_state;

#[test]
fn no_path_starts_with_an_empty_unnamed_buffer() {
    let state = initial_state(None).unwrap();

    assert_eq!(state.editor().buffer().text(), "");
    assert_eq!(state.editor().buffer().path(), None);
    assert_eq!(state.status(), None);
}

#[test]
fn an_existing_file_is_opened_without_a_message() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("todo.md");
    fs::write(&path, "# TODO\n- thing\n").unwrap();

    let state = initial_state(Some(&path)).unwrap();

    assert_eq!(state.editor().buffer().text(), "# TODO\n- thing\n");
    assert_eq!(state.status(), None);
}

#[test]
fn a_missing_file_opens_empty_and_says_it_is_new() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("typo.md");

    let state = initial_state(Some(&path)).unwrap();

    assert_eq!(state.editor().buffer().text(), "");
    assert_eq!(state.editor().buffer().path(), Some(path.as_path()));
    let status = state.status().unwrap();
    assert!(status.contains("new file"), "{status}");
    assert!(status.contains("typo.md"), "{status}");
    assert!(!path.exists(), "opening must not create the file");
}

#[test]
fn a_file_that_is_not_utf8_is_an_error_not_an_empty_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("binary");
    fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();

    assert!(initial_state(Some(&path)).is_err());
}
