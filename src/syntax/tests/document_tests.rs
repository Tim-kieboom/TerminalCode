use std::fs;
use std::ops::Range;
use std::path::Path;

use ratatui::style::Color;

use crate::buffer::{Buffer, Edit};
use crate::syntax::DocumentSyntax;
use crate::syntax::document::Parses;
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

/// Applies a replacement of `range` with `text` and tells `syntax` about it,
/// as the workspace does.
fn edit(buffer: &mut Buffer, syntax: &mut DocumentSyntax, range: Range<usize>, text: &str) {
    let mut transaction = buffer.begin_transaction(Default::default());
    transaction.apply(&Edit::new(range, text)).unwrap();
    transaction.commit(Default::default());
    syntax.record_edits(&buffer.take_edit_log());
}

fn whole(buffer: &Buffer) -> Vec<Range<usize>> {
    one(0..buffer.len_bytes())
}

#[test]
fn edits_that_were_recorded_reparse_from_the_old_tree() {
    let dir = tempfile::tempdir().unwrap();
    let mut buffer = buffer_in(dir.path(), "a.rs", "fn a() {}\nlet b = 1;\n");
    let mut syntax = DocumentSyntax::default();
    syntax.update(&buffer, &theme(), &whole(&buffer));
    assert_eq!(
        syntax.parses(),
        Parses {
            full: 1,
            incremental: 0
        }
    );

    edit(&mut buffer, &mut syntax, 0..2, "pub fn");
    syntax.update(&buffer, &theme(), &whole(&buffer));

    assert_eq!(
        syntax.parses(),
        Parses {
            full: 1,
            incremental: 1
        }
    );
    assert_eq!(colored(&syntax, &buffer.text()), ["pub", "fn", "let"]);
}

#[test]
fn several_edits_between_two_updates_are_replayed_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut buffer = buffer_in(dir.path(), "a.rs", "fn a() {}\n");
    let mut syntax = DocumentSyntax::default();
    syntax.update(&buffer, &theme(), &whole(&buffer));

    edit(&mut buffer, &mut syntax, 9..9, "\nlet x");
    edit(&mut buffer, &mut syntax, 15..15, " = 1;");
    edit(&mut buffer, &mut syntax, 0..0, "pub ");
    syntax.update(&buffer, &theme(), &whole(&buffer));

    assert_eq!(syntax.parses().incremental, 1);
    assert_eq!(syntax.parses().full, 1);
    assert_eq!(colored(&syntax, &buffer.text()), ["pub", "fn", "let"]);
}

#[test]
fn a_change_nobody_told_the_syntax_about_is_parsed_in_full() {
    let dir = tempfile::tempdir().unwrap();
    let mut buffer = buffer_in(dir.path(), "a.rs", "fn a() {}\n");
    let mut syntax = DocumentSyntax::default();
    syntax.update(&buffer, &theme(), &whole(&buffer));

    // Changed without `record_edits`, like a reload from disk.
    let mut transaction = buffer.begin_transaction(Default::default());
    transaction.apply(&Edit::new(0..2, "let")).unwrap();
    transaction.commit(Default::default());
    buffer.take_edit_log();
    syntax.update(&buffer, &theme(), &whole(&buffer));

    assert_eq!(
        syntax.parses(),
        Parses {
            full: 2,
            incremental: 0
        }
    );
    assert_eq!(colored(&syntax, &buffer.text()), ["let"]);
}

#[test]
fn the_spans_follow_an_edit_before_the_next_update() {
    let dir = tempfile::tempdir().unwrap();
    let mut buffer = buffer_in(dir.path(), "a.rs", "fn a() {}\n");
    let mut syntax = DocumentSyntax::default();
    syntax.update(&buffer, &theme(), &whole(&buffer));
    assert_eq!(syntax.spans()[0].range, 0..2);

    edit(&mut buffer, &mut syntax, 0..0, "    ");

    assert_eq!(syntax.spans()[0].range, 4..6, "shifted without a reparse");
}

#[test]
fn an_edit_recorded_before_anything_was_parsed_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let mut buffer = buffer_in(dir.path(), "a.rs", "fn a() {}\n");
    let mut syntax = DocumentSyntax::default();

    edit(&mut buffer, &mut syntax, 0..0, "pub ");
    syntax.update(&buffer, &theme(), &whole(&buffer));

    assert_eq!(
        syntax.parses(),
        Parses {
            full: 1,
            incremental: 0
        }
    );
    assert_eq!(colored(&syntax, &buffer.text()), ["pub", "fn"]);
}

#[test]
fn too_many_pending_edits_fall_back_to_a_full_parse() {
    let dir = tempfile::tempdir().unwrap();
    let mut buffer = buffer_in(dir.path(), "a.rs", "fn a() {}\n");
    let mut syntax = DocumentSyntax::default();
    syntax.update(&buffer, &theme(), &whole(&buffer));

    edit(&mut buffer, &mut syntax, 0..0, " ");
    let mut transaction = buffer.begin_transaction(Default::default());
    transaction.apply(&Edit::new(0..0, " ")).unwrap();
    transaction.commit(Default::default());
    let info = buffer.take_edit_log()[0];
    // Far more edits than the syntax keeps.
    syntax.record_edits(&vec![info; 10_001]);
    syntax.update(&buffer, &theme(), &whole(&buffer));

    assert_eq!(
        syntax.parses(),
        Parses {
            full: 2,
            incremental: 0
        }
    );
    assert_eq!(colored(&syntax, &buffer.text()), ["fn"]);
}

/// Every step of an edit sequence gives the spans a parse from scratch gives.
fn assert_incremental_matches_fresh(start: &str, steps: &[(Range<usize>, &str)]) {
    let dir = tempfile::tempdir().unwrap();
    let mut buffer = buffer_in(dir.path(), "a.rs", start);
    let mut incremental = DocumentSyntax::default();
    let theme = Theme::default();
    incremental.update(&buffer, &theme, &whole(&buffer));
    for (range, text) in steps {
        edit(&mut buffer, &mut incremental, range.clone(), text);
        incremental.update(&buffer, &theme, &whole(&buffer));

        let mut fresh = DocumentSyntax::default();
        fresh.update(&buffer, &theme, &whole(&buffer));
        assert_eq!(
            incremental.spans(),
            fresh.spans(),
            "after replacing {range:?} with {text:?}: {:?}",
            buffer.text()
        );
    }
    assert!(incremental.parses().incremental > 0);
}

#[test]
fn typing_a_function_one_key_at_a_time_matches_a_fresh_parse() {
    let source = "fn f(a: u8) -> u8 { a } // x\n";
    let steps: Vec<_> = source
        .char_indices()
        .map(|(at, c)| (at..at, &source[at..at + c.len_utf8()]))
        .collect();

    assert_incremental_matches_fresh("", &steps);
}

#[test]
fn deleting_and_replacing_matches_a_fresh_parse() {
    let start = "use std::fmt;\nfn a() { let s = \"x\"; }\nstruct P { x: u32 }\n";
    assert_incremental_matches_fresh(
        start,
        &[
            (0..4, "pub use"),
            (20..22, "async fn"),
            (30..40, ""),
            (0..0, "// header\n"),
            (5..30, ""),
        ],
    );
}
