use std::fs;
use std::path::PathBuf;

use super::*;

fn path(name: &str) -> PathBuf {
    PathBuf::from(format!("/p/{name}"))
}

#[test]
fn the_latest_is_first_and_a_path_is_listed_once() {
    let mut recent = Recent::default();

    assert!(recent.push_file(&path("a")));
    assert!(recent.push_file(&path("b")));
    assert!(recent.push_file(&path("a")));

    assert_eq!(recent.files(), [path("a"), path("b")]);
}

#[test]
fn a_path_that_is_first_already_changes_nothing() {
    let mut recent = Recent::default();
    recent.push_folder(&path("x"));

    assert!(!recent.push_folder(&path("x")));
}

#[test]
fn only_the_latest_ten_are_kept_of_each() {
    let mut recent = Recent::default();

    for number in 0..15 {
        recent.push_file(&path(&format!("f{number}")));
        recent.push_folder(&path(&format!("d{number}")));
    }

    assert_eq!(recent.files().len(), MOST);
    assert_eq!(recent.files()[0], path("f14"));
    assert_eq!(recent.files()[MOST - 1], path("f5"));
    assert_eq!(recent.folders().len(), MOST);
}

#[test]
fn what_is_gone_from_the_disk_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let (kept, gone) = (dir.path().join("kept.txt"), dir.path().join("gone.txt"));
    fs::write(&kept, "").unwrap();
    let mut recent = Recent::default();
    recent.push_file(&gone);
    recent.push_file(&kept);
    recent.push_folder(dir.path());
    recent.push_folder(&dir.path().join("no such folder"));

    assert!(recent.prune());

    assert_eq!(recent.files(), [kept]);
    assert_eq!(recent.folders(), [dir.path().to_path_buf()]);
    assert!(!recent.prune(), "nothing more to drop");
}

#[test]
fn a_file_is_not_taken_for_a_folder_when_pruning() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f");
    fs::write(&file, "").unwrap();
    let mut recent = Recent::default();
    recent.push_folder(&file);

    recent.prune();

    assert!(recent.folders().is_empty());
}

#[test]
fn clearing_forgets_everything_and_says_whether_there_was_anything() {
    let mut recent = Recent::default();
    assert!(!recent.clear());
    recent.push_file(&path("a"));

    assert!(recent.clear());

    assert!(recent.is_empty());
}

#[test]
fn a_list_survives_a_write_and_a_read_and_leaves_no_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut recent = Recent::default();
    recent.push_file(&path("a"));
    recent.push_folder(&path("d"));

    save(dir.path(), &recent).unwrap();
    let (loaded, warning) = load(Some(dir.path()));

    assert!(warning.is_none());
    assert_eq!(loaded, recent);
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, [RECENT_FILE]);
}

#[test]
fn a_missing_file_is_normal_and_a_missing_folder_is_made_on_save() {
    let dir = tempfile::tempdir().unwrap();
    let inside = dir.path().join("config");

    let (loaded, warning) = load(Some(&inside));
    assert!(loaded.is_empty() && warning.is_none());

    save(&inside, &loaded).unwrap();
    assert!(path_in(&inside).is_file());
}

#[test]
fn no_config_folder_means_nothing_remembered_and_no_warning() {
    let (loaded, warning) = load(None);

    assert!(loaded.is_empty() && warning.is_none());
}

#[test]
fn a_corrupt_file_gives_an_empty_list_and_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(path_in(dir.path()), "{ not json").unwrap();

    let (loaded, warning) = load(Some(dir.path()));

    assert!(loaded.is_empty());
    let text = warning.unwrap().to_string();
    assert!(
        text.contains(RECENT_FILE) && text.contains("recent files"),
        "{text}"
    );
}

#[test]
fn a_file_with_too_many_entries_or_unknown_names_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let many: Vec<String> = (0..30).map(|n| format!("\"/p/f{n}\"")).collect();
    fs::write(
        path_in(dir.path()),
        format!("{{ \"files\": [{}], \"later\": 1 }}", many.join(",")),
    )
    .unwrap();

    let (loaded, warning) = load(Some(dir.path()));

    assert!(warning.is_none());
    assert_eq!(loaded.files().len(), MOST);
    assert!(loaded.folders().is_empty());
}
