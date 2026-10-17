use std::path::PathBuf;

use crate::components::popups::finder::{Finder, Walk};

fn finder() -> Finder {
    Finder::new(PathBuf::from("/project"), 1, None)
}

fn batch(finder: &mut Finder, paths: &[&str]) {
    finder.add_batch(1, paths.iter().map(PathBuf::from).collect());
}

fn type_str(finder: &mut Finder, text: &str) {
    text.chars().for_each(|c| finder.insert(c));
}

#[test]
fn an_empty_query_lists_the_files_shortest_path_first() {
    let mut finder = finder();

    batch(
        &mut finder,
        &["src/components/long_name.rs", "a.rs", "src/b.rs"],
    );

    assert_eq!(
        finder.results(),
        ["a.rs", "src/b.rs", "src/components/long_name.rs"]
    );
}

#[test]
fn typing_narrows_the_list_to_files_that_match() {
    let mut finder = finder();
    batch(
        &mut finder,
        &["src/main.rs", "src/lib.rs", "docs/design.md"],
    );

    type_str(&mut finder, "lib");

    assert_eq!(finder.results(), ["src/lib.rs"]);
    type_str(&mut finder, "x");
    assert!(finder.results().is_empty());
}

#[test]
fn a_match_in_the_file_name_beats_a_scattered_match_in_the_path() {
    let mut finder = finder();
    batch(&mut finder, &["alpha/mapping_index.rs", "src/main.rs"]);

    type_str(&mut finder, "main");

    assert_eq!(finder.results()[0], "src/main.rs");
}

#[test]
fn files_that_arrive_after_the_query_was_typed_are_ranked_in_at_once() {
    let mut finder = finder();
    batch(&mut finder, &["docs/readme_mentions_main.md"]);
    type_str(&mut finder, "main");
    assert_eq!(finder.results(), ["docs/readme_mentions_main.md"]);

    batch(&mut finder, &["src/main.rs", "unrelated.txt"]);

    assert_eq!(finder.results()[0], "src/main.rs");
    assert_eq!(
        finder.results().len(),
        2,
        "the unrelated file does not match"
    );
    assert_eq!(finder.file_count(), 3);
}

#[test]
fn a_batch_from_another_walk_is_ignored() {
    let mut finder = finder();

    finder.add_batch(2, vec![PathBuf::from("stale.rs")]);
    finder.finish(2, 0);

    assert_eq!(finder.file_count(), 0);
    assert_eq!(finder.walk(), Walk::Running);
}

#[test]
fn finishing_the_walk_is_recorded() {
    let mut finder = finder();

    finder.finish(1, 3);

    assert_eq!(finder.walk(), Walk::Done { unreadable: 3 });
}

#[test]
fn the_selection_stays_on_its_file_while_better_files_arrive_after_it() {
    let mut finder = finder();
    batch(&mut finder, &["a/one.rs", "bb/two.rs"]);
    finder.select_next();
    assert_eq!(
        finder.selected_path(),
        Some(PathBuf::from("/project/bb/two.rs"))
    );

    batch(&mut finder, &["z.rs"]);

    assert_eq!(
        finder.selected_path(),
        Some(PathBuf::from("/project/bb/two.rs"))
    );
}

#[test]
fn changing_the_query_goes_back_to_the_top() {
    let mut finder = finder();
    batch(&mut finder, &["a.rs", "ab.rs", "abc.rs"]);
    finder.select_next();
    finder.select_next();

    finder.insert('a');

    assert_eq!(finder.selected(), 0);
}

#[test]
fn the_selection_stops_at_both_ends() {
    let mut finder = finder();
    batch(&mut finder, &["a.rs", "b.rs"]);

    finder.select_previous();
    assert_eq!(finder.selected(), 0);
    finder.select_next();
    finder.select_next();
    assert_eq!(finder.selected(), 1);
}

#[test]
fn nothing_is_selected_when_nothing_matches() {
    let mut finder = finder();
    batch(&mut finder, &["a.rs"]);

    type_str(&mut finder, "zzz");
    finder.select_next();

    assert_eq!(finder.selected_path(), None);
}

