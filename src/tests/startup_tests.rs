use std::fs;

use crate::initial_state;

#[test]
fn no_path_starts_with_an_empty_unnamed_buffer() {
    let state = initial_state(&[]).unwrap();

    assert_eq!(state.editor().buffer().text(), "");
    assert_eq!(state.editor().buffer().path(), None);
    assert_eq!(state.latest_notification(), None);
}

#[test]
fn an_existing_file_is_opened_without_a_message() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("todo.md");
    fs::write(&path, "# TODO\n- thing\n").unwrap();

    let state = initial_state(std::slice::from_ref(&path)).unwrap();

    assert_eq!(state.editor().buffer().text(), "# TODO\n- thing\n");
    assert_eq!(state.latest_notification(), None);
}

#[test]
fn a_missing_file_opens_empty_and_says_it_is_new() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("typo.md");

    let state = initial_state(std::slice::from_ref(&path)).unwrap();

    assert_eq!(state.editor().buffer().text(), "");
    assert_eq!(state.editor().buffer().path(), Some(path.as_path()));
    let status = state.latest_notification().unwrap();
    assert!(status.contains("new file"), "{status}");
    assert!(status.contains("typo.md"), "{status}");
    assert!(!path.exists(), "opening must not create the file");
}

#[test]
fn a_file_that_is_not_utf8_is_an_error_not_an_empty_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("binary");
    fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();

    assert!(initial_state(std::slice::from_ref(&path)).is_err());
}

#[test]
fn several_paths_open_as_tabs_with_the_first_active() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b, c) = (
        dir.path().join("a.txt"),
        dir.path().join("b.txt"),
        dir.path().join("c.txt"),
    );
    fs::write(&a, "alpha").unwrap();
    fs::write(&b, "beta").unwrap();
    fs::write(&c, "gamma").unwrap();

    let state = initial_state(&[a, b, c]).unwrap();

    let (names, active) = state.workspace().tab_names();
    assert_eq!(names, ["a.txt", "b.txt", "c.txt"]);
    assert_eq!(active, 0);
    assert_eq!(state.editor().buffer().text(), "alpha");
    assert_eq!(state.latest_notification(), None);
}

#[test]
fn missing_files_among_several_are_all_reported() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("here.txt");
    fs::write(&existing, "x").unwrap();
    let (new_a, new_b) = (dir.path().join("new_a.txt"), dir.path().join("new_b.txt"));

    let state = initial_state(&[new_a, existing, new_b]).unwrap();

    let status = state.latest_notification().unwrap();
    assert!(
        status.contains("new_a.txt") && status.contains("new_b.txt"),
        "{status}"
    );
    assert!(!status.contains("here.txt"), "{status}");
}

fn args(list: &[&str]) -> impl Iterator<Item = std::ffi::OsString> {
    list.iter()
        .map(std::ffi::OsString::from)
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn every_argument_after_the_program_name_is_a_path() {
    use std::path::PathBuf;

    assert_eq!(crate::paths_from_args(args(&["tc"])), Vec::<PathBuf>::new());
    assert_eq!(
        crate::paths_from_args(args(&["tc", "a.rs", "b.rs"])),
        [PathBuf::from("a.rs"), PathBuf::from("b.rs")]
    );
}

#[test]
fn a_leading_double_dash_is_skipped_but_later_ones_are_files() {
    use std::path::PathBuf;

    assert_eq!(
        crate::paths_from_args(args(&["tc", "--", "notes.md"])),
        [PathBuf::from("notes.md")]
    );
    assert_eq!(
        crate::paths_from_args(args(&["tc", "--"])),
        Vec::<PathBuf>::new()
    );
    assert_eq!(
        crate::paths_from_args(args(&["tc", "--", "--odd", "--"])),
        [PathBuf::from("--odd"), PathBuf::from("--")]
    );
}

#[test]
fn a_directory_argument_is_the_project_and_the_rest_are_files() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "").unwrap();

    let (project, files) =
        crate::project_and_files(&[file.clone(), dir.path().to_path_buf(), dir.path().join("b")]);

    assert_eq!(project.as_deref(), Some(dir.path()));
    assert_eq!(files, [file, dir.path().join("b")]);
}

#[test]
fn without_a_directory_argument_there_is_no_named_project() {
    let (project, files) = crate::project_and_files(&["a.txt".into()]);

    assert_eq!(project, None);
    assert_eq!(files.len(), 1);
}
