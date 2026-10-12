use std::fs;
use std::path::{Path, PathBuf};

use crate::buffer::{Buffer, Position};
use crate::components::editor::{Editor, Motion};
use crate::components::workspace::{CloseMany, CloseResult, CloseScope, SaveOutcome, Workspace};
use crate::ui::layout::Axis;

fn fresh(text: &str) -> Workspace {
    Workspace::new(Editor::new(Buffer::from_text(text)))
}

fn open(workspace: &mut Workspace, text: &str) {
    workspace.open_buffer(Buffer::from_text(text));
}

fn file(dir: &tempfile::TempDir, name: &str, text: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, text).unwrap();
    path
}

fn names(workspace: &Workspace) -> Vec<String> {
    workspace.tab_names().0
}

fn type_in(workspace: &mut Workspace, text: &str) {
    workspace.with_editor(|editor| editor.insert_text(text).unwrap());
}

fn text(workspace: &Workspace) -> String {
    workspace.active_editor().buffer().text()
}

/// A workspace with the files `a`, `b` and `c` open, `c` showing last.
fn three_files(dir: &tempfile::TempDir) -> Workspace {
    let mut workspace = Workspace::new(Editor::default());
    for name in ["a.txt", "b.txt", "c.txt"] {
        workspace.open_path(&file(dir, name, name)).unwrap();
    }
    // The empty first tab goes, so the three are all there is.
    workspace.activate_tab(0);
    assert_eq!(workspace.close_tab(), CloseResult::Closed);
    workspace.activate_tab(2);
    workspace
}

// ---- closing the other tabs

#[test]
fn close_others_keeps_the_active_tab() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    workspace.activate_tab(1);

    let result = workspace.close_tabs(CloseScope::Others);

    assert_eq!(result, CloseMany::Closed(2));
    assert_eq!(names(&workspace), ["b.txt"]);
    assert_eq!(workspace.document_count(), 1);
}

#[test]
fn close_others_with_one_tab_closes_nothing() {
    let mut workspace = fresh("only");

    assert_eq!(
        workspace.close_tabs(CloseScope::Others),
        CloseMany::Closed(0)
    );
    assert_eq!(text(&workspace), "only");
}

#[test]
fn close_others_leaves_the_other_panes_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    workspace.split(Axis::Horizontal);
    open(&mut workspace, "extra");

    workspace.close_tabs(CloseScope::Others);

    // The right pane keeps only its showing tab; the left pane still has all three.
    assert_eq!(workspace.pane_count(), 2);
    assert_eq!(names(&workspace).len(), 1);
    workspace.focus_next_pane();
    assert_eq!(names(&workspace).len(), 3);
}

#[test]
fn close_others_does_not_ask_about_the_tab_that_stays() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    type_in(&mut workspace, "x");

    assert_eq!(
        workspace.close_tabs(CloseScope::Others),
        CloseMany::Closed(2)
    );
    assert_eq!(text(&workspace), "xc.txt");
}

#[test]
fn close_others_asks_before_losing_changes_and_then_does_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    workspace.activate_tab(0);
    type_in(&mut workspace, "x");
    workspace.activate_tab(2);

    let first = workspace.close_tabs(CloseScope::Others);
    assert_eq!(first, CloseMany::Unsaved(vec!["a.txt [+]".to_owned()]));
    assert_eq!(names(&workspace).len(), 3);

    assert_eq!(
        workspace.close_tabs(CloseScope::Others),
        CloseMany::Closed(2)
    );
    assert_eq!(names(&workspace), ["c.txt"]);
}

#[test]
fn anything_in_between_cancels_the_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    workspace.activate_tab(0);
    type_in(&mut workspace, "x");
    workspace.activate_tab(2);
    workspace.close_tabs(CloseScope::Others);

    workspace.previous_tab();
    workspace.next_tab();

    assert!(matches!(
        workspace.close_tabs(CloseScope::Others),
        CloseMany::Unsaved(_)
    ));
    assert_eq!(names(&workspace).len(), 3);
}

#[test]
fn confirming_one_scope_does_not_confirm_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    workspace.activate_tab(2);
    type_in(&mut workspace, "x");

    // Closing all would lose the active tab's changes; closing the others would not.
    assert!(matches!(
        workspace.close_tabs(CloseScope::All),
        CloseMany::Unsaved(_)
    ));
    assert_eq!(
        workspace.close_tabs(CloseScope::Others),
        CloseMany::Closed(2)
    );
}

