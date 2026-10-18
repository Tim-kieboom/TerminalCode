use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::marks::MarkKind;
use super::*;
use crate::git::HunkKind;

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

// --- the lines that differ from the index ---

use super::diff::{Hunk, diff, hash_lines};

fn hunks(old: &str, new: &str) -> Vec<(HunkKind, std::ops::Range<usize>)> {
    diff(&hash_lines(old), &hash_lines(new))
        .into_iter()
        .map(|Hunk { kind, lines }| (kind, lines))
        .collect()
}

#[test]
fn equal_texts_have_no_hunks() {
    assert_eq!(hunks("a\nb\nc\n", "a\nb\nc\n"), vec![]);
    assert_eq!(hunks("", ""), vec![]);
}

#[test]
fn a_changed_line_is_modified() {
    assert_eq!(
        hunks("a\nb\nc\n", "a\nB\nc\n"),
        vec![(HunkKind::Modified, 1..2)]
    );
}

#[test]
fn a_new_line_is_added() {
    assert_eq!(hunks("a\nc\n", "a\nb\nc\n"), vec![(HunkKind::Added, 1..2)]);
    assert_eq!(
        hunks("a\n", "a\nb\nc\n"),
        vec![(HunkKind::Added, 1..3)],
        "at the end"
    );
}

#[test]
fn a_line_that_is_gone_is_a_removal_at_the_line_that_follows() {
    assert_eq!(
        hunks("a\nb\nc\n", "a\nc\n"),
        vec![(HunkKind::Removed, 1..1)]
    );
}

#[test]
fn a_line_replaced_by_more_lines_is_modified_and_added() {
    assert_eq!(
        hunks("a\nb\nc\n", "a\nx\ny\nz\nc\n"),
        vec![(HunkKind::Modified, 1..2), (HunkKind::Added, 2..4)]
    );
}

#[test]
fn lines_replaced_by_fewer_lines_are_modified() {
    assert_eq!(
        hunks("a\nb\nc\nd\n", "a\nx\nd\n"),
        vec![(HunkKind::Modified, 1..2)]
    );
}

#[test]
fn separate_changes_are_separate_hunks_in_order() {
    assert_eq!(
        hunks("a\nb\nc\nd\ne\nf\n", "a\nB\nc\nd\ne\nf\ng\n"),
        vec![(HunkKind::Modified, 1..2), (HunkKind::Added, 6..7)]
    );
}

#[test]
fn a_text_that_has_nothing_yet_has_every_line_added() {
    let hunks = diff(&[], &hash_lines("a\nb\n"));

    assert_eq!(
        hunks,
        vec![Hunk {
            kind: HunkKind::Added,
            lines: 0..3
        }]
    );
}

#[test]
fn a_text_that_became_empty_is_one_removal() {
    assert_eq!(hunks("a\nb\n", ""), vec![(HunkKind::Removed, 0..0)]);
}

#[test]
fn line_endings_do_not_count_as_a_difference() {
    assert_eq!(hunks("a\r\nb\r\n", "a\nb\n"), vec![]);
}

#[test]
fn a_difference_too_big_to_follow_is_one_modified_stretch() {
    let old: String = (0..2000).map(|n| format!("old {n}\n")).collect();
    let new: String = (0..2000).map(|n| format!("new {n}\n")).collect();

    let found = hunks(&old, &new);

    assert_eq!(found, vec![(HunkKind::Modified, 0..2000)]);
}

/// The length of the longest common subsequence, the slow and sure way.
fn longest_common(a: &[u64], b: &[u64]) -> usize {
    let mut table = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..a.len() {
        for j in 0..b.len() {
            table[i + 1][j + 1] = if a[i] == b[j] {
                table[i][j] + 1
            } else {
                table[i][j + 1].max(table[i + 1][j])
            };
        }
    }
    table[a.len()][b.len()]
}

