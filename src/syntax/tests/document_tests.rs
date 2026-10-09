use std::fs;
use std::path::Path;

use ratatui::style::Color;

use crate::buffer::{Buffer, Edit};
use crate::syntax::DocumentSyntax;
use crate::ui::theme::Theme;

/// A list of just one range.
fn one(range: std::ops::Range<usize>) -> Vec<std::ops::Range<usize>> {
    vec![range]
}

fn theme() -> Theme {
    Theme::from_toml("[\"syntax.keyword\"]\ntext = \"red\"\n").unwrap()
}

fn buffer_in(dir: &Path, name: &str, text: &str) -> Buffer {
    let path = dir.join(name);
    fs::write(&path, text).unwrap();
    Buffer::open(path).unwrap()
}

fn colored(syntax: &DocumentSyntax, text: &str) -> Vec<String> {
    syntax
        .spans()
        .iter()
        .filter(|span| span.style.fg == Some(Color::Red))
        .map(|span| text[span.range.clone()].to_owned())
        .collect()
}

#[test]
fn a_rust_file_gets_spans_for_the_ranges_asked() {
    let dir = tempfile::tempdir().unwrap();
    let buffer = buffer_in(dir.path(), "a.rs", "fn a() {}\nfn b() {}\n");
    let mut syntax = DocumentSyntax::default();

    syntax.update(&buffer, &theme(), &one(0..buffer.len_bytes()));

    assert_eq!(colored(&syntax, &buffer.text()), ["fn", "fn"]);
}

#[test]
fn only_the_given_ranges_are_colored() {
    let dir = tempfile::tempdir().unwrap();
    let buffer = buffer_in(dir.path(), "a.rs", "fn a() {}\nfn b() {}\nfn c() {}\n");
    let mut syntax = DocumentSyntax::default();
    let (first, last) = (0..9, 20..29);

    syntax.update(&buffer, &theme(), &[first, last]);

    let starts: Vec<_> = syntax.spans().iter().map(|span| span.range.start).collect();
    assert_eq!(starts, [0, 20]);
}

#[test]
fn a_file_with_no_known_language_gets_no_spans() {
    let dir = tempfile::tempdir().unwrap();
    let buffer = buffer_in(dir.path(), "notes.txt", "fn a() {}\n");
    let mut syntax = DocumentSyntax::default();

    syntax.update(&buffer, &theme(), &one(0..10));

    assert!(syntax.spans().is_empty());
    assert!(syntax.take_error().is_none());
}

#[test]
fn a_buffer_without_a_path_gets_no_spans() {
    let buffer = Buffer::from_text("fn a() {}\n");
    let mut syntax = DocumentSyntax::default();

    syntax.update(&buffer, &theme(), &one(0..10));

    assert!(syntax.spans().is_empty());
}

#[test]
fn edited_text_is_parsed_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut buffer = buffer_in(dir.path(), "a.rs", "let a = 1;\n");
    let mut syntax = DocumentSyntax::default();
    syntax.update(&buffer, &theme(), &one(0..buffer.len_bytes()));
    assert_eq!(colored(&syntax, &buffer.text()), ["let"]);

    let mut transaction = buffer.begin_transaction(Default::default());
    transaction.apply(&Edit::new(0..3, "fn")).unwrap();
    transaction.commit(Default::default());
    syntax.update(&buffer, &theme(), &one(0..buffer.len_bytes()));

    assert_eq!(colored(&syntax, &buffer.text()), ["fn"]);
    assert_eq!(syntax.spans()[0].range, 0..2);
}

#[test]
fn renaming_to_another_extension_switches_the_language_off_and_on() {
    let dir = tempfile::tempdir().unwrap();
    let mut buffer = buffer_in(dir.path(), "a.rs", "fn a() {}\n");
    let mut syntax = DocumentSyntax::default();
    syntax.update(&buffer, &theme(), &one(0..10));
    assert!(!syntax.spans().is_empty());

    buffer.set_path(dir.path().join("a.txt"));
    syntax.update(&buffer, &theme(), &one(0..10));
    assert!(syntax.spans().is_empty());

    buffer.set_path(dir.path().join("a.rs"));
    syntax.update(&buffer, &theme(), &one(0..10));
    assert!(!syntax.spans().is_empty());
}

#[test]
fn updating_with_nothing_changed_keeps_the_same_spans() {
    let dir = tempfile::tempdir().unwrap();
    let buffer = buffer_in(dir.path(), "a.rs", "fn a() {}\n");
    let mut syntax = DocumentSyntax::default();
    syntax.update(&buffer, &theme(), &one(0..10));
    let before = syntax.spans().to_vec();

    syntax.update(&buffer, &theme(), &one(0..10));

    assert_eq!(syntax.spans(), before);
}

#[test]
fn the_spans_follow_when_the_asked_range_moves() {
    let dir = tempfile::tempdir().unwrap();
    let buffer = buffer_in(dir.path(), "a.rs", "fn a() {}\nfn b() {}\n");
    let mut syntax = DocumentSyntax::default();
    syntax.update(&buffer, &theme(), &one(0..9));
    assert_eq!(syntax.spans()[0].range, 0..2);

    syntax.update(&buffer, &theme(), &one(10..19));

    assert_eq!(syntax.spans()[0].range, 10..12);
}
