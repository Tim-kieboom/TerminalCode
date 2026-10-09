use std::path::PathBuf;

use crate::components::finder::{Finder, Walk};

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

#[test]
fn backslashes_from_windows_paths_become_slashes() {
    let mut finder = finder();

    batch(&mut finder, &["src\\main.rs"]);

    assert_eq!(finder.results(), ["src/main.rs"]);
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
