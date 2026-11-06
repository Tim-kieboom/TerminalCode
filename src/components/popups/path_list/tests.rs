use std::fs;
use std::time::{Duration, Instant};

use super::{Entry, common_start, matching, read_dir, split_typed};

fn entry(name: &str, is_dir: bool) -> Entry {
    Entry {
        name: name.to_owned(),
        is_dir,
    }
}

fn names(entries: &[Entry], partial: &str, dirs_only: bool) -> Vec<String> {
    matching(entries, partial, dirs_only)
        .0
        .iter()
        .map(|found| entries[found.index].name.clone())
        .collect()
}

#[test]
fn a_typed_path_is_split_at_its_last_separator() {
    assert_eq!(split_typed("src/comp"), ("src/", "comp"));
    assert_eq!(split_typed("src/"), ("src/", ""));
    assert_eq!(split_typed("src"), ("", "src"));
    assert_eq!(split_typed("/etc/ho"), ("/etc/", "ho"));
    assert_eq!(split_typed("~/"), ("~/", ""));
    assert_eq!(split_typed(""), ("", ""));
    assert_eq!(split_typed("../x"), ("../", "x"));
}

#[test]
fn folders_come_before_files_and_each_group_is_in_name_order() {
    let entries = [
        entry("zeta.rs", false),
        entry("beta", true),
        entry("Alpha.rs", false),
        entry("alpha", true),
    ];

    assert_eq!(
        names(&entries, "", false),
        ["alpha", "beta", "Alpha.rs", "zeta.rs"]
    );
}

#[test]
fn a_name_is_matched_by_the_letters_in_order_not_only_by_its_start() {
    let entries = [
        entry("components", true),
        entry("commands.rs", false),
        entry("main.rs", false),
    ];

    assert_eq!(names(&entries, "comp", false), ["components"]);
    assert_eq!(names(&entries, "cmp", false), ["components"]);
    let mut with_rs = names(&entries, "rs", false);
    with_rs.sort();
    assert_eq!(with_rs, ["commands.rs", "main.rs"]);
    assert!(names(&entries, "zzz", false).is_empty());
}

#[test]
fn an_exact_name_comes_first_even_before_a_folder() {
    let entries = [entry("domain", true), entry("main", false)];

    assert_eq!(names(&entries, "main", false), ["main", "domain"]);
}

#[test]
fn names_that_start_with_a_dot_wait_for_a_dot() {
    let entries = [
        entry(".git", true),
        entry(".env", false),
        entry("src", true),
    ];

    assert_eq!(names(&entries, "", false), ["src"]);
    assert_eq!(names(&entries, "g", false), Vec::<String>::new());
    assert_eq!(names(&entries, ".", false), [".git", ".env"]);
    assert_eq!(names(&entries, ".e", false), [".env"]);
}

#[test]
fn a_folder_prompt_leaves_files_out() {
    let entries = [entry("src", true), entry("src.rs", false)];

    assert_eq!(names(&entries, "src", true), ["src"]);
}

#[test]
fn the_match_says_which_letters_it_used() {
    let entries = [entry("components", true)];

    let (found, total) = matching(&entries, "cmp", false);

    assert_eq!(total, 1);
    assert_eq!(found[0].positions, [0, 2, 3]);
}

#[test]
fn tab_completes_what_the_names_that_start_alike_share() {
    let names = ["components", "command", "other"];

    assert_eq!(common_start(&names, "co"), "com");
    assert_eq!(
        common_start(&names, "COM"),
        "COM",
        "what was typed is kept as it is"
    );
    assert_eq!(common_start(&names, "CO"), "com");
    assert_eq!(common_start(&names, "comp"), "components");
    assert_eq!(common_start(&names, "x"), "x", "nothing starts with it");
    assert_eq!(
        common_start(&names, ""),
        "",
        "they share nothing from the start"
    );
}

#[test]
fn a_folder_is_read_with_what_each_entry_is() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("a.txt"), "").unwrap();

    let mut entries = read_dir(dir.path()).unwrap();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(entries, [entry("a.txt", false), entry("src", true)]);
}

#[cfg(unix)]
#[test]
fn a_link_to_a_folder_is_a_folder_and_a_broken_link_is_a_file() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("real")).unwrap();
    symlink(dir.path().join("real"), dir.path().join("to_real")).unwrap();
    symlink("/nonexistent/x", dir.path().join("broken")).unwrap();

    let entries = read_dir(dir.path()).unwrap();

    let is_dir = |name: &str| entries.iter().find(|e| e.name == name).unwrap().is_dir;
    assert!(is_dir("to_real"));
    assert!(!is_dir("broken"));
}

#[test]
fn a_folder_that_is_not_there_or_is_a_file_is_an_error_that_says_which() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("f"), "").unwrap();

    let gone = read_dir(&dir.path().join("gone")).unwrap_err();
    let file = read_dir(&dir.path().join("f")).unwrap_err();

    assert!(gone.contains("not a folder that exists"), "{gone}");
    assert!(
        file.contains("is not a folder") || file.contains("cannot read"),
        "{file}"
    );
}

#[test]
fn a_folder_of_thirty_thousand_entries_is_matched_within_a_frame() {
    let entries: Vec<Entry> = (0..30_000)
        .map(|n| entry(&format!("module_{n:05}.rs"), n % 7 == 0))
        .collect();

    let start = Instant::now();
    let (found, total) = matching(&entries, "mod12", false);
    let all = matching(&entries, "", false);
    let took = start.elapsed();

    assert!(total > 0 && found.len() <= 200);
    assert_eq!(all.1, 30_000);
    assert_eq!(all.0.len(), 200, "only what shows is kept");
    assert!(took < Duration::from_millis(500), "took {took:?}");
}