#[test]
fn the_lines_without_a_mark_are_the_longest_common_part() {
    // A small deterministic generator: texts of a few kinds of lines, so the
    // two texts share much and differ in many ways.
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move |below: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % below
    };
    for _ in 0..400 {
        let old: Vec<u64> = (0..next(30)).map(|_| next(5)).collect();
        let new: Vec<u64> = (0..next(30)).map(|_| next(5)).collect();

        let found = diff(&old, &new);

        let mut marked = vec![false; new.len()];
        let mut last_end = 0;
        for hunk in &found {
            assert!(hunk.lines.start >= last_end, "in order, apart: {found:?}");
            last_end = hunk.lines.end;
            for line in hunk.lines.clone() {
                marked[line] = true;
            }
        }
        let unmarked: Vec<u64> = new
            .iter()
            .zip(&marked)
            .filter(|(_, marked)| !**marked)
            .map(|(line, _)| *line)
            .collect();
        assert_eq!(
            unmarked.len(),
            longest_common(&old, &new),
            "old {old:?} new {new:?} found {found:?}"
        );
        // What is not marked is in the old text, in the same order.
        let mut rest = old.iter();
        assert!(
            unmarked.iter().all(|line| rest.any(|old| old == line)),
            "old {old:?} new {new:?} found {found:?}"
        );
    }
}

// --- the marks of one document ---

use super::Lines;
use crate::buffer::Buffer;

fn lines_for(base: &str, text: &str) -> Lines {
    let mut lines = Lines::default();
    let request = lines.ask(PathBuf::from("a.txt"), None);
    assert!(lines.accept(request, Some(hash_lines(base))));
    lines.refresh(&Buffer::from_text(text));
    lines
}

#[test]
fn a_line_is_marked_by_the_hunk_it_is_in() {
    let lines = lines_for("a\nb\nc\nd\ne\n", "a\nB\nc\nd\nx\ny\ne\n");

    let marks: Vec<_> = (0..8).map(|line| lines.mark_at(line)).collect();

    assert_eq!(
        marks,
        vec![
            None,
            Some(HunkKind::Modified),
            None,
            None,
            Some(HunkKind::Added),
            Some(HunkKind::Added),
            None,
            None,
        ]
    );
}

#[test]
fn a_removal_marks_the_line_after_the_removed_ones() {
    let lines = lines_for("a\nb\nc\n", "a\nc\n");

    assert_eq!(lines.mark_at(0), None);
    assert_eq!(lines.mark_at(1), Some(HunkKind::Removed));
    assert_eq!(lines.mark_at(2), None);
}

#[test]
fn without_a_base_nothing_is_marked() {
    let mut lines = Lines::default();
    lines.refresh(&Buffer::from_text("a\nb\n"));

    assert_eq!(lines.mark_at(0), None);
}

#[test]
fn a_file_git_does_not_have_yet_is_marked_as_added_all_through() {
    let mut lines = Lines::default();
    let request = lines.ask(PathBuf::from("new.txt"), Some(Change::Untracked));
    lines.accept(request, Some(vec![]));
    lines.refresh(&Buffer::from_text("a\nb"));

    assert_eq!(lines.mark_at(0), Some(HunkKind::Added));
    assert_eq!(lines.mark_at(1), Some(HunkKind::Added));
}

#[test]
fn the_marks_follow_the_text_when_it_changes() {
    let mut buffer = Buffer::from_text("a\nb\n");
    let mut lines = Lines::default();
    let request = lines.ask(PathBuf::from("a.txt"), None);
    lines.accept(request, Some(hash_lines("a\nb\n")));
    lines.refresh(&buffer);
    assert_eq!(lines.mark_at(0), None);

    let mut transaction = buffer.begin_transaction(crate::buffer::Selections::default());
    transaction
        .apply(&crate::buffer::Edit::insert(1, "x"))
        .unwrap();
    transaction.commit(crate::buffer::Selections::default());
    lines.refresh(&buffer);

    assert_eq!(lines.mark_at(0), Some(HunkKind::Modified));
}

#[test]
fn the_index_is_asked_again_only_when_the_state_of_the_file_changed() {
    let mut lines = Lines::default();
    let path = Path::new("a.txt");
    assert!(lines.wants(path, None), "never asked");

    lines.ask(path.to_path_buf(), None);

    assert!(!lines.wants(path, None));
    assert!(lines.wants(path, Some(tracked(Some(Kind::Modified), None))));
    assert!(lines.wants(Path::new("b.txt"), None));
}

#[test]
fn the_answer_to_a_replaced_request_is_dropped() {
    let mut lines = Lines::default();
    let first = lines.ask(PathBuf::from("a.txt"), None);
    let second = lines.ask(PathBuf::from("a.txt"), Some(Change::Untracked));

    assert!(!lines.accept(first, Some(hash_lines("old"))));
    assert!(lines.accept(second, Some(hash_lines("new"))));
}

