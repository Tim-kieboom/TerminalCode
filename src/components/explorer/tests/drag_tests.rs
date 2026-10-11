//! Pressing, clicking and dragging rows with the mouse.

use std::fs;
use std::path::{Path, PathBuf};

use ratatui::layout::Rect;

use crate::components::ComponentKind;
use crate::components::explorer::{Explorer, ExplorerCommand, Release};
use crate::ui::Render;
use crate::ui::layout::Placement;

/// The project folder with `a.txt`, `docs/`, `src/main.rs` and `src/util/`.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/util")).unwrap();
    fs::create_dir(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("a.txt"), "").unwrap();
    fs::write(dir.path().join("src/main.rs"), "").unwrap();
    fs::write(dir.path().join("src/util/mod.rs"), "").unwrap();
    dir
}

/// An explorer on `dir` laid out in 40 x 20 cells, with `src` open.
fn open_explorer(dir: &tempfile::TempDir) -> Explorer {
    let mut explorer = Explorer::open(dir.path()).unwrap();
    layout(&mut explorer);
    explorer.apply(ExplorerCommand::Down).unwrap();
    while explorer.selected_row().unwrap().name != "src" {
        explorer.apply(ExplorerCommand::Down).unwrap();
    }
    explorer.apply(ExplorerCommand::Expand).unwrap();
    layout(&mut explorer);
    explorer
}

fn layout(explorer: &mut Explorer) {
    let placement = Placement::new(ComponentKind::Explorer, Rect::new(0, 0, 40, 20));
    explorer.prepare(&placement);
}

/// The screen cell of the row called `name`.
fn cell(explorer: &Explorer, name: &str) -> (u16, u16) {
    let index = explorer
        .rows()
        .iter()
        .position(|row| row.name == name)
        .unwrap_or_else(|| panic!("no row {name}"));
    let body = explorer.body_for_tests();
    (body.x + 2, body.y + index as u16)
}

fn press_on(explorer: &mut Explorer, name: &str) {
    let (column, row) = cell(explorer, name);
    assert!(explorer.press(column, row));
}

fn drag_over(explorer: &mut Explorer, name: &str) {
    let (column, row) = cell(explorer, name);
    explorer.drag_to(column, row);
}

fn release_on(explorer: &mut Explorer, name: &str) -> Release {
    let (column, row) = cell(explorer, name);
    explorer.release(column, row).unwrap()
}

fn path(dir: &tempfile::TempDir, relative: &str) -> PathBuf {
    dir.path().join(relative)
}

fn names(explorer: &Explorer) -> Vec<String> {
    explorer.rows().iter().map(|row| row.name.clone()).collect()
}

fn the_folder(release: &Release) -> Option<&Path> {
    match release {
        Release::Move { into, .. } => Some(into),
        _ => None,
    }
}

// ---- clicks

#[test]
fn pressing_a_row_selects_it_and_opens_nothing() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    let before = names(&explorer);

    press_on(&mut explorer, "docs");

    assert_eq!(explorer.selected_row().unwrap().name, "docs");
    assert_eq!(names(&explorer), before);
    assert!(explorer.is_pressed());
    assert!(!explorer.is_dragging());
}

#[test]
fn letting_go_on_a_file_opens_it() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "a.txt");

    let release = release_on(&mut explorer, "a.txt");

    assert_eq!(release, Release::Open(path(&dir, "a.txt")));
    assert!(!explorer.is_pressed());
}

#[test]
fn letting_go_on_a_folder_toggles_it() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    assert!(explorer.rows().iter().any(|row| row.name == "main.rs"));
    press_on(&mut explorer, "src");

    let release = release_on(&mut explorer, "src");

    assert_eq!(release, Release::Nothing);
    assert!(!explorer.rows().iter().any(|row| row.name == "main.rs"));
}

#[test]
fn a_press_outside_the_rows_selects_nothing_and_a_release_does_nothing() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    let selected = explorer.selected();
    let body = explorer.body_for_tests();

    assert!(!explorer.press(body.x + 2, body.bottom() - 1));
    let release = explorer.release(body.x + 2, body.bottom() - 1).unwrap();

    assert_eq!(release, Release::Nothing);
    assert_eq!(explorer.selected(), selected);
}

#[test]
fn a_release_without_a_press_does_nothing() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    assert_eq!(release_on(&mut explorer, "a.txt"), Release::Nothing);
}

#[test]
fn moving_inside_the_pressed_row_is_still_a_click() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "a.txt");
    let (column, row) = cell(&explorer, "a.txt");

    explorer.drag_to(column + 5, row);
    assert!(!explorer.is_dragging());

    let release = explorer.release(column + 5, row).unwrap();
    assert_eq!(release, Release::Open(path(&dir, "a.txt")));
}

// ---- drags

#[test]
fn leaving_the_pressed_row_starts_a_drag() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "a.txt");

    drag_over(&mut explorer, "docs");

    assert!(explorer.is_dragging());
    assert_eq!(explorer.drop_target(), Some(path(&dir, "docs").as_path()));
}

#[test]
fn dropping_on_a_folder_moves_the_row_into_it() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "a.txt");
    drag_over(&mut explorer, "docs");

    let release = release_on(&mut explorer, "docs");

    assert_eq!(
        release,
        Release::Move {
            from: path(&dir, "a.txt"),
            into: path(&dir, "docs"),
        }
    );
    assert!(!explorer.is_dragging());
    assert!(!explorer.is_pressed());
}

#[test]
fn a_drag_opens_and_closes_nothing() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    let before = names(&explorer);
    press_on(&mut explorer, "a.txt");
    drag_over(&mut explorer, "docs");
    drag_over(&mut explorer, "src");
    drag_over(&mut explorer, "util");

    assert_eq!(names(&explorer), before);
}

