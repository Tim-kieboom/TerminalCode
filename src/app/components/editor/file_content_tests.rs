use std::path::PathBuf;

use super::FileContent;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("file_content_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn read_from_path_normalizes_crlf() {
    let dir = temp_dir("crlf");
    let path = dir.join("file.txt");
    std::fs::write(&path, "one\r\ntwo").unwrap();

    let file = FileContent::read_from_path(&path).unwrap();
    assert_eq!(file.content(), "one\ntwo");
    assert_eq!(file.name(), "file.txt");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn content_mut_updates_content() {
    let mut file = FileContent::new("a.rs", "one");
    *file.content_mut() = "two".to_string();
    assert_eq!(file.content(), "two");
}

#[test]
fn new_starts_clean() {
    let file = FileContent::new("a.rs", "one");
    assert!(!file.is_dirty());
}

#[test]
fn read_from_path_starts_clean() {
    let dir = temp_dir("clean");
    let path = dir.join("a.rs");
    std::fs::write(&path, "one").unwrap();

    let file = FileContent::read_from_path(&path).unwrap();
    assert!(!file.is_dirty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mark_dirty_flags_file() {
    let mut file = FileContent::new("a.rs", "one");
    file.mark_dirty();
    assert!(file.is_dirty());
}