#[test]
fn a_failed_request_is_asked_again_and_clearing_drops_what_is_coming() {
    let mut lines = Lines::default();
    let path = Path::new("a.txt");
    let request = lines.ask(path.to_path_buf(), None);

    lines.failed(request);
    assert!(lines.wants(path, None), "asked again");

    let request = lines.ask(path.to_path_buf(), None);
    lines.clear();
    assert!(!lines.accept(request, Some(vec![])), "dropped");
}

// --- the index text of a real repository ---

#[test]
fn the_index_text_of_a_tracked_file_is_what_was_staged_not_the_working_file() {
    let dir = repository();
    let root = canonical(dir.path());
    std::fs::write(root.join("kept.txt"), "one\nunstaged\n").unwrap();

    let before = Git::default()
        .index_lines(&root, Path::new("kept.txt"))
        .unwrap();
    git(&root, &["add", "kept.txt"]);
    let after = Git::default()
        .index_lines(&root, Path::new("kept.txt"))
        .unwrap();

    assert_eq!(before, Some(hash_lines("one\n")));
    assert_eq!(after, Some(hash_lines("one\nunstaged\n")));
}

#[test]
fn a_file_in_a_folder_with_spaces_is_found() {
    let dir = repository();
    let root = canonical(dir.path());
    std::fs::create_dir_all(root.join("my dir")).unwrap();
    std::fs::write(root.join("my dir/a b.txt"), "x\n").unwrap();
    git(&root, &["add", "."]);

    let text = Git::default()
        .index_lines(&root, Path::new("my dir").join("a b.txt").as_path())
        .unwrap();

    assert_eq!(text, Some(hash_lines("x\n")));
}

#[test]
fn a_file_git_does_not_have_yet_has_an_empty_base() {
    let dir = repository();
    let root = canonical(dir.path());
    std::fs::write(root.join("new.txt"), "x\n").unwrap();

    let text = Git::default()
        .index_lines(&root, Path::new("new.txt"))
        .unwrap();

    assert_eq!(text, Some(vec![]));
}

#[test]
fn an_ignored_file_has_no_base() {
    let dir = repository();
    let root = canonical(dir.path());
    std::fs::write(root.join(".gitignore"), "*.log\n").unwrap();
    std::fs::write(root.join("a.log"), "x\n").unwrap();

    let text = Git::default()
        .index_lines(&root, Path::new("a.log"))
        .unwrap();

    assert_eq!(text, None);
}

#[test]
fn a_file_that_is_not_text_has_no_base() {
    let dir = repository();
    let root = canonical(dir.path());
    std::fs::write(root.join("blob.bin"), [0xff, 0xfe, 0x00, 0x80]).unwrap();
    git(&root, &["add", "blob.bin"]);

    let text = Git::default()
        .index_lines(&root, Path::new("blob.bin"))
        .unwrap();

    assert_eq!(text, None);
}

// --- doing it sooner after the editor's own change ---

#[test]
fn a_change_the_editor_made_does_not_wait_for_a_burst_to_end() {
    let start = Instant::now();
    let mut schedule = Schedule::default();

    schedule.request_now(start);

    assert_eq!(schedule.deadline(), Some(start));
    assert!(schedule.is_due(start));
}

#[test]
fn a_change_of_the_editor_pulls_a_waiting_refresh_forward() {
    let start = Instant::now();
    let mut schedule = Schedule::default();
    schedule.request(start);

    schedule.request_now(at(start, 50));

    assert_eq!(schedule.deadline(), Some(at(start, 50)));
}

#[test]
fn a_change_of_the_editor_still_waits_for_the_back_off() {
    let start = Instant::now();
    let mut schedule = Schedule::default();
    schedule.request(start);
    schedule.started();
    schedule.finished(at(start, 2_000), Duration::from_secs(2));

    schedule.request_now(at(start, 2_001));

    assert_eq!(schedule.deadline(), Some(at(start, 6_000)));
}

// --- changing the repository ---

use super::ActionError;
use super::actions::repository_path;

#[test]
fn a_path_is_written_the_way_git_wants_it() {
    assert_eq!(repository_path("a.rs").unwrap(), "a.rs");
    assert_eq!(repository_path("src/a.rs").unwrap(), "src/a.rs");
    assert_eq!(repository_path("./src//a.rs").unwrap(), "src/a.rs");
    assert_eq!(repository_path("my dir/a b.rs").unwrap(), "my dir/a b.rs");
}

