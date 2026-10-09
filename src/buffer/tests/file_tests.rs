use std::fs;

use crate::buffer::edit::Edit;
use crate::buffer::line_ending::LineEnding;
use crate::buffer::selection::Selections;
use crate::buffer::{Buffer, DiskChange, FileError, Position};

fn type_text(buffer: &mut Buffer, text: &str) {
    let mut transaction = buffer.begin_transaction(Selections::default());
    transaction.apply(&Edit::insert(0, text)).unwrap();
    transaction.commit(Selections::default());
}

#[test]
fn open_reads_text_path_and_line_ending() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "one\r\ntwo\r\n").unwrap();

    let buffer = Buffer::open(&path).unwrap();

    assert_eq!(buffer.text(), "one\r\ntwo\r\n");
    assert_eq!(buffer.line_ending(), LineEnding::Crlf);
    assert_eq!(buffer.path(), Some(path.as_path()));
    assert!(!buffer.is_dirty());
}

#[test]
fn open_missing_file_is_an_io_error_naming_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.txt");

    let error = Buffer::open(&path).unwrap_err();

    assert!(matches!(error, FileError::Io { .. }));
    assert!(error.to_string().contains("missing.txt"));
}

#[test]
fn open_rejects_invalid_utf8() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bin");
    fs::write(&path, [0x66, 0xff, 0xfe]).unwrap();

    let error = Buffer::open(&path).unwrap_err();

    assert!(matches!(error, FileError::NotUtf8 { .. }));
}

#[test]
fn editing_makes_the_buffer_dirty_and_save_cleans_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "world").unwrap();
    let mut buffer = Buffer::open(&path).unwrap();

    type_text(&mut buffer, "hello ");
    assert!(buffer.is_dirty());

    buffer.save().unwrap();

    assert!(!buffer.is_dirty());
    assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
}

#[test]
fn save_keeps_crlf_text_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "a\r\nb\r\n").unwrap();
    let mut buffer = Buffer::open(&path).unwrap();

    buffer.save().unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"a\r\nb\r\n");
}

#[test]
fn save_without_a_path_is_an_error() {
    let mut buffer = Buffer::from_text("x");

    assert!(matches!(buffer.save(), Err(FileError::NoPath)));
}

#[test]
fn save_as_writes_the_file_and_adopts_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.txt");
    let mut buffer = Buffer::from_text("fresh");

    buffer.save_as(&path).unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "fresh");
    assert_eq!(buffer.path(), Some(path.as_path()));
}

#[test]
fn save_leaves_no_temporary_files_behind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    let mut buffer = Buffer::from_text("x");

    buffer.save_as(&path).unwrap();

    let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1);
}

#[test]
fn failed_save_keeps_the_buffer_dirty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("no_such_dir").join("a.txt");
    let mut buffer = Buffer::from_text("");
    type_text(&mut buffer, "x");

    let result = buffer.save_as(&path);

    assert!(result.is_err());
    assert!(buffer.is_dirty());
}

fn file_with(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, text).unwrap();
    (dir, path)
}

#[test]
fn a_freshly_opened_buffer_matches_the_disk() {
    let (_dir, path) = file_with("one\n");

    let buffer = Buffer::open(&path).unwrap();

    assert_eq!(buffer.disk_change().unwrap(), None);
}

#[test]
fn the_buffers_own_save_is_not_a_change_on_disk() {
    let (_dir, path) = file_with("one\n");
    let mut buffer = Buffer::open(&path).unwrap();
    type_text(&mut buffer, "zero ");

    buffer.save().unwrap();

    assert_eq!(buffer.disk_change().unwrap(), None);
}

#[test]
fn someone_elses_write_is_a_change_and_the_same_change_is_recognized_again() {
    let (_dir, path) = file_with("one\n");
    let buffer = Buffer::open(&path).unwrap();

    fs::write(&path, "two\n").unwrap();
    let first = buffer.disk_change().unwrap();
    let again = buffer.disk_change().unwrap();

    assert!(matches!(first, Some(DiskChange::Modified(_))));
    assert_eq!(first, again);
    fs::write(&path, "three\n").unwrap();
    assert_ne!(buffer.disk_change().unwrap(), first);
}

