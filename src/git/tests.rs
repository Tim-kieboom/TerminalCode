use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::marks::MarkKind;
use super::*;

fn tracked(staged: Option<Kind>, unstaged: Option<Kind>) -> Change {
    Change::Tracked { staged, unstaged }
}

fn entry(path: &str, change: Change) -> Entry {
    Entry {
        path: PathBuf::from(path),
        original: None,
        change,
    }
}

/// The output of `git status`: the records joined by NUL, as `-z` does.
fn output(records: &[&str]) -> Vec<u8> {
    let mut bytes = records.join("\0").into_bytes();
    bytes.push(0);
    bytes
}

const HASH: &str = "0000000000000000000000000000000000000000";

// --- reading the answer ---

#[test]
fn the_branch_is_read_from_the_headers() {
    let answer = output(&[
        "# branch.oid 1234567890abcdef1234567890abcdef12345678",
        "# branch.head main",
        "# branch.upstream origin/main",
        "# branch.ab +3 -1",
    ]);

    let (branch, entries) = parse_status(&answer).unwrap();

    assert_eq!(
        branch,
        Branch {
            name: Some("main".into()),
            oid: Some("1234567890abcdef1234567890abcdef12345678".into()),
            upstream: Some("origin/main".into()),
            ahead: 3,
            behind: 1,
        }
    );
    assert_eq!(entries, vec![]);
}

#[test]
fn a_detached_head_has_no_name_and_is_labelled_by_its_commit() {
    let answer = output(&["# branch.oid 1234567890abcdef", "# branch.head (detached)"]);

    let (branch, _) = parse_status(&answer).unwrap();

    assert_eq!(branch.name, None);
    assert_eq!(branch.label(), "(1234567)");
}

#[test]
fn a_branch_before_its_first_commit_has_a_name_and_no_commit() {
    let answer = output(&["# branch.oid (initial)", "# branch.head main"]);

    let (branch, _) = parse_status(&answer).unwrap();

    assert_eq!(branch.name.as_deref(), Some("main"));
    assert_eq!(branch.oid, None);
    assert_eq!(branch.label(), "main");
}

#[test]
fn a_branch_without_a_name_or_commit_says_so() {
    assert_eq!(Branch::default().label(), "(no commit)");
}

#[test]
fn staged_and_unstaged_changes_are_told_apart() {
    let line =
        |xy: &str, path: &str| format!("1 {xy} N... 100644 100644 100644 {HASH} {HASH} {path}");
    let answer = output(&[
        &line("M.", "staged.rs"),
        &line(".M", "unstaged.rs"),
        &line("MD", "both.rs"),
        &line("A.", "new.rs"),
        &line(".D", "gone.rs"),
        &line(".T", "link.rs"),
    ]);

    let (_, entries) = parse_status(&answer).unwrap();

    assert_eq!(
        entries,
        vec![
            entry("staged.rs", tracked(Some(Kind::Modified), None)),
            entry("unstaged.rs", tracked(None, Some(Kind::Modified))),
            entry(
                "both.rs",
                tracked(Some(Kind::Modified), Some(Kind::Deleted))
            ),
            entry("new.rs", tracked(Some(Kind::Added), None)),
            entry("gone.rs", tracked(None, Some(Kind::Deleted))),
            entry("link.rs", tracked(None, Some(Kind::TypeChanged))),
        ]
    );
}

#[test]
fn a_path_with_spaces_is_one_path() {
    let answer = output(&[&format!(
        "1 .M N... 100644 100644 100644 {HASH} {HASH} my notes/a b.txt"
    )]);

    let (_, entries) = parse_status(&answer).unwrap();

    assert_eq!(entries[0].path, PathBuf::from("my notes/a b.txt"));
}

#[test]
fn a_renamed_file_has_its_old_path_from_the_next_record() {
    let answer = output(&[
        &format!("2 R. N... 100644 100644 100644 {HASH} {HASH} R100 new name.rs"),
        "old name.rs",
        "? after.txt",
    ]);

    let (_, entries) = parse_status(&answer).unwrap();

    assert_eq!(
        entries,
        vec![
            Entry {
                path: PathBuf::from("new name.rs"),
                original: Some(PathBuf::from("old name.rs")),
                change: tracked(Some(Kind::Renamed), None),
            },
            entry("after.txt", Change::Untracked),
        ]
    );
}

