use std::fs;
use std::path::{Path, PathBuf};

use ratatui::layout::Rect;

use crate::components::ComponentKind;
use crate::components::explorer::{Explorer, ExplorerCommand, NodeKind};
use crate::ui::Render;
use crate::ui::layout::Placement;

/// A project with a `.gitignore`, nested directories and a hidden `.git`.
pub(super) fn project(with_git: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, ".gitignore", "target/\n*.log\n");
    write(root, "Cargo.toml", "");
    write(root, "README.md", "");
    write(root, "zeta.txt", "");
    write(root, "build.log", "");
    write(root, "src/main.rs", "fn main() {}\n");
    write(root, "src/lib.rs", "");
    write(root, "src/util/mod.rs", "");
    write(root, "docs/design.md", "");
    write(root, "target/debug.txt", "");
    if with_git {
        write(root, ".git/HEAD", "ref: refs/heads/main\n");
    }
    dir
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

pub(super) fn names(explorer: &Explorer) -> Vec<String> {
    explorer
        .rows()
        .iter()
        .map(|row| format!("{}{}", "  ".repeat(row.depth), row.name))
        .collect()
}

fn open(dir: &tempfile::TempDir) -> Explorer {
    Explorer::open(dir.path()).unwrap()
}

fn select(explorer: &mut Explorer, name: &str) {
    explorer.apply(ExplorerCommand::First).unwrap();
    while explorer.selected_row().unwrap().name != name {
        let before = explorer.selected();
        explorer.apply(ExplorerCommand::Down).unwrap();
        assert_ne!(before, explorer.selected(), "no row named {name}");
    }
}

fn apply(explorer: &mut Explorer, command: ExplorerCommand) -> Option<PathBuf> {
    explorer.apply(command).unwrap()
}

#[test]
fn opening_a_project_lists_the_root_and_its_children_directories_first() {
    let dir = project(true);

    let explorer = open(&dir);

    let root = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        names(&explorer),
        [
            root.as_str(),
            "  docs",
            "  src",
            "  .gitignore",
            "  Cargo.toml",
            "  README.md",
            "  zeta.txt",
        ]
    );
    assert!(explorer.has_project());
}

#[test]
fn ignored_files_and_the_git_directory_are_not_listed() {
    let dir = project(true);

    let listed = names(&open(&dir)).join(" ");

    assert!(!listed.contains("target"), "{listed}");
    assert!(!listed.contains("build.log"), "{listed}");
    assert!(!listed.contains(".git "), "{listed}");
    assert!(
        listed.contains(".gitignore"),
        "hidden files that are not ignored stay: {listed}"
    );
}

#[test]
fn a_gitignore_applies_outside_a_git_repository_too() {
    let dir = project(false);

    let listed = names(&open(&dir)).join(" ");

    assert!(!listed.contains("target"), "{listed}");
    assert!(!listed.contains("build.log"), "{listed}");
}

#[test]
fn directories_are_read_when_opened_not_before() {
    let dir = project(false);
    let mut explorer = open(&dir);
    // Added after the explorer was created, but before `src` is opened.
    write(dir.path(), "src/late.rs", "");

    select(&mut explorer, "src");
    apply(&mut explorer, ExplorerCommand::Expand);

    assert!(
        names(&explorer).contains(&"    late.rs".to_owned()),
        "{:?}",
        names(&explorer)
    );
}

#[test]
fn expanding_shows_the_children_one_level_deeper_and_collapsing_hides_them() {
    let dir = project(false);
    let mut explorer = open(&dir);
    select(&mut explorer, "src");

    apply(&mut explorer, ExplorerCommand::Expand);
    let expanded = names(&explorer);
    assert!(expanded.contains(&"    util".to_owned()), "{expanded:?}");
    assert!(expanded.contains(&"    main.rs".to_owned()), "{expanded:?}");
    assert_eq!(explorer.selected_row().unwrap().name, "src");

    apply(&mut explorer, ExplorerCommand::Collapse);
    assert!(!names(&explorer).iter().any(|name| name.contains("main.rs")));
    assert_eq!(explorer.selected_row().unwrap().name, "src");
}

