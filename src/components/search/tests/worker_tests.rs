use std::fs;
use std::path::Path;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::components::search::worker::{MAX_HITS, SearchHandle};
use crate::components::search::{Hit, Options, start_search};
use crate::event::Event;

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

struct Outcome {
    hits: Vec<Hit>,
    files: usize,
    truncated: bool,
}

fn run(root: &Path, query: &str, options: Options) -> Outcome {
    let (tx, mut rx) = mpsc::channel(64);
    let _handle: SearchHandle = start_search(root.to_path_buf(), 3, query, options, tx).unwrap();
    let mut hits = Vec::new();
    while let Some(event) = rx.blocking_recv() {
        match event {
            Event::SearchBatch {
                search,
                hits: found,
            } => {
                assert_eq!(search, 3);
                hits.extend(found);
            }
            Event::SearchDone {
                search,
                files,
                truncated,
            } => {
                assert_eq!(search, 3);
                return Outcome {
                    hits,
                    files,
                    truncated,
                };
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
    panic!("the search ended without saying so");
}

fn lines(outcome: &Outcome) -> Vec<String> {
    outcome
        .hits
        .iter()
        .map(|hit| format!("{}:{}", hit.path.display(), hit.line + 1))
        .collect()
}

#[test]
fn a_literal_search_finds_lines_with_their_place() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "one\n  needle two\nthree\n");
    write(dir.path(), "src/b.txt", "needle\nnothing\nanother needle\n");

    let outcome = run(dir.path(), "needle", Options::default());

    assert_eq!(lines(&outcome), ["a.txt:2", "src/b.txt:1", "src/b.txt:3"]);
    assert_eq!(outcome.files, 2);
    assert!(!outcome.truncated);
    assert_eq!(outcome.hits[0].column, 2);
    assert_eq!(outcome.hits[0].text, "needle two");
    assert_eq!(outcome.hits[2].column, 8);
}

#[test]
fn results_come_in_path_order() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["zeta.txt", "alpha.txt", "m/inner.txt", "beta.txt"] {
        write(dir.path(), name, "needle\n");
    }

    let outcome = run(dir.path(), "needle", Options::default());

    assert_eq!(
        lines(&outcome),
        ["alpha.txt:1", "beta.txt:1", "m/inner.txt:1", "zeta.txt:1"]
    );
}

#[test]
fn a_literal_query_takes_regex_characters_literally() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "abc\na.c\nfn(x)\n");

    let dot = run(dir.path(), "a.c", Options::default());
    let paren = run(dir.path(), "fn(", Options::default());

    assert_eq!(lines(&dot), ["a.txt:2"]);
    assert_eq!(lines(&paren), ["a.txt:3"]);
}

#[test]
fn with_regex_on_the_query_is_a_pattern() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "abc\na.c\nxyz\n");
    let options = Options {
        regex: true,
        ..Options::default()
    };

    let outcome = run(dir.path(), "a.c", options);

    assert_eq!(lines(&outcome), ["a.txt:1", "a.txt:2"]);
    let digits = run(dir.path(), "^[a-c]+$", options);
    assert_eq!(lines(&digits), ["a.txt:1"]);
}

#[test]
fn case_is_ignored_unless_match_case_is_on() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "Hello\nhello\nHELLO\n");

    let loose = run(dir.path(), "hello", Options::default());
    let exact = run(
        dir.path(),
        "hello",
        Options {
            case_sensitive: true,
            ..Options::default()
        },
    );

    assert_eq!(loose.hits.len(), 3);
    assert_eq!(lines(&exact), ["a.txt:2"]);
}

#[test]
fn an_invalid_pattern_is_an_error_with_a_one_line_reason() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = mpsc::channel(4);

    let error = start_search(
        dir.path().to_path_buf(),
        1,
        "(unclosed",
        Options {
            regex: true,
            ..Options::default()
        },
        tx,
    )
    .unwrap_err()
    .to_string();

    assert!(!error.is_empty());
    assert!(!error.contains('\n'), "{error:?}");
}