#[test]
fn a_path_that_cannot_be_right_is_refused_naming_it() {
    for path in [
        "",
        ".",
        "./",
        "../a.rs",
        "src/../../a.rs",
        "/etc/passwd",
        ".git/config",
        "src/.git/hooks/pre-commit",
        "a\0b",
    ] {
        let error = repository_path(path).unwrap_err();
        assert!(
            matches!(&error, ActionError::BadArgument(text) if text.contains(&format!("`{path}`"))),
            "{path:?}: {error:?}"
        );
    }
}

#[test]
fn a_bad_path_is_refused_before_git_is_asked() {
    let git = Git::with_program("trcode-no-such-git-program", Duration::from_secs(1));

    let error = git.stage(Path::new("."), "../outside").unwrap_err();

    assert!(matches!(error, ActionError::BadArgument(_)), "{error:?}");
}

fn staged_and_unstaged(status: &Status, path: &str) -> Option<Change> {
    status
        .entries
        .iter()
        .find(|entry| entry.path == Path::new(path))
        .map(|entry| entry.change)
}

#[test]
fn staging_moves_a_new_file_into_the_staged_changes_and_unstaging_puts_it_back() {
    let dir = repository();
    let root = canonical(dir.path());
    std::fs::write(root.join("new.txt"), "x\n").unwrap();
    let git = Git::default();

    git.stage(&root, "new.txt").unwrap();
    let staged = git.status(&root).unwrap();
    git.unstage(&root, "new.txt").unwrap();
    let unstaged = git.status(&root).unwrap();

    assert_eq!(
        staged_and_unstaged(&staged, "new.txt"),
        Some(tracked(Some(Kind::Added), None))
    );
    assert_eq!(
        staged_and_unstaged(&unstaged, "new.txt"),
        Some(Change::Untracked)
    );
}

#[test]
fn staging_a_changed_file_and_unstaging_it() {
    let dir = repository();
    let root = canonical(dir.path());
    std::fs::write(root.join("kept.txt"), "two\n").unwrap();
    let git = Git::default();

    git.stage(&root, "kept.txt").unwrap();
    assert_eq!(
        staged_and_unstaged(&git.status(&root).unwrap(), "kept.txt"),
        Some(tracked(Some(Kind::Modified), None))
    );
    git.unstage(&root, "kept.txt").unwrap();

    assert_eq!(
        staged_and_unstaged(&git.status(&root).unwrap(), "kept.txt"),
        Some(tracked(None, Some(Kind::Modified))),
        "the change in the working folder is kept"
    );
}

#[test]
fn staging_a_deleted_file_stages_its_deletion() {
    let dir = repository();
    let root = canonical(dir.path());
    std::fs::remove_file(root.join("kept.txt")).unwrap();

    Git::default().stage(&root, "kept.txt").unwrap();

    assert_eq!(
        staged_and_unstaged(&Git::default().status(&root).unwrap(), "kept.txt"),
        Some(tracked(Some(Kind::Deleted), None))
    );
}

#[test]
fn unstaging_works_before_the_first_commit() {
    let dir = tempfile::tempdir().unwrap();
    let root = canonical(dir.path());
    git(&root, &["init", "-q", "-b", "main"]);
    std::fs::write(root.join("first.txt"), "x\n").unwrap();
    let tool = Git::default();
    tool.stage(&root, "first.txt").unwrap();

    tool.unstage(&root, "first.txt").unwrap();

    assert_eq!(
        staged_and_unstaged(&tool.status(&root).unwrap(), "first.txt"),
        Some(Change::Untracked)
    );
}

// `*` and `:` cannot be in a file name on Windows.
#[cfg(unix)]
#[test]
fn a_name_that_looks_like_a_pattern_is_only_a_name() {
    let dir = repository();
    let root = canonical(dir.path());
    std::fs::write(root.join("a*b.txt"), "x\n").unwrap();
    std::fs::write(root.join("axb.txt"), "y\n").unwrap();
    std::fs::write(root.join(":(top)c.txt"), "z\n").unwrap();
    let tool = Git::default();

    tool.stage(&root, "a*b.txt").unwrap();
    tool.stage(&root, ":(top)c.txt").unwrap();

    let status = tool.status(&root).unwrap();
    assert_eq!(
        staged_and_unstaged(&status, "a*b.txt"),
        Some(tracked(Some(Kind::Added), None))
    );
    assert_eq!(
        staged_and_unstaged(&status, "axb.txt"),
        Some(Change::Untracked),
        "the pattern did not match another file"
    );
    assert_eq!(
        staged_and_unstaged(&status, ":(top)c.txt"),
        Some(tracked(Some(Kind::Added), None))
    );
}