#[test]
fn right_on_an_open_directory_steps_into_it_and_on_a_file_does_nothing() {
    let dir = project(false);
    let mut explorer = open(&dir);
    select(&mut explorer, "src");
    apply(&mut explorer, ExplorerCommand::Expand);

    apply(&mut explorer, ExplorerCommand::Expand);
    assert_eq!(explorer.selected_row().unwrap().name, "util");

    select(&mut explorer, "main.rs");
    let before = explorer.selected();
    apply(&mut explorer, ExplorerCommand::Expand);
    assert_eq!(explorer.selected(), before);
}

#[test]
fn left_on_a_file_or_a_closed_directory_steps_out_to_the_parent() {
    let dir = project(false);
    let mut explorer = open(&dir);
    select(&mut explorer, "src");
    apply(&mut explorer, ExplorerCommand::Expand);
    select(&mut explorer, "main.rs");

    apply(&mut explorer, ExplorerCommand::Collapse);
    assert_eq!(explorer.selected_row().unwrap().name, "src");

    // `src` is open: the first left closes it, the second steps out to the root.
    apply(&mut explorer, ExplorerCommand::Collapse);
    apply(&mut explorer, ExplorerCommand::Collapse);
    assert_eq!(explorer.selected(), 0);
}

#[test]
fn left_on_the_root_does_nothing_once_it_is_closed() {
    let dir = project(false);
    let mut explorer = open(&dir);

    apply(&mut explorer, ExplorerCommand::Collapse);
    assert_eq!(explorer.rows().len(), 1);
    apply(&mut explorer, ExplorerCommand::Collapse);

    assert_eq!(explorer.selected(), 0);
}

#[test]
fn open_returns_a_file_and_toggles_a_directory() {
    let dir = project(false);
    let mut explorer = open(&dir);

    select(&mut explorer, "README.md");
    let chosen = apply(&mut explorer, ExplorerCommand::Open);
    assert_eq!(chosen, Some(dir.path().join("README.md")));

    select(&mut explorer, "docs");
    assert_eq!(apply(&mut explorer, ExplorerCommand::Open), None);
    assert!(explorer.selected_row().unwrap().expanded);
    assert_eq!(apply(&mut explorer, ExplorerCommand::Open), None);
    assert!(!explorer.selected_row().unwrap().expanded);
}

#[test]
fn the_selection_stops_at_both_ends_and_first_and_last_jump() {
    let dir = project(false);
    let mut explorer = open(&dir);

    apply(&mut explorer, ExplorerCommand::Up);
    assert_eq!(explorer.selected(), 0);

    apply(&mut explorer, ExplorerCommand::Last);
    let last = explorer.rows().len() - 1;
    assert_eq!(explorer.selected(), last);
    apply(&mut explorer, ExplorerCommand::Down);
    assert_eq!(explorer.selected(), last);

    apply(&mut explorer, ExplorerCommand::First);
    assert_eq!(explorer.selected(), 0);
}

#[test]
fn paging_moves_at_least_one_row_and_stops_at_the_ends() {
    let dir = project(false);
    let mut explorer = open(&dir);

    apply(&mut explorer, ExplorerCommand::PageDown);
    assert!(explorer.selected() >= 1);
    for _ in 0..10 {
        apply(&mut explorer, ExplorerCommand::PageDown);
    }
    assert_eq!(explorer.selected(), explorer.rows().len() - 1);
    for _ in 0..10 {
        apply(&mut explorer, ExplorerCommand::PageUp);
    }
    assert_eq!(explorer.selected(), 0);
}

#[test]
fn refresh_picks_up_new_and_removed_files_and_keeps_open_directories_and_selection() {
    let dir = project(false);
    let mut explorer = open(&dir);
    select(&mut explorer, "src");
    apply(&mut explorer, ExplorerCommand::Expand);
    select(&mut explorer, "lib.rs");
    write(dir.path(), "src/new.rs", "");
    write(dir.path(), "added.md", "");
    fs::remove_file(dir.path().join("src/main.rs")).unwrap();

    apply(&mut explorer, ExplorerCommand::Refresh);

    let listed = names(&explorer);
    assert!(listed.contains(&"    new.rs".to_owned()), "{listed:?}");
    assert!(listed.contains(&"  added.md".to_owned()), "{listed:?}");
    assert!(
        !listed.iter().any(|name| name.contains("main.rs")),
        "{listed:?}"
    );
    assert_eq!(explorer.selected_row().unwrap().name, "lib.rs");
}