#[test]
fn rewriting_the_same_contents_is_not_a_change() {
    let (_dir, path) = file_with("one\n");
    let buffer = Buffer::open(&path).unwrap();

    fs::write(&path, "one\n").unwrap();

    assert_eq!(buffer.disk_change().unwrap(), None);
}

#[test]
fn a_removed_file_is_reported_as_deleted() {
    let (_dir, path) = file_with("one\n");
    let buffer = Buffer::open(&path).unwrap();

    fs::remove_file(&path).unwrap();

    assert_eq!(buffer.disk_change().unwrap(), Some(DiskChange::Deleted));
}

#[test]
fn a_new_buffer_for_a_missing_file_notices_when_the_file_appears() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.txt");
    let buffer = Buffer::open_or_new(&path).unwrap();
    assert_eq!(buffer.disk_change().unwrap(), None);

    fs::write(&path, "someone made me\n").unwrap();

    assert!(matches!(
        buffer.disk_change().unwrap(),
        Some(DiskChange::Modified(_))
    ));
}

#[test]
fn a_buffer_without_a_file_has_nothing_to_compare() {
    let buffer = Buffer::from_text("scratch");

    assert_eq!(buffer.disk_change().unwrap(), None);
}

#[test]
fn reload_takes_the_text_from_disk_and_leaves_the_buffer_clean_without_history() {
    let (_dir, path) = file_with("one\n");
    let mut buffer = Buffer::open(&path).unwrap();
    type_text(&mut buffer, "mine ");
    fs::write(&path, "theirs\r\nlines\r\n").unwrap();

    buffer.reload().unwrap();

    assert_eq!(buffer.text(), "theirs\r\nlines\r\n");
    assert_eq!(buffer.line_ending(), LineEnding::Crlf);
    assert!(!buffer.is_dirty());
    assert_eq!(buffer.disk_change().unwrap(), None);
    assert_eq!(
        buffer.undo().unwrap(),
        None,
        "history described the old text"
    );
}

#[test]
fn reload_bumps_the_version_so_cached_results_are_stale() {
    let (_dir, path) = file_with("one\n");
    let mut buffer = Buffer::open(&path).unwrap();
    let before = buffer.version();
    fs::write(&path, "two\n").unwrap();

    buffer.reload().unwrap();

    assert!(buffer.version() > before);
}

#[test]
fn a_failed_reload_leaves_the_buffer_as_it_was() {
    let (_dir, path) = file_with("one\n");
    let mut buffer = Buffer::open(&path).unwrap();
    type_text(&mut buffer, "mine ");
    fs::write(&path, [0xff, 0xfe]).unwrap();

    let error = buffer.reload().unwrap_err();

    assert!(matches!(error, FileError::NotUtf8 { .. }));
    assert_eq!(buffer.text(), "mine one\n");
    assert!(buffer.is_dirty());
}

#[test]
fn reloading_a_buffer_without_a_path_is_an_error() {
    let mut buffer = Buffer::from_text("scratch");

    assert!(matches!(buffer.reload(), Err(FileError::NoPath)));
}

#[test]
fn clamp_position_moves_to_the_nearest_place_in_the_text() {
    let buffer = Buffer::from_text("ab\ncdef\n");

    assert_eq!(
        buffer.clamp_position(Position::new(1, 2)),
        Position::new(1, 2)
    );
    assert_eq!(
        buffer.clamp_position(Position::new(0, 9)),
        Position::new(0, 2)
    );
    assert_eq!(
        buffer.clamp_position(Position::new(9, 1)),
        Position::new(2, 0)
    );
    assert_eq!(
        buffer.clamp_position(Position::new(9, 9)),
        Position::new(2, 0)
    );
}
