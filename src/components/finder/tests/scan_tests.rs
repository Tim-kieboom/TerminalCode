use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use tokio::sync::mpsc;

use crate::components::finder::start_scan;
use crate::event::Event;

fn write(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "").unwrap();
}

/// Runs a walk to the end and returns the files it found and the number of
/// batches they came in.
fn walk(root: &Path, scan: u64) -> (BTreeSet<String>, usize, Option<usize>) {
    let (tx, mut rx) = mpsc::channel(16);
    let _handle = start_scan(root.to_path_buf(), scan, tx);
    let mut files = BTreeSet::new();
    let mut batches = 0;
    while let Some(event) = rx.blocking_recv() {
        match event {
            Event::FinderBatch {
                scan: seen,
                files: found,
            } => {
                assert_eq!(seen, scan);
                batches += 1;
                files.extend(
                    found
                        .iter()
                        .map(|path| path.to_string_lossy().replace('\\', "/")),
                );
            }
            Event::FinderDone {
                scan: seen,
                unreadable,
            } => {
                assert_eq!(seen, scan);
                return (files, batches, Some(unreadable));
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
    (files, batches, None)
}

#[test]
fn the_walk_finds_files_as_paths_from_the_root() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Cargo.toml");
    write(dir.path(), "src/main.rs");
    write(dir.path(), "src/util/mod.rs");

    let (files, _, done) = walk(dir.path(), 5);

    assert_eq!(
        files.iter().map(String::as_str).collect::<Vec<_>>(),
        ["Cargo.toml", "src/main.rs", "src/util/mod.rs"]
    );
    assert_eq!(done, Some(0));
}

#[test]
fn ignored_files_and_the_git_directory_are_left_out_but_other_hidden_files_stay() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".gitignore");
    fs::write(dir.path().join(".gitignore"), "target/\n*.log\n").unwrap();
    write(dir.path(), ".env");
    write(dir.path(), "kept.rs");
    write(dir.path(), "build.log");
    write(dir.path(), "target/debug/big.bin");
    write(dir.path(), ".git/HEAD");

    let (files, _, _) = walk(dir.path(), 1);

    assert_eq!(
        files.iter().map(String::as_str).collect::<Vec<_>>(),
        [".env", ".gitignore", "kept.rs"]
    );
}

#[test]
fn directories_are_not_listed_only_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("empty/nested")).unwrap();
    write(dir.path(), "a.txt");

    let (files, _, _) = walk(dir.path(), 1);

    assert_eq!(
        files.iter().map(String::as_str).collect::<Vec<_>>(),
        ["a.txt"]
    );
}

#[test]
fn many_files_arrive_in_several_batches() {
    let dir = tempfile::tempdir().unwrap();
    for number in 0..1200 {
        write(dir.path(), &format!("d{}/f{number}.txt", number % 7));
    }

    let (files, batches, done) = walk(dir.path(), 1);

    assert_eq!(files.len(), 1200);
    assert!(batches >= 3, "{batches} batches");
    assert_eq!(done, Some(0));
}

#[cfg(unix)]
#[test]
fn a_link_to_a_file_is_listed_and_a_link_to_a_directory_is_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "real.txt");
    write(dir.path(), "dir/inner.txt");
    std::os::unix::fs::symlink(dir.path().join("real.txt"), dir.path().join("link.txt")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("dir"), dir.path().join("dirlink")).unwrap();

    let (files, _, _) = walk(dir.path(), 1);

    assert!(files.contains("link.txt"), "{files:?}");
    assert!(
        !files.iter().any(|file| file.starts_with("dirlink")),
        "{files:?}"
    );
}

#[test]
fn an_empty_project_still_finishes() {
    let dir = tempfile::tempdir().unwrap();

    let (files, batches, done) = walk(dir.path(), 1);

    assert!(files.is_empty());
    assert_eq!(batches, 0);
    assert_eq!(done, Some(0));
}
