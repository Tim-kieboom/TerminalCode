use std::fs;

use crate::buffer::edit::Edit;
use crate::buffer::line_ending::LineEnding;
use crate::buffer::selection::Selections;
use crate::buffer::{Buffer, FileError};

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
