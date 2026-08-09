use std::{fs, path::PathBuf};

use super::{Editor, StartupArgs};

fn editor() -> Editor {
    Editor::new(&StartupArgs::new(PathBuf::from(".")))
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("editor_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn editing_active_file_marks_it_dirty() {
    let dir = temp_dir("dirty");
    let path = dir.join("a.rs");
    fs::write(&path, "one").unwrap();

    let mut e = editor();
    e.open(&path).unwrap();
    assert!(!e.tabs.active().unwrap().is_dirty());

    e.insert_char('x');
    assert!(e.tabs.active().unwrap().is_dirty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dirty_flag_persists_across_tab_switch() {
    let dir = temp_dir("switch");
    let a = dir.join("a.rs");
    let b = dir.join("b.rs");
    fs::write(&a, "one").unwrap();
    fs::write(&b, "two").unwrap();

    let mut e = editor();
    e.open(&a).unwrap();
    e.open(&b).unwrap();

    e.insert_char('x');
    assert!(e.tabs.active().unwrap().is_dirty());

    e.switch_tab(-1);
    assert!(!e.tabs.active().unwrap().is_dirty());

    e.insert_char('y');
    assert!(e.tabs.active().unwrap().is_dirty());

    e.switch_tab(1);
    assert!(e.tabs.active().unwrap().is_dirty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn edits_are_committed_when_switching_tabs() {
    let dir = temp_dir("commit_switch");
    let a = dir.join("a.rs");
    let b = dir.join("b.rs");
    fs::write(&a, "one").unwrap();
    fs::write(&b, "two").unwrap();

    let mut e = editor();
    e.open(&a).unwrap();
    e.open(&b).unwrap();

    e.insert_char('x');
    e.switch_tab(-1);
    e.switch_tab(1);

    assert_eq!(e.tabs.active().unwrap().content(), "xtwo");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn edits_are_committed_when_opening_another_file() {
    let dir = temp_dir("commit_open");
    let a = dir.join("a.rs");
    let b = dir.join("b.rs");
    fs::write(&a, "one").unwrap();
    fs::write(&b, "two").unwrap();

    let mut e = editor();
    e.open(&a).unwrap();

    e.insert_char('x');
    e.open(&b).unwrap();

    e.switch_tab(-1);
    assert_eq!(e.tabs.active().unwrap().content(), "xone");
    assert!(e.tabs.active().unwrap().is_dirty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn save_writes_file_and_marks_clean() {
    let dir = temp_dir("save");
    let path = dir.join("a.rs");
    fs::write(&path, "one").unwrap();

    let mut e = editor();
    e.open(&path).unwrap();
    e.insert_char('x');
    assert!(e.tabs.active().unwrap().is_dirty());

    assert!(e.save_active().unwrap());
    assert_eq!(fs::read_to_string(&path).unwrap(), "xone");
    assert!(!e.tabs.active().unwrap().is_dirty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn save_is_noop_when_clean() {
    let dir = temp_dir("save_clean");
    let path = dir.join("a.rs");
    fs::write(&path, "one").unwrap();

    let mut e = editor();
    e.open(&path).unwrap();

    assert!(!e.save_active().unwrap());
    assert_eq!(fs::read_to_string(&path).unwrap(), "one");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn save_is_noop_without_active_file() {
    let mut e = editor();
    assert!(!e.save_active().unwrap());
}

#[test]
fn editing_after_save_marks_dirty_again() {
    let dir = temp_dir("save_again");
    let path = dir.join("a.rs");
    fs::write(&path, "one").unwrap();

    let mut e = editor();
    e.open(&path).unwrap();
    e.insert_char('x');
    assert!(e.save_active().unwrap());
    assert!(!e.tabs.active().unwrap().is_dirty());

    e.insert_char('y');
    assert!(e.tabs.active().unwrap().is_dirty());

    assert!(e.save_active().unwrap());
    assert_eq!(fs::read_to_string(&path).unwrap(), "xyone");

    let _ = fs::remove_dir_all(&dir);
}
