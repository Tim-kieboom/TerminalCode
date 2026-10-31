#[cfg(unix)]
use std::time::{Duration, Instant};

use crate::components::terminal::links::find_link;
#[cfg(unix)]
use crate::components::terminal::{Cell, TerminalPane};
#[cfg(unix)]
use crate::pty::{Session, Shell, SpawnConfig};

fn found(text: &str, at: usize) -> Option<(String, Option<usize>, Option<usize>)> {
    let chars: Vec<char> = text.chars().collect();
    find_link(&chars, at).map(|link| (link.path, link.line, link.column))
}

fn path(text: &str, at: usize) -> Option<String> {
    found(text, at).map(|(path, _, _)| path)
}

#[test]
fn a_path_with_a_line_and_a_column_is_cut_out_of_a_compiler_message() {
    let text = "  --> src/lib.rs:3:9";

    assert_eq!(
        found(text, 10),
        Some(("src/lib.rs".to_owned(), Some(3), Some(9)))
    );
}

#[test]
fn grep_and_stack_traces_name_a_line_after_the_file() {
    assert_eq!(
        found("src/main.rs:12:    let x = 1;", 3),
        Some(("src/main.rs".to_owned(), Some(12), None))
    );
    assert_eq!(
        found("at app.py:40", 4),
        Some(("app.py".to_owned(), Some(40), None))
    );
}

#[test]
fn a_path_without_a_place_is_a_path() {
    assert_eq!(
        found("see crates/dap/src/lib.rs now", 8),
        Some(("crates/dap/src/lib.rs".to_owned(), None, None))
    );
    assert_eq!(
        found("open Cargo.toml", 7),
        Some(("Cargo.toml".to_owned(), None, None))
    );
}

#[test]
fn an_absolute_path_and_a_windows_path_keep_their_drive_and_root() {
    assert_eq!(
        found("/code/src/main.rs:12", 5),
        Some(("/code/src/main.rs".to_owned(), Some(12), None))
    );
    assert_eq!(
        found(r"C:\code\src\main.rs:12:5", 4),
        Some((r"C:\code\src\main.rs".to_owned(), Some(12), Some(5)))
    );
}

#[test]
fn punctuation_and_brackets_around_a_path_are_not_part_of_it() {
    assert_eq!(
        path("error in src/main.rs.", 12),
        Some("src/main.rs".to_owned())
    );
    assert_eq!(path("(src/main.rs:12)", 3), Some("src/main.rs".to_owned()));
    assert_eq!(path("\"src/main.rs\",", 3), Some("src/main.rs".to_owned()));
    assert_eq!(path("src/main.rs:12:", 3), Some("src/main.rs".to_owned()));
}

#[test]
fn a_url_a_plain_word_and_a_number_are_not_paths() {
    assert_eq!(path("see https://example.com/a/b.html", 10), None);
    assert_eq!(path("compiling trcode now", 5), None);
    assert_eq!(path("version 0.9.5 is out", 9), None);
    assert_eq!(path("   ", 1), None);
}

#[test]
fn only_the_characters_of_the_path_are_a_link() {
    // On the arrow and on the space before the path.
    assert_eq!(path("  --> src/lib.rs:3:9", 3), None);
    assert_eq!(path("  --> src/lib.rs:3:9", 5), None);
    assert_eq!(path("src/lib.rs:3:9 and more", 17), None);
}

/// A pane with a shell that shows `text` on a screen 40 columns wide.
#[cfg(unix)]
fn pane_showing(text: &str) -> TerminalPane {
    let session = Session::spawn(
        &SpawnConfig {
            shell: Shell::script(&format!("printf '{text}'; sleep 30")),
            directory: std::env::temp_dir(),
            env: Vec::new(),
            rows: 10,
            columns: 40,
        },
        || {},
    )
    .unwrap();
    let mut pane = TerminalPane::default();
    pane.start(session, "sh".to_owned());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !pane
        .session()
        .unwrap()
        .with_screen(|screen| screen.contents().contains(text.get(..5).unwrap_or(text)))
    {
        assert!(Instant::now() < deadline, "the shell showed nothing");
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(100));
    pane
}

#[test]
#[cfg(unix)]
fn a_path_on_the_screen_is_found_with_the_cells_it_covers() {
    let pane = pane_showing("see src/main.rs:12 here");

    let link = pane.link_at(Cell { row: 0, column: 6 }).unwrap();

    assert_eq!(link.text.path, "src/main.rs");
    assert_eq!(link.text.line, Some(12));
    assert_eq!(link.cells.first(), Some(&Cell { row: 0, column: 4 }));
    assert_eq!(link.cells.last(), Some(&Cell { row: 0, column: 17 }));
    assert!(pane.link_at(Cell { row: 0, column: 2 }).is_none());
    assert!(pane.link_at(Cell { row: 0, column: 30 }).is_none());
}

#[test]
#[cfg(unix)]
fn a_path_that_wrapped_onto_the_next_row_is_one_link() {
    // 50 characters on a screen of 40 columns: the end is on the second row.
    let name = "crates/some/very/long/folder/name/lib_x.rs:7";
    let pane = pane_showing(name);

    let on_the_first_row = pane.link_at(Cell { row: 0, column: 3 }).unwrap();
    let on_the_second_row = pane.link_at(Cell { row: 1, column: 1 }).unwrap();

    assert_eq!(
        on_the_first_row.text.path,
        "crates/some/very/long/folder/name/lib_x.rs"
    );
    assert_eq!(on_the_first_row.text.line, Some(7));
    assert_eq!(on_the_second_row.text, on_the_first_row.text);
    assert!(on_the_first_row.cells.iter().any(|cell| cell.row == 1));
}

#[test]
#[cfg(unix)]
fn rows_that_ended_with_a_line_feed_are_not_joined() {
    let pane = pane_showing("src/a.rs:1\\nsrc/b.rs:2");

    let first = pane.link_at(Cell { row: 0, column: 2 }).unwrap();
    let second = pane.link_at(Cell { row: 1, column: 2 }).unwrap();

    assert_eq!(first.text.path, "src/a.rs");
    assert_eq!(second.text.path, "src/b.rs");
}