#[test]
fn a_file_in_a_folder_with_spaces_can_be_staged() {
    let dir = repository();
    let root = canonical(dir.path());
    std::fs::create_dir_all(root.join("my dir")).unwrap();
    std::fs::write(root.join("my dir/a b.txt"), "x\n").unwrap();

    Git::default().stage(&root, "my dir/a b.txt").unwrap();

    assert_eq!(
        staged_and_unstaged(&Git::default().status(&root).unwrap(), "my dir/a b.txt"),
        Some(tracked(Some(Kind::Added), None))
    );
}

#[test]
fn what_git_says_when_it_cannot_is_in_the_error() {
    let dir = repository();
    let root = canonical(dir.path());

    let error = Git::default().stage(&root, "no-such-file.txt").unwrap_err();

    let ActionError::Git(GitError::Failed { command, detail }) = &error else {
        panic!("{error:?}");
    };
    assert_eq!(command, "add");
    assert!(detail.contains("no-such-file.txt"), "{detail}");
}

#[test]
fn a_leftover_index_lock_fails_a_change_with_gits_words_and_not_a_read() {
    // A leftover `index.lock` makes `git add` fail, and the error says why.
    let dir = repository();
    let root = canonical(dir.path());
    std::fs::write(root.join("new.txt"), "x\n").unwrap();
    std::fs::write(root.join(".git/index.lock"), "").unwrap();

    let error = Git::default().stage(&root, "new.txt").unwrap_err();

    let ActionError::Git(GitError::Failed { detail, .. }) = &error else {
        panic!("{error:?}");
    };
    assert!(detail.contains("index.lock"), "{detail}");
    // And a read is not stopped by it.
    assert!(Git::default().status(&root).is_ok());
}

// --- stage all, unstage all, commit ---

/// A repository that may commit: the identity is in its own config, so the test
/// does not depend on the machine's.
fn committer() -> tempfile::TempDir {
    let dir = repository();
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    dir
}