#[test]
fn refresh_closes_a_directory_that_was_deleted() {
    let dir = project(false);
    let mut explorer = open(&dir);
    select(&mut explorer, "docs");
    apply(&mut explorer, ExplorerCommand::Expand);
    fs::remove_dir_all(dir.path().join("docs")).unwrap();

    apply(&mut explorer, ExplorerCommand::Refresh);

    assert!(!names(&explorer).iter().any(|name| name.contains("docs")));
    assert!(explorer.selected() < explorer.rows().len());
}

#[test]
fn a_missing_root_is_an_error_naming_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");

    let error = Explorer::open(&missing).unwrap_err();

    assert!(error.to_string().contains("nope"), "{error}");
}

#[test]
fn an_explorer_without_a_project_has_no_rows_and_ignores_commands() {
    let mut explorer = Explorer::default();

    assert!(!explorer.has_project());
    assert!(explorer.rows().is_empty());
    for command in [
        ExplorerCommand::Down,
        ExplorerCommand::Expand,
        ExplorerCommand::Collapse,
        ExplorerCommand::Open,
        ExplorerCommand::Refresh,
        ExplorerCommand::Last,
    ] {
        assert_eq!(explorer.apply(command).unwrap(), None);
    }
}

#[test]
fn clicking_a_row_selects_and_opens_it() {
    let dir = project(false);
    let mut explorer = open(&dir);
    explorer.prepare(&Placement::new(
        ComponentKind::Explorer,
        Rect::new(0, 0, 30, 20),
    ));
    let body = explorer.body_for_tests();
    let docs = explorer
        .rows()
        .iter()
        .position(|row| row.name == "docs")
        .unwrap();

    let chosen = explorer.click(body.x + 2, body.y + docs as u16).unwrap();

    assert_eq!(chosen, None);
    assert_eq!(explorer.selected_row().unwrap().name, "docs");
    assert!(explorer.selected_row().unwrap().expanded);
    assert_eq!(explorer.selected_row().unwrap().kind, NodeKind::Dir);

    let readme = explorer
        .rows()
        .iter()
        .position(|row| row.name == "README.md")
        .unwrap();
    let chosen = explorer.click(body.x, body.y + readme as u16).unwrap();
    assert_eq!(chosen, Some(dir.path().join("README.md")));
}

#[test]
fn clicking_below_the_last_row_or_outside_the_body_does_nothing() {
    let dir = project(false);
    let mut explorer = open(&dir);
    explorer.prepare(&Placement::new(
        ComponentKind::Explorer,
        Rect::new(0, 0, 30, 20),
    ));
    let body = explorer.body_for_tests();

    assert_eq!(explorer.click(body.x, body.y + 19).unwrap(), None);
    assert_eq!(explorer.click(80, 80).unwrap(), None);

    assert_eq!(explorer.selected(), 0);
}

fn listed(explorer: &Explorer, name: &str) -> bool {
    explorer.rows().iter().any(|row| row.name == name)
}

fn two_open_directories() -> (tempfile::TempDir, Explorer) {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a/old_a.txt", "");
    write(dir.path(), "b/old_b.txt", "");
    let mut explorer = open(&dir);
    select(&mut explorer, "a");
    apply(&mut explorer, ExplorerCommand::Expand);
    select(&mut explorer, "b");
    apply(&mut explorer, ExplorerCommand::Expand);
    (dir, explorer)
}

#[test]
fn a_directory_opened_again_shows_what_was_added_while_it_was_closed() {
    let dir = project(false);
    let mut explorer = open(&dir);
    select(&mut explorer, "docs");
    apply(&mut explorer, ExplorerCommand::Expand);
    apply(&mut explorer, ExplorerCommand::Collapse);

    // A closed directory is not watched, so nothing refreshed it.
    write(dir.path(), "docs/added_while_closed.md", "");
    apply(&mut explorer, ExplorerCommand::Expand);

    assert!(listed(&explorer, "added_while_closed.md"));
}