#[test]
fn a_rename_without_its_old_path_is_unreadable() {
    let answer = output(&[&format!(
        "2 R. N... 100644 100644 100644 {HASH} {HASH} R100 new.rs"
    )]);

    assert!(matches!(
        parse_status(&answer),
        Err(GitError::Unreadable(_))
    ));
}

#[test]
fn untracked_and_conflicted_files_are_recognised() {
    let answer = output(&[
        "? notes.txt",
        "? build/",
        &format!("u UU N... 100644 100644 100644 100644 {HASH} {HASH} {HASH} clash.rs"),
    ]);

    let (_, entries) = parse_status(&answer).unwrap();

    assert_eq!(
        entries,
        vec![
            entry("notes.txt", Change::Untracked),
            entry("build/", Change::Untracked),
            entry("clash.rs", Change::Conflicted),
        ]
    );
}

#[test]
fn ignored_files_and_records_from_a_newer_git_are_skipped() {
    let answer = output(&["! target/", "9 something new", "? kept.txt"]);

    let (_, entries) = parse_status(&answer).unwrap();

    assert_eq!(entries, vec![entry("kept.txt", Change::Untracked)]);
}

#[test]
fn a_broken_record_is_unreadable() {
    for record in [
        "1 .M".to_owned(),
        format!("1 X. N... 100644 100644 100644 {HASH} {HASH} a.rs"),
        format!("1 M N... 100644 100644 100644 {HASH} {HASH} a.rs"),
        "u UU N...".to_owned(),
    ] {
        assert!(
            matches!(
                parse_status(&output(&[&record])),
                Err(GitError::Unreadable(_))
            ),
            "{record}"
        );
    }
}

#[test]
fn nothing_changed_is_no_entries() {
    assert_eq!(parse_status(b"").unwrap(), (Branch::default(), vec![]));
}

// --- when to ask ---

fn at(start: Instant, millis: u64) -> Instant {
    start + Duration::from_millis(millis)
}

#[test]
fn nothing_is_wanted_until_something_changes() {
    let schedule = Schedule::default();

    assert_eq!(schedule.deadline(), None);
    assert!(!schedule.is_due(Instant::now()));
}

#[test]
fn a_change_is_asked_about_after_a_moment() {
    let start = Instant::now();
    let mut schedule = Schedule::default();

    schedule.request(start);

    assert_eq!(schedule.deadline(), Some(at(start, 300)));
    assert!(!schedule.is_due(at(start, 299)));
    assert!(schedule.is_due(at(start, 300)));
}

#[test]
fn more_changes_in_the_same_burst_do_not_move_it_back() {
    let start = Instant::now();
    let mut schedule = Schedule::default();

    schedule.request(start);
    schedule.request(at(start, 200));
    schedule.request(at(start, 250));

    assert_eq!(schedule.deadline(), Some(at(start, 300)));
}

#[test]
fn only_one_refresh_runs_and_the_change_during_it_waits() {
    let start = Instant::now();
    let mut schedule = Schedule::default();
    schedule.request(start);
    schedule.started();

    schedule.request(at(start, 400));

    assert_eq!(schedule.deadline(), None, "nothing starts while one runs");
    schedule.finished(at(start, 500), Duration::from_millis(100));
    assert_eq!(
        schedule.deadline(),
        Some(at(start, 700)),
        "twice what the last took"
    );
}

#[test]
fn a_slow_refresh_makes_the_next_wait_twice_as_long() {
    let start = Instant::now();
    let mut schedule = Schedule::default();
    schedule.request(start);
    schedule.started();
    schedule.finished(at(start, 5_000), Duration::from_secs(5));

    schedule.request(at(start, 5_001));

    assert_eq!(schedule.deadline(), Some(at(start, 15_000)));
    assert!(!schedule.is_due(at(start, 14_999)));
    assert!(schedule.is_due(at(start, 15_000)));
}

