use std::fs;
use std::path::Path;

use crate::persist::recovery::{self, FOLDER, MAX_BYTES, Recovery};

fn item(project: &Path, file: &str, text: &str) -> Recovery {
    Recovery::new(project, Some(&project.join(file)), text.to_owned())
}

#[test]
fn what_is_written_is_found_again_for_its_project() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/work/one");
    let written = item(project, "a.rs", "fn a() {}\n")
        .with_disk(Some(42))
        .with_cursor(3, 7);

    assert_eq!(
        recovery::write(dir.path(), std::slice::from_ref(&written)).unwrap(),
        1
    );
    let search = recovery::find(dir.path(), project);

    assert_eq!(search.found.len(), 1);
    assert_eq!(search.found[0].recovery, written);
    assert!(search.problems.is_empty());
}

#[test]
fn another_projects_files_are_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let one = Path::new("/work/one");
    let two = Path::new("/work/two");
    recovery::write(
        dir.path(),
        &[item(one, "a.rs", "x"), item(two, "a.rs", "y")],
    )
    .unwrap();

    let search = recovery::find(dir.path(), two);

    assert_eq!(search.found.len(), 1);
    assert_eq!(search.found[0].recovery.text, "y");
}

#[test]
fn a_second_write_of_a_document_replaces_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/work/one");
    recovery::write(dir.path(), &[item(project, "a.rs", "first")]).unwrap();
    recovery::write(dir.path(), &[item(project, "a.rs", "second")]).unwrap();

    let search = recovery::find(dir.path(), project);

    assert_eq!(search.found.len(), 1);
    assert_eq!(search.found[0].recovery.text, "second");
}

#[test]
fn texts_without_a_file_each_get_their_own_file() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/work/one");
    let first = Recovery::new(project, None, "one".to_owned());
    let second = Recovery::new(project, None, "two".to_owned()).with_title(Some("notes"));

    assert_eq!(recovery::write(dir.path(), &[first, second]).unwrap(), 2);
    let search = recovery::find(dir.path(), project);

    let mut names: Vec<String> = search.found.iter().map(|f| f.recovery.name()).collect();
    names.sort();
    assert_eq!(names, ["[no name]", "notes"]);
}

#[test]
fn a_text_over_the_cap_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/work/one");
    let big = item(project, "big.txt", &"x".repeat(MAX_BYTES + 1));
    let small = item(project, "small.txt", "x");

    assert_eq!(recovery::write(dir.path(), &[big, small]).unwrap(), 1);

    let search = recovery::find(dir.path(), project);
    assert_eq!(search.found.len(), 1);
    assert_eq!(search.found[0].recovery.name(), "small.txt");
}

#[test]
fn a_corrupt_file_is_reported_and_left_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/work/one");
    recovery::write(dir.path(), &[item(project, "a.rs", "ok")]).unwrap();
    let bad = dir.path().join(FOLDER).join("broken.json");
    fs::write(&bad, "{ not json").unwrap();

    let search = recovery::find(dir.path(), project);

    assert_eq!(search.found.len(), 1);
    assert_eq!(search.problems.len(), 1);
    assert!(search.problems[0].contains("broken.json"));
    assert!(bad.exists());
}

#[test]
fn a_file_of_a_newer_layout_is_not_used() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join(FOLDER);
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("new.json"),
        r#"{"version":99,"project":"/work/one","file":null,"text":"x","line":0,"column":0}"#,
    )
    .unwrap();

    let search = recovery::find(dir.path(), Path::new("/work/one"));

    assert!(search.found.is_empty());
    assert!(search.problems[0].contains("newer version"));
}

#[test]
fn nothing_is_found_without_a_recovery_folder() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(
        recovery::find(dir.path(), Path::new("/work/one")),
        recovery::Search::default()
    );
}

#[test]
fn removing_files_deletes_them_and_ignores_ones_that_are_gone() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/work/one");
    recovery::write(dir.path(), &[item(project, "a.rs", "x")]).unwrap();
    let found = recovery::find(dir.path(), project).found;
    let mut paths: Vec<_> = found.iter().map(|f| f.path.clone()).collect();
    paths.push(dir.path().join(FOLDER).join("gone.json"));

    recovery::remove(&paths).unwrap();

    assert!(recovery::find(dir.path(), project).found.is_empty());
}

#[cfg(unix)]
#[test]
fn the_files_and_the_folder_are_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/work/one");
    recovery::write(dir.path(), &[item(project, "secret.env", "KEY=1")]).unwrap();

    let found = recovery::find(dir.path(), project).found;

    let file_mode = fs::metadata(&found[0].path).unwrap().permissions().mode();
    let folder_mode = fs::metadata(dir.path().join(FOLDER))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(file_mode & 0o777, 0o600);
    assert_eq!(folder_mode & 0o777, 0o700);
}

#[test]
fn a_path_cannot_make_a_file_outside_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/work/one");
    let sneaky = Recovery::new(
        project,
        Some(Path::new("/work/one/../../x/../../y")),
        "x".into(),
    );

    recovery::write(dir.path(), &[sneaky]).unwrap();

    let entries: Vec<_> = fs::read_dir(dir.path().join(FOLDER)).unwrap().collect();
    assert_eq!(entries.len(), 1);
    assert!(!dir.path().join("y").exists());
}

#[test]
fn a_later_write_does_not_leave_old_unsaved_texts_to_come_back() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/work/one");
    let untitled = |text: &str| Recovery::new(project, None, text.to_owned());
    recovery::write(dir.path(), &[untitled("one"), untitled("two")]).unwrap();

    recovery::write(dir.path(), &[untitled("three")]).unwrap();

    let search = recovery::find(dir.path(), project);
    let texts: Vec<&str> = search
        .found
        .iter()
        .map(|found| found.recovery.text.as_str())
        .collect();
    assert_eq!(texts, ["three"]);
}

#[test]
fn a_file_left_half_written_is_removed_by_the_next_write() {
    let dir = tempfile::tempdir().unwrap();
    let project = Path::new("/work/one");
    recovery::write(dir.path(), &[item(project, "a.rs", "x")]).unwrap();
    let leftover = dir.path().join(FOLDER).join("0123456789abcdef.json.tmp");
    fs::write(&leftover, "{").unwrap();

    recovery::write(dir.path(), &[item(project, "a.rs", "y")]).unwrap();

    assert!(!leftover.exists());
}
