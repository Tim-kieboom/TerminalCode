use std::fs;

use super::{first_that_exists, written_text};
use crate::components::terminal::LinkText;

#[test]
fn a_relative_path_is_tried_from_each_base_in_turn() {
    let project = tempfile::tempdir().unwrap();
    let shell = tempfile::tempdir().unwrap();
    fs::write(shell.path().join("here.rs"), "").unwrap();
    fs::write(project.path().join("both.rs"), "").unwrap();
    fs::write(shell.path().join("both.rs"), "").unwrap();
    let bases = || [project.path().to_path_buf(), shell.path().to_path_buf()];

    assert_eq!(
        first_that_exists("here.rs", bases()),
        Some(shell.path().join("here.rs")),
        "only the shell's folder has it"
    );
    assert_eq!(
        first_that_exists("both.rs", bases()),
        Some(project.path().join("both.rs")),
        "the project comes first"
    );
    assert_eq!(first_that_exists("none.rs", bases()), None);
}

#[test]
fn a_whole_path_ignores_the_bases() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.rs");
    fs::write(&file, "").unwrap();

    assert_eq!(
        first_that_exists(&file.display().to_string(), []),
        Some(file)
    );
}

#[test]
fn a_link_is_written_back_with_its_line_and_column() {
    let link = |line, column| LinkText {
        path: "src/a.rs".to_owned(),
        line,
        column,
        start: 0,
        end: 0,
    };

    assert_eq!(written_text(&link(Some(3), Some(5))), "src/a.rs:3:5");
    assert_eq!(written_text(&link(Some(3), None)), "src/a.rs:3");
    assert_eq!(written_text(&link(None, None)), "src/a.rs");
}