#[test]
fn a_quick_refresh_still_waits_for_the_burst_to_settle() {
    let start = Instant::now();
    let mut schedule = Schedule::default();
    schedule.request(start);
    schedule.started();
    schedule.finished(at(start, 20), Duration::from_millis(20));

    schedule.request(at(start, 30));

    assert_eq!(schedule.deadline(), Some(at(start, 330)));
}

// --- the real thing ---

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A repository on branch `main` with `kept.txt` committed.
fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    std::fs::write(dir.path().join("kept.txt"), "one\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "first"]);
    dir
}

#[test]
fn a_clean_repository_has_a_branch_and_no_changes() {
    let dir = repository();

    let status = Git::default().status(&canonical(dir.path())).unwrap();

    assert_eq!(status.branch.name.as_deref(), Some("main"));
    assert!(status.branch.oid.is_some());
    assert_eq!(status.entries, vec![]);
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

#[test]
fn changes_are_found_staged_unstaged_and_untracked() {
    let dir = repository();
    let root = dir.path();
    std::fs::write(root.join("kept.txt"), "two\n").unwrap();
    std::fs::write(root.join("added.txt"), "new\n").unwrap();
    git(root, &["add", "added.txt"]);
    std::fs::write(root.join("loose.txt"), "x\n").unwrap();

    let status = Git::default().status(&canonical(root)).unwrap();

    assert_eq!(
        status.entries,
        vec![
            entry("added.txt", tracked(Some(Kind::Added), None)),
            entry("kept.txt", tracked(None, Some(Kind::Modified))),
            entry("loose.txt", Change::Untracked),
        ]
    );
}

#[test]
fn a_renamed_file_is_found_with_its_old_name() {
    let dir = repository();
    git(dir.path(), &["mv", "kept.txt", "moved.txt"]);

    let status = Git::default().status(&canonical(dir.path())).unwrap();

    assert_eq!(
        status.entries,
        vec![Entry {
            path: PathBuf::from("moved.txt"),
            original: Some(PathBuf::from("kept.txt")),
            change: tracked(Some(Kind::Renamed), None),
        }]
    );
}

#[test]
fn the_repository_is_found_from_a_folder_inside_it() {
    let dir = repository();
    let inside = dir.path().join("src/deep");
    std::fs::create_dir_all(&inside).unwrap();

    let root = Git::default().repository_root(&inside).unwrap();

    assert_eq!(canonical(&root), canonical(dir.path()));
}

#[test]
fn changes_outside_the_project_folder_are_still_listed() {
    let dir = repository();
    let project = dir.path().join("app");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(dir.path().join("kept.txt"), "outside\n").unwrap();

    let status = Git::default().refresh(&project, None).unwrap();

    assert_eq!(canonical(&status.root), canonical(dir.path()));
    assert_eq!(
        status.entries,
        vec![entry("kept.txt", tracked(None, Some(Kind::Modified)))]
    );
}

#[test]
fn a_folder_outside_any_repository_is_no_repository() {
    let dir = tempfile::tempdir().unwrap();

    let error = Git::default().repository_root(dir.path()).unwrap_err();

    assert!(matches!(error, GitError::NoRepository(_)), "{error:?}");
    assert!(error.is_final());
}

#[test]
fn a_missing_git_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    let git = Git::with_program("trcode-no-such-git-program", Duration::from_secs(1));

    let error = git.repository_root(dir.path()).unwrap_err();

    assert_eq!(error, GitError::NotInstalled);
    assert!(error.is_final());
}

// --- a stand-in for git, to see how it is called and what a failure does ---