#[test]
fn a_directory_opened_again_keeps_the_directories_that_were_open_inside_it() {
    let dir = project(false);
    let mut explorer = open(&dir);
    select(&mut explorer, "src");
    apply(&mut explorer, ExplorerCommand::Expand);
    select(&mut explorer, "util");
    apply(&mut explorer, ExplorerCommand::Expand);
    assert!(listed(&explorer, "mod.rs"));

    select(&mut explorer, "src");
    apply(&mut explorer, ExplorerCommand::Collapse);
    apply(&mut explorer, ExplorerCommand::Expand);

    assert!(
        listed(&explorer, "mod.rs"),
        "util is still open: {:?}",
        names(&explorer)
    );
}

#[test]
fn refresh_paths_reads_only_the_directories_the_paths_are_in() {
    let (dir, mut explorer) = two_open_directories();
    write(dir.path(), "a/new_a.txt", "");
    write(dir.path(), "b/new_b.txt", "");

    explorer
        .refresh_paths(&[dir.path().join("a/new_a.txt")])
        .unwrap();

    assert!(listed(&explorer, "new_a.txt"));
    assert!(
        !listed(&explorer, "new_b.txt"),
        "b was not named, so it was not read"
    );

    explorer.refresh().unwrap();
    assert!(
        listed(&explorer, "new_b.txt"),
        "a full refresh reads everything"
    );
}

#[test]
fn a_changed_path_that_is_an_open_directory_has_its_own_listing_read_again() {
    let (dir, mut explorer) = two_open_directories();
    write(dir.path(), "a/new_a.txt", "");

    explorer.refresh_paths(&[dir.path().join("a")]).unwrap();

    assert!(listed(&explorer, "new_a.txt"));
}

#[test]
fn a_report_of_many_paths_in_one_directory_reads_it_once_and_finds_every_change() {
    let (dir, mut explorer) = two_open_directories();
    let created: Vec<_> = (0..300)
        .map(|n| {
            write(dir.path(), &format!("a/file_{n}.txt"), "");
            dir.path().join(format!("a/file_{n}.txt"))
        })
        .collect();

    explorer.refresh_paths(&created).unwrap();

    let count = explorer
        .rows()
        .iter()
        .filter(|row| row.name.starts_with("file_"))
        .count();
    assert_eq!(count, 300);
}

#[test]
fn a_deleted_open_directory_disappears_when_its_parent_is_reported() {
    let (dir, mut explorer) = two_open_directories();
    fs::remove_dir_all(dir.path().join("a")).unwrap();

    explorer.refresh_paths(&[dir.path().join("a")]).unwrap();

    assert!(!listed(&explorer, "a"));
    assert!(!listed(&explorer, "old_a.txt"));
    assert!(listed(&explorer, "old_b.txt"), "b is untouched");
}

#[test]
fn paths_outside_the_project_change_nothing() {
    let (dir, mut explorer) = two_open_directories();
    write(dir.path(), "a/new_a.txt", "");
    let before = names(&explorer);

    explorer
        .refresh_paths(&[PathBuf::from("/somewhere/else/file.txt")])
        .unwrap();

    assert_eq!(names(&explorer), before);
}

#[test]
fn refreshing_a_very_large_directory_keeps_what_was_open_and_the_selection() {
    let dir = tempfile::tempdir().unwrap();
    for n in 0..5_000 {
        write(dir.path(), &format!("big/file_{n:04}.txt"), "");
    }
    write(dir.path(), "big/sub/inner.txt", "");
    let mut explorer = open(&dir);
    select(&mut explorer, "big");
    apply(&mut explorer, ExplorerCommand::Expand);
    select(&mut explorer, "sub");
    apply(&mut explorer, ExplorerCommand::Expand);
    select(&mut explorer, "file_2500.txt");
    write(dir.path(), "big/file_9999.txt", "");

    explorer.refresh().unwrap();

    assert!(listed(&explorer, "file_9999.txt"));
    assert!(listed(&explorer, "inner.txt"), "sub is still open");
    assert_eq!(explorer.selected_row().unwrap().name, "file_2500.txt");
    assert_eq!(
        explorer.rows().len(),
        1 + 1 + 1 + 1 + 5_001,
        "root, big, sub, inner and the files"
    );
}