#[test]
fn only_the_best_matches_are_kept_when_there_are_very_many() {
    let mut finder = finder();
    let many: Vec<String> = (0..1000)
        .map(|n| format!("generated/file_{n:04}.txt"))
        .collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    batch(&mut finder, &many);
    batch(&mut finder, &["tiny.txt"]);

    assert_eq!(finder.results().len(), 200);
    assert_eq!(finder.results()[0], "tiny.txt");

    type_str(&mut finder, "file_0999");
    assert_eq!(finder.results()[0], "generated/file_0999.txt");
    assert_eq!(finder.file_count(), 1001);
}

#[cfg(windows)]
#[test]
fn windows_separators_are_shown_as_slashes() {
    let mut finder = finder();

    batch(&mut finder, &["src\\main.rs"]);

    assert_eq!(finder.results(), ["src/main.rs"]);
}

#[cfg(unix)]
mod unix_names {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    use super::{batch, finder, type_str};

    #[test]
    fn a_backslash_in_a_file_name_is_part_of_the_name_not_a_separator() {
        let mut finder = finder();

        batch(&mut finder, &["src/we\\ird.txt"]);

        assert_eq!(finder.results(), ["src/we\\ird.txt"]);
        assert_eq!(
            finder.selected_path(),
            Some(PathBuf::from("/project/src/we\\ird.txt")),
            "the file that exists, not one in a directory called we"
        );
    }

    #[test]
    fn a_name_that_is_not_valid_unicode_is_still_opened_by_its_real_path() {
        let mut finder = finder();
        let name = PathBuf::from(OsString::from_vec(b"caf\xe9.txt".to_vec()));

        finder.add_batch(1, vec![name.clone()]);

        assert_eq!(finder.results(), ["caf\u{FFFD}.txt"]);
        assert_eq!(
            finder.selected_path(),
            Some(PathBuf::from("/project").join(name))
        );
    }

    #[test]
    fn a_name_that_is_not_valid_unicode_can_be_found_by_the_part_that_is() {
        let mut finder = finder();
        let name = PathBuf::from(OsString::from_vec(b"docs/caf\xe9.txt".to_vec()));
        finder.add_batch(1, vec![name, PathBuf::from("src/main.rs")]);

        type_str(&mut finder, "caf");

        assert_eq!(finder.results().len(), 1);
        assert!(finder.results()[0].starts_with("docs/caf"));
    }
}

#[test]
fn the_names_of_a_nested_path_are_shown_with_slashes_and_the_last_is_the_file_name() {
    let mut finder = finder();

    batch(&mut finder, &["a/b/c.rs"]);
    type_str(&mut finder, "c.rs");

    assert_eq!(finder.results(), ["a/b/c.rs"]);
    assert_eq!(
        finder.selected_path(),
        Some(PathBuf::from("/project").join("a/b/c.rs"))
    );
}

#[test]
fn the_query_can_be_edited_and_cleared() {
    let mut finder = finder();
    batch(&mut finder, &["a.rs", "b.rs"]);
    type_str(&mut finder, "ab");
    assert!(finder.results().is_empty());

    finder.backspace();
    assert_eq!(finder.results(), ["a.rs"]);

    finder.clear_query();
    assert_eq!(finder.query(), "");
    assert_eq!(finder.results().len(), 2);
}

#[test]
fn growing_a_query_gives_the_same_results_as_typing_it_fresh() {
    let paths = [
        "src/main.rs",
        "src/lib.rs",
        "src/maintenance/mod.rs",
        "docs/MAIN_NOTES.md",
        "a/b/c/main_test.rs",
        "unrelated.txt",
    ];
    let mut grown = finder();
    batch(&mut grown, &paths);
    type_str(&mut grown, "main");

    let mut fresh = finder();
    fresh.insert('m');
    batch(&mut fresh, &paths);
    fresh.backspace();
    type_str(&mut fresh, "main");

    assert_eq!(grown.results(), fresh.results());
    assert_eq!(grown.results().len(), 4);
}

#[test]
fn backspacing_brings_back_files_the_longer_query_had_excluded() {
    let mut finder = finder();
    batch(&mut finder, &["main.rs", "mod.rs"]);
    type_str(&mut finder, "main");
    assert_eq!(finder.results(), ["main.rs"]);

    finder.backspace();
    finder.backspace();
    finder.backspace();

    assert_eq!(finder.results().len(), 2);
}

#[test]
fn files_arriving_after_a_longer_query_are_candidates_for_later_edits_too() {
    let mut finder = finder();
    batch(&mut finder, &["main.rs"]);
    type_str(&mut finder, "ma");
    batch(&mut finder, &["map.rs"]);

    type_str(&mut finder, "p");

    assert_eq!(finder.results(), ["map.rs"]);
}

