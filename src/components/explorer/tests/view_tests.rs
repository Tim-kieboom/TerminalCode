//! Dot entries that can be hidden, and links that point back up the tree.

use std::fs;

use super::explorer_tests::names;
use crate::components::explorer::{Explorer, ExplorerCommand};

fn tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".config")).unwrap();
    fs::write(dir.path().join(".config/app.toml"), "").unwrap();
    fs::write(dir.path().join(".env"), "").unwrap();
    fs::write(dir.path().join("main.rs"), "").unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/.hidden"), "").unwrap();
    fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    dir
}

/// What the explorer lists below the project's own row, indented from there.
/// (The temporary folders the tests run in have names that start with a dot,
/// so the root row also shows that it stays listed when dot entries are hidden.)
fn listed(explorer: &Explorer) -> Vec<String> {
    let all = names(explorer);
    all[1..]
        .iter()
        .map(|name| name.strip_prefix("  ").unwrap_or(name).to_owned())
        .collect()
}

fn go_to(explorer: &mut Explorer, name: &str) {
    explorer.apply(ExplorerCommand::First).unwrap();
    while explorer.selected_row().unwrap().name != name {
        let before = explorer.selected();
        explorer.apply(ExplorerCommand::Down).unwrap();
        assert_ne!(before, explorer.selected(), "no row named {name}");
    }
}

// ---- hidden entries

#[test]
fn dot_entries_are_listed_until_they_are_hidden() {
    let dir = tree();
    let mut explorer = Explorer::open(dir.path()).unwrap();
    assert!(!explorer.hides_dotfiles());
    assert_eq!(listed(&explorer), [".config", "src", ".env", "main.rs"]);

    explorer.apply(ExplorerCommand::ToggleHidden).unwrap();

    assert!(explorer.hides_dotfiles());
    assert_eq!(listed(&explorer), ["src", "main.rs"]);
}

#[test]
fn toggling_again_shows_them_again() {
    let dir = tree();
    let mut explorer = Explorer::open(dir.path()).unwrap();
    explorer.apply(ExplorerCommand::ToggleHidden).unwrap();

    explorer.apply(ExplorerCommand::ToggleHidden).unwrap();

    assert_eq!(listed(&explorer), [".config", "src", ".env", "main.rs"]);
}

#[test]
fn hiding_applies_to_open_folders_too() {
    let dir = tree();
    let mut explorer = Explorer::open(dir.path()).unwrap();
    go_to(&mut explorer, "src");
    explorer.apply(ExplorerCommand::Expand).unwrap();
    assert!(listed(&explorer).contains(&"  .hidden".to_owned()));

    explorer.apply(ExplorerCommand::ToggleHidden).unwrap();

    assert_eq!(listed(&explorer), ["src", "  lib.rs", "main.rs"]);
}

#[test]
fn a_hidden_folder_stays_open_underneath() {
    let dir = tree();
    let mut explorer = Explorer::open(dir.path()).unwrap();
    go_to(&mut explorer, ".config");
    explorer.apply(ExplorerCommand::Expand).unwrap();

    explorer.apply(ExplorerCommand::ToggleHidden).unwrap();
    explorer.apply(ExplorerCommand::ToggleHidden).unwrap();

    assert!(listed(&explorer).contains(&"  app.toml".to_owned()));
}

#[test]
fn the_selection_stays_on_its_entry_when_it_is_still_listed() {
    let dir = tree();
    let mut explorer = Explorer::open(dir.path()).unwrap();
    go_to(&mut explorer, "main.rs");

    explorer.apply(ExplorerCommand::ToggleHidden).unwrap();

    assert_eq!(explorer.selected_row().unwrap().name, "main.rs");
}

#[test]
fn hiding_the_selected_entry_leaves_a_valid_selection() {
    let dir = tree();
    let mut explorer = Explorer::open(dir.path()).unwrap();
    go_to(&mut explorer, ".env");

    explorer.apply(ExplorerCommand::ToggleHidden).unwrap();

    assert!(explorer.selected_row().is_some());
    assert!(explorer.selected() < explorer.rows().len());
}