#[test]
fn dropping_on_a_file_means_the_folder_it_is_in() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "a.txt");
    drag_over(&mut explorer, "main.rs");

    assert_eq!(explorer.drop_target(), Some(path(&dir, "src").as_path()));
    let release = release_on(&mut explorer, "main.rs");
    assert_eq!(the_folder(&release), Some(path(&dir, "src").as_path()));
}

#[test]
fn dropping_on_empty_space_means_the_project_folder() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "main.rs");
    let body = explorer.body_for_tests();

    explorer.drag_to(body.x + 2, body.bottom() - 2);
    let release = explorer.release(body.x + 2, body.bottom() - 2).unwrap();

    assert_eq!(
        release,
        Release::Move {
            from: path(&dir, "src/main.rs"),
            into: dir.path().to_path_buf(),
        }
    );
}

#[test]
fn dropping_where_the_row_already_is_does_nothing() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "main.rs");

    // Onto its own folder, and onto itself.
    drag_over(&mut explorer, "src");
    assert_eq!(explorer.drop_target(), None);
    drag_over(&mut explorer, "main.rs");
    assert_eq!(explorer.drop_target(), None);
    assert_eq!(release_on(&mut explorer, "src"), Release::Nothing);
}

#[test]
fn a_folder_cannot_be_dropped_into_itself_or_what_it_holds() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "src");

    drag_over(&mut explorer, "util");
    assert_eq!(explorer.drop_target(), None);
    drag_over(&mut explorer, "main.rs");
    assert_eq!(explorer.drop_target(), None);

    assert_eq!(release_on(&mut explorer, "util"), Release::Nothing);
}

#[test]
fn a_folder_can_be_dropped_on_another_folder() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "util");
    drag_over(&mut explorer, "docs");

    let release = release_on(&mut explorer, "docs");

    assert_eq!(
        release,
        Release::Move {
            from: path(&dir, "src/util"),
            into: path(&dir, "docs"),
        }
    );
}

#[test]
fn the_target_follows_the_pointer_and_can_be_lost() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "a.txt");

    drag_over(&mut explorer, "docs");
    assert_eq!(explorer.drop_target(), Some(path(&dir, "docs").as_path()));
    drag_over(&mut explorer, "src");
    assert_eq!(explorer.drop_target(), Some(path(&dir, "src").as_path()));
    // Out of the explorer altogether.
    explorer.drag_to(60, 5);
    assert_eq!(explorer.drop_target(), None);
    assert!(explorer.is_dragging());
}

#[test]
fn letting_go_outside_the_explorer_cancels() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "a.txt");
    drag_over(&mut explorer, "docs");

    let release = explorer.release(60, 5).unwrap();

    assert_eq!(release, Release::Nothing);
    assert!(!explorer.is_dragging());
}

#[test]
fn cancelling_a_drag_drops_nothing() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "a.txt");
    drag_over(&mut explorer, "docs");

    explorer.cancel_drag();

    assert!(!explorer.is_dragging());
    assert!(!explorer.is_pressed());
    assert_eq!(release_on(&mut explorer, "docs"), Release::Nothing);
}

#[test]
fn a_terminal_without_drag_events_still_moves_on_release() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "a.txt");

    // No `drag_to` calls: the release alone shows the pointer is elsewhere.
    let release = release_on(&mut explorer, "docs");

    assert_eq!(the_folder(&release), Some(path(&dir, "docs").as_path()));
}

#[test]
fn a_release_on_another_row_that_cannot_take_it_is_not_a_click_either() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "src");
    let before = names(&explorer);

    let release = release_on(&mut explorer, "util");

    assert_eq!(release, Release::Nothing);
    assert_eq!(names(&explorer), before);
}

#[test]
fn the_project_folder_cannot_be_dragged() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    let root = explorer.rows()[0].name.clone();
    press_on(&mut explorer, &root);

    drag_over(&mut explorer, "docs");

    assert!(!explorer.is_dragging());
    assert_eq!(explorer.drop_target(), None);
}

#[test]
fn a_new_press_forgets_an_unfinished_drag() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "a.txt");
    drag_over(&mut explorer, "docs");

    press_on(&mut explorer, "docs");

    assert!(!explorer.is_dragging());
    assert_eq!(explorer.drop_target(), None);
}

#[test]
fn dragging_without_a_press_does_nothing() {
    let dir = project();
    let mut explorer = open_explorer(&dir);

    drag_over(&mut explorer, "docs");

    assert!(!explorer.is_dragging());
}

#[cfg(unix)]
#[test]
fn a_link_that_loops_takes_no_drop() {
    let dir = project();
    std::os::unix::fs::symlink(".", dir.path().join("here")).unwrap();
    let mut explorer = open_explorer(&dir);
    explorer.refresh().unwrap();
    layout(&mut explorer);
    press_on(&mut explorer, "a.txt");

    drag_over(&mut explorer, "here");

    assert_eq!(explorer.drop_target(), None);
}

#[test]
fn a_drag_follows_paths_not_rows_when_the_tree_changes() {
    let dir = project();
    let mut explorer = open_explorer(&dir);
    press_on(&mut explorer, "a.txt");
    drag_over(&mut explorer, "docs");
    // Something new appears above while dragging; the row moves down by one.
    fs::create_dir(dir.path().join("aaa")).unwrap();
    explorer.refresh().unwrap();
    layout(&mut explorer);

    let release = release_on(&mut explorer, "docs");

    assert_eq!(
        release,
        Release::Move {
            from: path(&dir, "a.txt"),
            into: path(&dir, "docs"),
        }
    );
}