#[cfg(unix)]
mod stand_in {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// A program that runs `script` where `git` would be run.
    fn program(dir: &Path, script: &str) -> PathBuf {
        let path = dir.join("fake-git");
        let probe = "[ \"$1\" = --probe ] && exit 0";
        std::fs::write(&path, format!("#!/bin/sh\n{probe}\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Another test's child may still hold the file open for writing, and
        // running it is refused ("text file busy") until that child has started.
        while let Err(error) = Command::new(&path).arg("--probe").status() {
            assert_eq!(error.kind(), std::io::ErrorKind::ExecutableFileBusy);
            std::thread::sleep(Duration::from_millis(5));
        }
        path
    }

    #[test]
    fn a_read_takes_no_locks_and_never_asks_for_a_password() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let script = format!(
            "echo \"$@\" > {log}\necho \"$GIT_OPTIONAL_LOCKS $GIT_TERMINAL_PROMPT\" >> {log}",
            log = log.display()
        );
        let git = Git::with_program(program(dir.path(), &script), Duration::from_secs(5));

        git.status(dir.path()).unwrap();

        let logged = std::fs::read_to_string(&log).unwrap();
        let mut lines = logged.lines();
        let arguments = lines.next().unwrap();
        assert!(
            arguments.starts_with("--no-optional-locks -C "),
            "{arguments}"
        );
        assert!(arguments.contains("status --porcelain=v2"), "{arguments}");
        assert_eq!(lines.next(), Some("0 0"));
    }

    #[test]
    fn a_call_that_takes_too_long_is_given_up_on() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::with_program(program(dir.path(), "sleep 30"), Duration::from_millis(200));
        let started = Instant::now();

        let error = git.status(dir.path()).unwrap_err();

        assert_eq!(error, GitError::TimedOut(Duration::from_millis(200)));
        assert!(!error.is_final(), "a slow repository is asked again");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_failure_says_what_git_said() {
        let dir = tempfile::tempdir().unwrap();
        let script = "echo 'fatal: index file corrupt' >&2\nexit 1";
        let git = Git::with_program(program(dir.path(), script), Duration::from_secs(5));

        let error = git.status(dir.path()).unwrap_err();

        assert_eq!(
            error,
            GitError::Failed {
                command: "status".into(),
                detail: "fatal: index file corrupt".into(),
            }
        );
        assert!(!error.is_final());
    }

    #[test]
    fn a_lot_of_output_does_not_stop_the_call() {
        let dir = tempfile::tempdir().unwrap();
        // More than a pipe holds, as one unreadable line the call must still end on.
        let script = "head -c 1000000 /dev/zero | tr '\\0' 'x'";
        let git = Git::with_program(program(dir.path(), script), Duration::from_secs(10));

        let result = git.status(dir.path());

        assert!(result.is_ok(), "{result:?}");
    }
}

// --- what shows beside a file ---

fn changed(path: &str, change: Change) -> Entry {
    entry(path, change)
}

fn status_of(root: &Path, entries: Vec<Entry>) -> Status {
    Status {
        root: root.to_path_buf(),
        branch: Branch::default(),
        entries,
    }
}

fn mark(kind: MarkKind, letter: char) -> Option<Mark> {
    Some(Mark { kind, letter })
}

#[test]
fn a_file_has_the_mark_of_its_change() {
    let root = Path::new("/repo");
    let marks = Marks::from_status(&status_of(
        root,
        vec![
            changed("m.rs", tracked(None, Some(Kind::Modified))),
            changed("t.rs", tracked(None, Some(Kind::TypeChanged))),
            changed("a.rs", tracked(Some(Kind::Added), None)),
            changed("c.rs", tracked(Some(Kind::Copied), None)),
            changed("d.rs", tracked(None, Some(Kind::Deleted))),
            changed("r.rs", tracked(Some(Kind::Renamed), None)),
            changed("u.rs", Change::Untracked),
            changed("x.rs", Change::Conflicted),
        ],
    ));

    let of = |name: &str| marks.of(&root.join(name));

    assert_eq!(of("m.rs"), mark(MarkKind::Modified, 'M'));
    assert_eq!(of("t.rs"), mark(MarkKind::Modified, 'M'));
    assert_eq!(of("a.rs"), mark(MarkKind::Added, 'A'));
    assert_eq!(of("c.rs"), mark(MarkKind::Added, 'A'));
    assert_eq!(of("d.rs"), mark(MarkKind::Deleted, 'D'));
    assert_eq!(of("r.rs"), mark(MarkKind::Added, 'R'));
    assert_eq!(of("u.rs"), mark(MarkKind::Untracked, 'U'));
    assert_eq!(of("x.rs"), mark(MarkKind::Conflict, '!'));
    assert_eq!(of("clean.rs"), None);
}

#[test]
fn a_file_changed_on_both_sides_shows_the_stronger_change() {
    let root = Path::new("/repo");
    let marks = Marks::from_status(&status_of(
        root,
        vec![
            changed("a.rs", tracked(Some(Kind::Added), Some(Kind::Modified))),
            changed("b.rs", tracked(Some(Kind::Modified), Some(Kind::Deleted))),
        ],
    ));

    assert_eq!(marks.of(&root.join("a.rs")), mark(MarkKind::Modified, 'M'));
    assert_eq!(marks.of(&root.join("b.rs")), mark(MarkKind::Deleted, 'D'));
}

#[test]
fn a_folder_takes_the_strongest_mark_of_what_is_inside() {
    let root = Path::new("/repo");
    let marks = Marks::from_status(&status_of(
        root,
        vec![
            changed("src/a/new.rs", Change::Untracked),
            changed("src/a/deep/edit.rs", tracked(None, Some(Kind::Modified))),
            changed("src/b/clash.rs", Change::Conflicted),
            changed("docs/note.md", tracked(Some(Kind::Added), None)),
        ],
    ));

    let kind = |folder: &str| marks.of(&root.join(folder)).map(|mark| mark.kind);

    assert_eq!(kind("src/a/deep"), Some(MarkKind::Modified));
    assert_eq!(kind("src/a"), Some(MarkKind::Modified));
    assert_eq!(kind("src/b"), Some(MarkKind::Conflict));
    assert_eq!(kind("src"), Some(MarkKind::Conflict));
    assert_eq!(kind("docs"), Some(MarkKind::Added));
    assert_eq!(kind("elsewhere"), None);
}

#[test]
fn the_files_in_an_untracked_folder_are_untracked_too() {
    let root = Path::new("/repo");
    let marks = Marks::from_status(&status_of(root, vec![changed("build/", Change::Untracked)]));

    assert_eq!(
        marks.of(&root.join("build")).map(|mark| mark.kind),
        Some(MarkKind::Untracked)
    );
    assert_eq!(
        marks.of(&root.join("build/out/a.o")).map(|mark| mark.kind),
        Some(MarkKind::Untracked)
    );
    assert_eq!(marks.of(&root.join("builder/a.o")), None);
}

#[test]
fn a_file_inside_a_changed_folder_is_not_marked_by_the_folder() {
    let root = Path::new("/repo");
    let marks = Marks::from_status(&status_of(
        root,
        vec![changed("src/a.rs", tracked(None, Some(Kind::Modified)))],
    ));

    assert_eq!(marks.of(&root.join("src/b.rs")), None);
}

#[test]
fn no_status_has_no_marks() {
    assert_eq!(Marks::default().of(Path::new("/repo/a.rs")), None);
}

#[test]
fn a_path_outside_the_repository_has_no_mark() {
    let marks = Marks::from_status(&status_of(
        Path::new("/repo"),
        vec![changed("a.rs", tracked(None, Some(Kind::Modified)))],
    ));

    assert_eq!(marks.of(Path::new("/elsewhere/a.rs")), None);
}

#[cfg(unix)]
#[test]
fn a_path_through_a_link_is_found_by_where_it_really_is() {
    let dir = tempfile::tempdir().unwrap();
    let real = canonical(dir.path()).join("repo");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("a.rs"), "").unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let marks = Marks::from_status(&status_of(
        &real,
        vec![changed("a.rs", tracked(None, Some(Kind::Modified)))],
    ));

    assert_eq!(marks.of(&link.join("a.rs")), mark(MarkKind::Modified, 'M'));
}

#[test]
fn every_mark_has_a_theme_slot_in_every_built_in_theme() {
    for kind in [
        MarkKind::Untracked,
        MarkKind::Added,
        MarkKind::Modified,
        MarkKind::Deleted,
        MarkKind::Conflict,
    ] {
        let slot = Mark { kind, letter: ' ' }.slot();
        for theme in [
            include_str!("../../crates/ui/defaults/default_theme.toml"),
            include_str!("../../crates/ui/defaults/light_theme.toml"),
            include_str!("../../crates/ui/defaults/sol_dark_theme.toml"),
            include_str!("../../crates/ui/defaults/sol_light_theme.toml"),
        ] {
            assert!(theme.contains(&format!("[\"{slot}\"]")), "{slot}");
        }
    }
}