#[test]
fn a_changed_document_that_another_tab_still_shows_is_not_lost() {
    let mut workspace = fresh("shared");
    type_in(&mut workspace, "x");
    workspace.split(Axis::Horizontal);
    open(&mut workspace, "other");
    workspace.activate_tab(1);

    // The right pane has the shared document and another one; closing the
    // other tabs there closes only the clean one.
    assert_eq!(
        workspace.close_tabs(CloseScope::Others),
        CloseMany::Closed(1)
    );
}

// ---- closing everything

#[test]
fn close_all_leaves_no_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);

    assert_eq!(workspace.close_tabs(CloseScope::All), CloseMany::Closed(3));

    assert!(!workspace.has_tabs());
    assert_eq!(workspace.document_count(), 0);
}

#[test]
fn close_all_reaches_every_pane() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    workspace.split(Axis::Horizontal);
    open(&mut workspace, "extra");

    let result = workspace.close_tabs(CloseScope::All);

    assert!(
        matches!(result, CloseMany::Closed(n) if n >= 4),
        "{result:?}"
    );
    assert!(!workspace.has_tabs());
    assert_eq!(workspace.pane_count(), 1);
}

#[test]
fn close_all_names_every_document_that_would_lose_changes_in_the_order_opened() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    for index in [2, 0] {
        workspace.activate_tab(index);
        type_in(&mut workspace, "x");
    }

    let result = workspace.close_tabs(CloseScope::All);

    assert_eq!(
        result,
        CloseMany::Unsaved(vec!["a.txt [+]".to_owned(), "c.txt [+]".to_owned()])
    );
    assert_eq!(names(&workspace).len(), 3);
}

#[test]
fn close_all_with_nothing_open_does_nothing() {
    let mut workspace = fresh("x");
    workspace.close_tabs(CloseScope::All);

    assert_eq!(workspace.close_tabs(CloseScope::All), CloseMany::Closed(0));
}

// ---- reopening

#[test]
fn a_closed_tab_comes_back_with_its_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    workspace.activate_tab(1);
    workspace
        .with_editor(|editor| editor.move_cursor(Motion::LineEnd))
        .unwrap();
    workspace.close_tab();

    let reopened = workspace.reopen_closed_tab();

    assert_eq!(reopened.unwrap().unwrap(), dir.path().join("b.txt"));
    assert_eq!(text(&workspace), "b.txt");
    assert_eq!(
        workspace.active_editor().selections().primary().head(),
        Position::new(0, 5)
    );
}

#[test]
fn tabs_come_back_latest_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    workspace.close_tabs(CloseScope::All);

    // The tabs closed in order a, b, c; c is the latest.
    let first = workspace.reopen_closed_tab().unwrap().unwrap();
    let second = workspace.reopen_closed_tab().unwrap().unwrap();
    let third = workspace.reopen_closed_tab().unwrap().unwrap();

    assert_eq!(
        [first, second, third],
        [
            dir.path().join("c.txt"),
            dir.path().join("b.txt"),
            dir.path().join("a.txt")
        ]
    );
    assert!(workspace.reopen_closed_tab().is_none());
}

#[test]
fn with_nothing_closed_there_is_nothing_to_reopen() {
    let mut workspace = fresh("x");
    assert!(workspace.reopen_closed_tab().is_none());
}

#[test]
fn a_file_that_is_gone_is_an_error_and_is_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    workspace.close_tab();
    fs::remove_file(dir.path().join("c.txt")).unwrap();

    assert!(workspace.reopen_closed_tab().unwrap().is_err());
    assert!(workspace.reopen_closed_tab().is_none());
}

#[test]
fn an_untitled_tab_is_not_remembered() {
    let mut workspace = fresh("scratch");
    open(&mut workspace, "more");

    workspace.close_tab();

    assert_eq!(workspace.closed_tab_count(), 0);
}

#[test]
fn closing_one_of_two_views_of_a_file_remembers_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = Workspace::new(Editor::default());
    workspace.open_path(&file(&dir, "a.txt", "a")).unwrap();
    workspace.split(Axis::Horizontal);

    workspace.close_tab();

    assert_eq!(workspace.closed_tab_count(), 0);
}

#[test]
fn closing_the_same_file_twice_is_remembered_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = file(&dir, "a.txt", "a");
    let mut workspace = Workspace::new(Editor::default());
    for _ in 0..2 {
        workspace.open_path(&path).unwrap();
        workspace.close_tab();
    }

    assert_eq!(workspace.closed_tab_count(), 1);
}

#[test]
fn only_the_last_twenty_closed_tabs_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = Workspace::new(Editor::default());
    for number in 0..25 {
        let path = file(&dir, &format!("f{number}.txt"), "x");
        workspace.open_path(&path).unwrap();
        workspace.close_tab();
    }

    assert_eq!(workspace.closed_tab_count(), 20);
    let latest = workspace.reopen_closed_tab().unwrap().unwrap();
    assert_eq!(latest, dir.path().join("f24.txt"));
}

