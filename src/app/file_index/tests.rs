use std::path::PathBuf;

use super::{FileIndex, normalized, unique_with_ending};

fn files(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_name_is_found_when_one_file_ends_with_it() {
    let all = files(&["src/main.rs", "src/app/mod.rs", "docs/a.md"]);

    assert_eq!(
        unique_with_ending(&all, "main.rs").as_deref(),
        Some("src/main.rs")
    );
    assert_eq!(
        unique_with_ending(&all, "app/mod.rs").as_deref(),
        Some("src/app/mod.rs")
    );
    assert_eq!(
        unique_with_ending(&all, "src/main.rs").as_deref(),
        Some("src/main.rs")
    );
}

#[test]
fn two_files_with_that_ending_are_none_found() {
    let all = files(&["a/mod.rs", "b/mod.rs"]);

    assert_eq!(unique_with_ending(&all, "mod.rs"), None);
    assert_eq!(
        unique_with_ending(&all, "a/mod.rs").as_deref(),
        Some("a/mod.rs")
    );
}

#[test]
fn an_ending_is_whole_parts_not_the_end_of_a_name() {
    let all = files(&["src/domain.rs", "src/main.rs"]);

    assert_eq!(
        unique_with_ending(&all, "main.rs").as_deref(),
        Some("src/main.rs")
    );
    assert_eq!(
        unique_with_ending(&all, "ain.rs"),
        None,
        "not part of a name"
    );
    assert_eq!(unique_with_ending(&all, "omain.rs"), None);
}

#[test]
fn a_file_that_is_the_whole_path_wins_over_files_that_only_end_with_it() {
    let all = files(&["Cargo.toml", "crates/a/Cargo.toml"]);

    assert_eq!(
        unique_with_ending(&all, "Cargo.toml").as_deref(),
        Some("Cargo.toml")
    );
}

#[test]
fn text_is_made_into_the_form_the_list_has() {
    assert_eq!(normalized("./src/a.rs").as_deref(), Some("src/a.rs"));
    assert_eq!(normalized(r"src\a.rs").as_deref(), Some("src/a.rs"));
    assert_eq!(
        normalized("/abs/a.rs"),
        None,
        "a whole path is not an ending"
    );
    assert_eq!(normalized("../a.rs"), None);
    assert_eq!(normalized("a/../b.rs"), None);
    assert_eq!(normalized(r"C:\x\a.rs"), None, "a drive");
    assert_eq!(normalized(""), None);
}

#[test]
fn the_index_makes_a_whole_path_from_the_project_folder() {
    let index = FileIndex {
        root: Some(PathBuf::from("/p")),
        files: files(&["src/lib.rs"]),
        read_at: None,
        reading: false,
    };

    assert_eq!(
        index.unique_ending("lib.rs"),
        Some(PathBuf::from("/p/src/lib.rs"))
    );
    assert_eq!(index.unique_ending("other.rs"), None);
    assert_eq!(
        FileIndex::default().unique_ending("lib.rs"),
        None,
        "nothing read yet"
    );
}