// ---- recently opened files

fn finder_with_recent(recent: &[&str]) -> Finder {
    let recent: Vec<PathBuf> = recent.iter().map(PathBuf::from).collect();
    finder().with_recent(&recent)
}

#[test]
fn recently_opened_files_come_first_while_nothing_is_typed() {
    let mut finder = finder_with_recent(&["/project/src/c.rs", "/project/src/b.rs"]);

    batch(&mut finder, &["a.rs", "src/b.rs", "src/c.rs", "src/d.rs"]);

    assert_eq!(
        finder.results(),
        ["src/c.rs", "src/b.rs", "a.rs", "src/d.rs"]
    );
}

#[test]
fn a_recent_file_that_arrives_in_a_later_batch_is_still_first() {
    let mut finder = finder_with_recent(&["/project/src/late.rs"]);
    batch(&mut finder, &["a.rs", "b.rs"]);

    batch(&mut finder, &["src/late.rs"]);

    assert_eq!(finder.results()[0], "src/late.rs");
}

#[test]
fn among_equal_matches_the_recent_one_wins() {
    let mut finder = finder_with_recent(&["/project/two/util.rs"]);
    batch(&mut finder, &["one/util.rs", "two/util.rs"]);

    type_str(&mut finder, "util");

    assert_eq!(finder.results(), ["two/util.rs", "one/util.rs"]);
}

#[test]
fn a_much_better_match_still_beats_a_recent_file() {
    let mut finder = finder_with_recent(&["/project/old/domain/mapping_index.rs"]);
    batch(&mut finder, &["old/domain/mapping_index.rs", "src/main.rs"]);

    type_str(&mut finder, "main");

    assert_eq!(finder.results()[0], "src/main.rs");
}

#[test]
fn files_outside_the_project_are_ignored_as_recent() {
    let mut finder = finder_with_recent(&["/elsewhere/a.rs"]);

    batch(&mut finder, &["a.rs", "b.rs"]);

    assert_eq!(finder.results(), ["a.rs", "b.rs"]);
}

// ---- a place in the file

#[test]
fn a_line_after_the_name_does_not_change_which_files_match() {
    let mut finder = finder();
    batch(&mut finder, &["src/main.rs", "src/lib.rs"]);

    type_str(&mut finder, "main.rs:42");

    assert_eq!(finder.results(), ["src/main.rs"]);
}

#[test]
fn typing_the_colon_and_digits_keeps_the_match() {
    let mut finder = finder();
    batch(&mut finder, &["src/main.rs", "src/lib.rs"]);

    for (typed, expected) in [("main", 1), (".rs", 1), (":", 1), ("4", 1), ("2", 1)] {
        type_str(&mut finder, typed);
        assert_eq!(finder.results().len(), expected, "after {typed:?}");
    }
    assert_eq!(finder.selected_target().unwrap().line, Some(42));
}

#[test]
fn deleting_back_through_the_place_finds_the_files_again() {
    let mut finder = finder();
    batch(&mut finder, &["src/main.rs", "src/lib.rs"]);
    type_str(&mut finder, "main.rs:42");

    for _ in 0..3 {
        finder.backspace();
    }

    assert_eq!(finder.results(), ["src/main.rs"]);
    assert_eq!(finder.selected_target().unwrap().line, None);
    finder.clear_query();
    assert_eq!(finder.results().len(), 2);
}

#[test]
fn the_target_has_the_path_the_line_and_the_column() {
    let mut finder = finder();
    batch(&mut finder, &["src/main.rs"]);
    type_str(&mut finder, "main:7:3");

    let target = finder.selected_target().unwrap();

    assert_eq!(target.path, PathBuf::from("/project/src/main.rs"));
    assert_eq!((target.line, target.column), (Some(7), Some(3)));
}

#[test]
fn without_a_place_the_target_is_just_the_file() {
    let mut finder = finder();
    batch(&mut finder, &["src/main.rs"]);
    type_str(&mut finder, "main");

    let target = finder.selected_target().unwrap();

    assert_eq!((target.line, target.column), (None, None));
}

#[test]
fn with_no_match_there_is_no_target() {
    let mut finder = finder();
    batch(&mut finder, &["src/main.rs"]);

    type_str(&mut finder, "nothing:3");

    assert!(finder.selected_target().is_none());
}