#[test]
fn a_refresh_keeps_the_dot_entries_hidden() {
    let dir = tree();
    let mut explorer = Explorer::open(dir.path()).unwrap();
    explorer.apply(ExplorerCommand::ToggleHidden).unwrap();
    fs::write(dir.path().join(".new"), "").unwrap();
    fs::write(dir.path().join("new.rs"), "").unwrap();

    explorer.refresh().unwrap();

    assert_eq!(listed(&explorer), ["src", "main.rs", "new.rs"]);
}

#[test]
fn an_empty_project_can_toggle_without_trouble() {
    let mut explorer = Explorer::default();
    explorer.apply(ExplorerCommand::ToggleHidden).unwrap();
    assert!(explorer.rows().is_empty());
}

// ---- links that loop

#[cfg(unix)]
mod loops {
    use std::os::unix::fs::symlink;

    use super::*;

    fn linked() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("sub/deeper")).unwrap();
        fs::write(dir.path().join("sub/file.rs"), "").unwrap();
        symlink("..", dir.path().join("sub/up")).unwrap();
        symlink(".", dir.path().join("here")).unwrap();
        fs::create_dir(dir.path().join("docs")).unwrap();
        fs::write(dir.path().join("docs/a.md"), "").unwrap();
        symlink("docs", dir.path().join("docs_link")).unwrap();
        dir
    }

    fn row<'a>(explorer: &'a Explorer, name: &str) -> &'a crate::components::explorer::Row {
        explorer.rows().iter().find(|row| row.name == name).unwrap()
    }

    #[test]
    fn a_link_to_the_folder_it_is_in_is_marked_as_a_loop() {
        let dir = linked();
        let explorer = Explorer::open(dir.path()).unwrap();
        assert!(row(&explorer, "here").looped);
    }

    #[test]
    fn a_link_to_a_parent_is_marked_as_a_loop() {
        let dir = linked();
        let mut explorer = Explorer::open(dir.path()).unwrap();
        go_to(&mut explorer, "sub");
        explorer.apply(ExplorerCommand::Expand).unwrap();

        assert!(row(&explorer, "up").looped);
        assert!(!row(&explorer, "deeper").looped);
    }

    #[test]
    fn a_loop_does_not_open() {
        let dir = linked();
        let mut explorer = Explorer::open(dir.path()).unwrap();
        let before = listed(&explorer);
        go_to(&mut explorer, "here");

        explorer.apply(ExplorerCommand::Expand).unwrap();
        explorer.apply(ExplorerCommand::Open).unwrap();

        assert_eq!(listed(&explorer), before);
        assert!(!explorer.selected_row().unwrap().expanded);
    }

    #[test]
    fn a_link_to_another_folder_opens_like_a_folder() {
        let dir = linked();
        let mut explorer = Explorer::open(dir.path()).unwrap();
        go_to(&mut explorer, "docs_link");
        assert!(!explorer.selected_row().unwrap().looped);

        explorer.apply(ExplorerCommand::Expand).unwrap();

        assert!(listed(&explorer).contains(&"  a.md".to_owned()));
    }

    #[test]
    fn a_loop_stays_marked_after_a_refresh() {
        let dir = linked();
        let mut explorer = Explorer::open(dir.path()).unwrap();

        explorer.refresh().unwrap();

        assert!(row(&explorer, "here").looped);
    }

    #[test]
    fn a_loop_does_not_make_a_refresh_wander() {
        let dir = linked();
        let mut explorer = Explorer::open(dir.path()).unwrap();
        go_to(&mut explorer, "sub");
        explorer.apply(ExplorerCommand::Expand).unwrap();
        let before = explorer.rows().len();

        explorer.refresh().unwrap();

        assert_eq!(explorer.rows().len(), before);
    }
}

#[test]
fn the_project_row_stays_when_its_own_name_starts_with_a_dot() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join(".project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.rs"), "").unwrap();
    let mut explorer = Explorer::open(&root).unwrap();

    explorer.apply(ExplorerCommand::ToggleHidden).unwrap();

    assert_eq!(names(&explorer), [".project", "  a.rs"]);
}