#[test]
fn ignored_files_git_internals_and_binary_files_are_skipped_but_hidden_files_are_searched() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".gitignore", "target/\n*.log\n");
    write(dir.path(), "kept.txt", "needle\n");
    write(dir.path(), ".hidden", "needle\n");
    write(dir.path(), "build.log", "needle\n");
    write(dir.path(), "target/out.txt", "needle\n");
    write(dir.path(), ".git/config", "needle\n");
    fs::write(dir.path().join("binary.bin"), b"needle\0\x01\x02").unwrap();

    let outcome = run(dir.path(), "needle", Options::default());

    assert_eq!(lines(&outcome), [".hidden:1", "kept.txt:1"]);
}

#[test]
fn a_line_with_several_matches_is_one_hit_at_the_first() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "x needle needle needle\n");

    let outcome = run(dir.path(), "needle", Options::default());

    assert_eq!(outcome.hits.len(), 1);
    assert_eq!(outcome.hits[0].column, 2);
}

#[test]
fn windows_line_endings_and_invalid_utf8_do_not_break_the_hit() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "crlf.txt", "first\r\nneedle here\r\n");
    fs::write(dir.path().join("latin.txt"), b"caf\xe9 needle \xff end\n").unwrap();

    let outcome = run(dir.path(), "needle", Options::default());

    assert_eq!(lines(&outcome), ["crlf.txt:2", "latin.txt:1"]);
    assert_eq!(outcome.hits[0].text, "needle here");
    let latin = &outcome.hits[1];
    assert_eq!(&latin.text[latin.matched.clone()], "needle");
    assert_eq!(latin.column, 5, "caf, the replacement for é, and a space");
}

#[test]
fn one_file_cannot_use_up_the_whole_budget() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "log.txt", &"needle\n".repeat(1000));
    write(dir.path(), "other.txt", "needle\n");

    let outcome = run(dir.path(), "needle", Options::default());

    let from_log = outcome
        .hits
        .iter()
        .filter(|hit| hit.path.ends_with("log.txt"))
        .count();
    assert_eq!(from_log, 200);
    assert_eq!(outcome.files, 2);
}

#[test]
fn a_search_with_too_many_results_stops_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    for number in 0..60 {
        write(
            dir.path(),
            &format!("f{number:02}.txt"),
            &"needle\n".repeat(150),
        );
    }

    let outcome = run(dir.path(), "needle", Options::default());

    assert!(outcome.truncated);
    assert!(outcome.hits.len() >= MAX_HITS);
    assert!(outcome.hits.len() < MAX_HITS + 200);
}

#[test]
fn dropping_the_handle_before_the_search_starts_cancels_it() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "needle\n");
    let (tx, mut rx) = mpsc::channel(8);

    let handle = start_search(
        dir.path().to_path_buf(),
        1,
        "needle",
        Options::default(),
        tx,
    )
    .unwrap();
    drop(handle);
    std::thread::sleep(Duration::from_millis(500));

    assert!(rx.try_recv().is_err(), "a cancelled search sends nothing");
}

#[test]
fn an_empty_project_still_finishes() {
    let dir = tempfile::tempdir().unwrap();

    let outcome = run(dir.path(), "needle", Options::default());

    assert!(outcome.hits.is_empty());
    assert_eq!(outcome.files, 0);
}

#[test]
fn a_regex_anchored_at_the_end_of_the_line_matches_in_a_file_with_windows_line_endings() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "crlf.txt", "foo;\r\nbar\r\nbaz; \r\n");
    write(dir.path(), "lf.txt", "foo;\nbar\n");
    let options = Options {
        regex: true,
        ..Options::default()
    };

    let outcome = run(dir.path(), ";$", options);

    assert_eq!(lines(&outcome), ["crlf.txt:1", "lf.txt:1"]);
}

#[test]
fn a_regex_anchored_at_the_start_of_the_line_matches_after_a_windows_line_ending() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "crlf.txt", "foo\r\nbar\r\n");
    let options = Options {
        regex: true,
        ..Options::default()
    };

    let outcome = run(dir.path(), "^bar$", options);

    assert_eq!(lines(&outcome), ["crlf.txt:2"]);
    assert_eq!(outcome.hits[0].text, "bar");
}