#[test]
fn a_file_reopened_by_hand_meanwhile_is_just_switched_to() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    let c = dir.path().join("c.txt");
    workspace.close_tab();
    workspace.open_path(&c).unwrap();
    let count = names(&workspace).len();

    workspace.reopen_closed_tab().unwrap().unwrap();

    assert_eq!(names(&workspace).len(), count);
}

#[test]
fn files_closed_because_they_were_deleted_are_not_remembered() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    let documents = workspace.documents_under(dir.path());

    workspace.close_documents(&documents);

    assert_eq!(workspace.closed_tab_count(), 0);
}

#[test]
fn what_was_typed_is_not_brought_back_only_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);
    type_in(&mut workspace, "x");
    // The first close asks, the second discards.
    workspace.close_tab();
    workspace.close_tab();

    workspace.reopen_closed_tab().unwrap().unwrap();

    assert_eq!(text(&workspace), "c.txt");
}

// ---- saving everything

fn dirty_open(workspace: &mut Workspace, path: &Path, typed: &str) {
    workspace.open_path(path).unwrap();
    type_in(workspace, typed);
}

#[test]
fn save_all_writes_every_modified_file_and_only_those() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b, c) = (
        file(&dir, "a.txt", "a"),
        file(&dir, "b.txt", "b"),
        file(&dir, "c.txt", "c"),
    );
    let mut workspace = Workspace::new(Editor::default());
    dirty_open(&mut workspace, &a, "1");
    workspace.open_path(&b).unwrap();
    dirty_open(&mut workspace, &c, "3");

    let report = workspace.save_all();

    assert_eq!(report.saved, 2);
    assert_eq!(fs::read_to_string(a).unwrap(), "1a");
    assert_eq!(fs::read_to_string(b).unwrap(), "b");
    assert_eq!(fs::read_to_string(c).unwrap(), "3c");
    assert!(workspace.dirty_documents().is_empty());
}

#[test]
fn save_all_with_nothing_modified_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = three_files(&dir);

    assert_eq!(workspace.save_all(), Default::default());
}

#[test]
fn save_all_leaves_untitled_documents_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir, "a.txt", "a");
    let mut workspace = fresh("scratch");
    type_in(&mut workspace, "x");
    dirty_open(&mut workspace, &a, "1");

    let report = workspace.save_all();

    assert_eq!(report.saved, 1);
    assert_eq!(report.untitled, ["[no name] [+]"]);
    assert_eq!(workspace.dirty_documents().len(), 1);
}

#[test]
fn save_all_skips_a_file_that_changed_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (file(&dir, "a.txt", "a"), file(&dir, "b.txt", "b"));
    let mut workspace = Workspace::new(Editor::default());
    dirty_open(&mut workspace, &a, "1");
    dirty_open(&mut workspace, &b, "2");
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(&a, "someone else").unwrap();

    let report = workspace.save_all();

    assert_eq!(report.saved, 1);
    assert_eq!(report.changed_on_disk, ["a.txt"]);
    assert_eq!(fs::read_to_string(a).unwrap(), "someone else");
    assert_eq!(fs::read_to_string(b).unwrap(), "2b");
}

#[test]
fn save_all_does_not_leave_an_overwrite_armed() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir, "a.txt", "a");
    let mut workspace = Workspace::new(Editor::default());
    dirty_open(&mut workspace, &a, "1");
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(&a, "someone else").unwrap();
    workspace.save_all();

    // The first save of the file on its own still asks.
    let outcome = workspace.save_active().unwrap();

    assert!(
        matches!(outcome, SaveOutcome::NeedsConfirmation(_)),
        "{outcome:?}"
    );
    assert_eq!(fs::read_to_string(a).unwrap(), "someone else");
}

#[test]
fn save_all_reports_a_file_that_cannot_be_written_and_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("sub");
    fs::create_dir(&gone).unwrap();
    let doomed = file_in(&gone, "x.txt");
    let b = file(&dir, "b.txt", "b");
    let mut workspace = Workspace::new(Editor::default());
    dirty_open(&mut workspace, &doomed, "1");
    dirty_open(&mut workspace, &b, "2");
    fs::remove_dir_all(&gone).unwrap();

    let report = workspace.save_all();

    assert_eq!(report.saved, 1);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, "x.txt [+]");
    assert_eq!(fs::read_to_string(b).unwrap(), "2b");
}

fn file_in(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, "x").unwrap();
    path
}

// ---- recent files and opening in a split