fn log(dir: &Path) -> Vec<String> {
    let output = Command::new("git")
        .args(["log", "--format=%s"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn stage_all_stages_new_changed_and_deleted_files_in_every_folder() {
    let dir = committer();
    let root = canonical(dir.path());
    std::fs::create_dir(root.join("sub")).unwrap();
    std::fs::write(root.join("sub/new.txt"), "n\n").unwrap();
    std::fs::write(root.join("kept.txt"), "changed\n").unwrap();
    std::fs::write(root.join("top.txt"), "t\n").unwrap();
    let tool = Git::default();

    tool.stage_all(&root).unwrap();

    let status = tool.status(&root).unwrap();
    for (path, expected) in [
        ("sub/new.txt", tracked(Some(Kind::Added), None)),
        ("top.txt", tracked(Some(Kind::Added), None)),
        ("kept.txt", tracked(Some(Kind::Modified), None)),
    ] {
        assert_eq!(staged_and_unstaged(&status, path), Some(expected), "{path}");
    }
}

#[test]
fn stage_all_stages_a_deletion() {
    let dir = committer();
    let root = canonical(dir.path());
    std::fs::remove_file(root.join("kept.txt")).unwrap();

    Git::default().stage_all(&root).unwrap();

    let status = Git::default().status(&root).unwrap();
    assert_eq!(
        staged_and_unstaged(&status, "kept.txt"),
        Some(tracked(Some(Kind::Deleted), None))
    );
}

#[test]
fn unstage_all_keeps_the_files_and_unstages_everything() {
    let dir = committer();
    let root = canonical(dir.path());
    std::fs::write(root.join("a.txt"), "a\n").unwrap();
    std::fs::write(root.join("kept.txt"), "changed\n").unwrap();
    let tool = Git::default();
    tool.stage_all(&root).unwrap();

    tool.unstage_all(&root).unwrap();

    let status = tool.status(&root).unwrap();
    assert_eq!(
        staged_and_unstaged(&status, "kept.txt"),
        Some(tracked(None, Some(Kind::Modified)))
    );
    assert_eq!(
        staged_and_unstaged(&status, "a.txt"),
        Some(Change::Untracked)
    );
    assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "a\n");
}

#[test]
fn unstage_all_works_before_the_first_commit() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    let root = canonical(dir.path());
    std::fs::write(root.join("a.txt"), "a\n").unwrap();
    let tool = Git::default();
    tool.stage_all(&root).unwrap();

    tool.unstage_all(&root).unwrap();

    let status = tool.status(&root).unwrap();
    assert_eq!(
        staged_and_unstaged(&status, "a.txt"),
        Some(Change::Untracked)
    );
}

#[test]
fn a_commit_records_what_is_staged_with_the_message_and_leaves_the_rest() {
    let dir = committer();
    let root = canonical(dir.path());
    std::fs::write(root.join("kept.txt"), "changed\n").unwrap();
    std::fs::write(root.join("other.txt"), "o\n").unwrap();
    let tool = Git::default();
    tool.stage(&root, "kept.txt").unwrap();

    tool.commit(&root, "Change kept").unwrap();

    assert_eq!(log(&root), ["Change kept", "first"]);
    let status = tool.status(&root).unwrap();
    assert_eq!(staged_and_unstaged(&status, "kept.txt"), None, "committed");
    assert_eq!(
        staged_and_unstaged(&status, "other.txt"),
        Some(Change::Untracked)
    );
}

#[test]
fn a_message_is_only_a_message_whatever_it_looks_like() {
    let dir = committer();
    let root = canonical(dir.path());
    let tool = Git::default();
    for (n, message) in ["--amend", "-m \"quoted\" and $HOME `x`", "  padded  "]
        .into_iter()
        .enumerate()
    {
        std::fs::write(root.join("kept.txt"), format!("{n}\n")).unwrap();
        tool.stage(&root, "kept.txt").unwrap();

        tool.commit(&root, message).unwrap();

        assert_eq!(log(&root)[0], message.trim());
    }
    assert_eq!(log(&root).len(), 4, "each made a commit of its own");
}

#[test]
fn an_empty_message_is_refused_and_nothing_is_committed() {
    let dir = committer();
    let root = canonical(dir.path());
    std::fs::write(root.join("kept.txt"), "changed\n").unwrap();
    let tool = Git::default();
    tool.stage(&root, "kept.txt").unwrap();

    for message in ["", "   ", "\n\t\n"] {
        let error = tool.commit(&root, message).unwrap_err();
        assert_eq!(
            error,
            ActionError::BadArgument("the commit message is empty".to_owned())
        );
    }
    assert!(matches!(
        tool.commit(&root, "a\0b"),
        Err(ActionError::BadArgument(_))
    ));
    assert_eq!(log(&root), ["first"]);
}

#[test]
fn a_commit_with_nothing_staged_says_so_even_when_files_changed() {
    let dir = committer();
    let root = canonical(dir.path());
    std::fs::write(root.join("kept.txt"), "changed but not staged\n").unwrap();

    let error = Git::default().commit(&root, "nothing").unwrap_err();

    assert_eq!(error, ActionError::NothingStaged);
    assert_eq!(log(&root), ["first"]);
}

#[cfg(unix)]
#[test]
fn a_hook_that_says_no_fails_the_commit_with_what_it_printed() {
    use std::os::unix::fs::PermissionsExt;
    let dir = committer();
    let root = canonical(dir.path());
    let hook = root.join(".git/hooks/pre-commit");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'lint: tabs are not allowed' >&2\necho 'fix them first'\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(root.join("kept.txt"), "changed\n").unwrap();
    let tool = Git::default();
    tool.stage(&root, "kept.txt").unwrap();

    let error = tool.commit(&root, "rejected").unwrap_err();

    let ActionError::Git(GitError::Failed { command, detail }) = error else {
        panic!("a failure of git");
    };
    assert_eq!(command, "commit");
    assert!(detail.contains("lint: tabs are not allowed"), "{detail}");
    assert!(detail.contains("fix them first"), "{detail}");
    assert_eq!(log(&root), ["first"], "nothing was committed");
}

#[test]
fn what_a_failed_command_printed_is_cut_short() {
    let long = "x".repeat(2000);
    let outcome = super::run::Outcome {
        success: false,
        code: Some(1),
        stdout: Vec::new(),
        stderr: format!("{long}\nsecond\n").into_bytes(),
    };
    let text = super::actions::output_of(&outcome);
    assert!(text.chars().count() <= 603, "{}", text.len());
    assert!(text.ends_with("..."));

    let many: String = (0..30).map(|n| format!("line {n}\n")).collect();
    let outcome = super::run::Outcome {
        stderr: many.into_bytes(),
        ..outcome
    };
    let text = super::actions::output_of(&outcome);
    assert!(
        text.contains("line 11") && !text.contains("line 12"),
        "{text}"
    );

    let silent = super::run::Outcome {
        stderr: Vec::new(),
        ..outcome
    };
    assert_eq!(super::actions::output_of(&silent), "it said nothing");
}