#[test]
fn opened_files_are_remembered_latest_first_without_repeats() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (file(&dir, "a.txt", "a"), file(&dir, "b.txt", "b"));
    let mut workspace = Workspace::new(Editor::default());

    for path in [&a, &b, &a] {
        workspace.open_path(path).unwrap();
    }

    assert_eq!(workspace.recent_paths(), [a, b]);
}

#[test]
fn a_file_that_could_not_be_opened_is_not_remembered() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = Workspace::new(Editor::default());

    assert!(
        workspace
            .open_path(&dir.path().join("missing.txt"))
            .is_err()
    );

    assert!(workspace.recent_paths().is_empty());
}

#[test]
fn only_the_last_twenty_opened_files_are_remembered() {
    let dir = tempfile::tempdir().unwrap();
    let mut workspace = Workspace::new(Editor::default());
    for number in 0..25 {
        let path = file(&dir, &format!("f{number}.txt"), "x");
        workspace.open_path(&path).unwrap();
    }

    assert_eq!(workspace.recent_paths().len(), 20);
    assert_eq!(workspace.recent_paths()[0], dir.path().join("f24.txt"));
}

#[test]
fn opening_in_a_split_puts_the_file_alone_in_a_new_focused_pane() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir, "a.txt", "a");
    let b = file(&dir, "b.txt", "b");
    let mut workspace = Workspace::new(Editor::default());
    workspace.open_path(&a).unwrap();

    workspace.open_path_in_split(&b, Axis::Horizontal).unwrap();

    assert_eq!(workspace.pane_count(), 2);
    assert_eq!(names(&workspace), ["b.txt"]);
    assert_eq!(text(&workspace), "b");
    workspace.focus_next_pane();
    assert!(names(&workspace).contains(&"a.txt".to_owned()));
}

#[test]
fn opening_in_a_split_a_file_that_is_open_shares_the_document() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir, "a.txt", "a");
    let mut workspace = Workspace::new(Editor::default());
    workspace.open_path(&a).unwrap();
    let documents = workspace.document_count();

    workspace.open_path_in_split(&a, Axis::Horizontal).unwrap();

    assert_eq!(workspace.document_count(), documents);
    assert_eq!(names(&workspace), ["a.txt"]);
    type_in(&mut workspace, "x");
    workspace.focus_next_pane();
    assert!(
        text(&workspace).contains('x'),
        "the two panes show one buffer"
    );
}

#[test]
fn opening_in_a_split_a_file_that_cannot_be_read_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir, "a.txt", "a");
    let mut workspace = Workspace::new(Editor::default());
    workspace.open_path(&a).unwrap();

    let result = workspace.open_path_in_split(&dir.path().join("gone.txt"), Axis::Horizontal);

    assert!(result.is_err());
    assert_eq!(workspace.pane_count(), 1);
    assert_eq!(
        workspace.document_count(),
        2,
        "the empty first tab and a.txt"
    );
}

#[test]
fn opening_in_a_split_with_no_tab_just_opens() {
    let dir = tempfile::tempdir().unwrap();
    let a = file(&dir, "a.txt", "a");
    let mut workspace = Workspace::new(Editor::default());
    workspace.close_tabs(CloseScope::All);

    workspace.open_path_in_split(&a, Axis::Horizontal).unwrap();

    assert_eq!(workspace.pane_count(), 1);
    assert_eq!(names(&workspace), ["a.txt"]);
}

#[test]
fn opening_in_a_split_a_second_time_makes_a_third_pane() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b, c) = (
        file(&dir, "a.txt", "a"),
        file(&dir, "b.txt", "b"),
        file(&dir, "c.txt", "c"),
    );
    let mut workspace = Workspace::new(Editor::default());
    workspace.open_path(&a).unwrap();

    workspace.open_path_in_split(&b, Axis::Horizontal).unwrap();
    workspace.open_path_in_split(&c, Axis::Horizontal).unwrap();

    assert_eq!(workspace.pane_count(), 3);
    assert_eq!(names(&workspace), ["c.txt"]);
}

#[test]
fn save_all_leaves_a_deleted_file_alone_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (file(&dir, "a.txt", "a"), file(&dir, "b.txt", "b"));
    let mut workspace = Workspace::new(Editor::default());
    dirty_open(&mut workspace, &a, "1");
    dirty_open(&mut workspace, &b, "2");
    fs::remove_file(&a).unwrap();

    let report = workspace.save_all();

    assert_eq!(report.saved, 1);
    assert!(!a.exists());
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, "a.txt");
    assert!(
        report.failed[0].1.contains("deleted"),
        "{:?}",
        report.failed
    );
}
