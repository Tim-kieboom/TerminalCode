use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::state::AppState;
use crate::buffer::{Buffer, Position};
use crate::components::editor::{Editor, Motion};
use crate::components::workspace::Workspace;
use crate::event::mouse::Clicks;
use crate::git::MAX_DIFF_LINES as MAX_LINES;
use crate::ui::ComponentKind;
use crate::ui::Render;
use crate::ui::layout::Placement;

const WIDTH: u16 = 60;
const HEIGHT: u16 = 12;

/// A workspace whose one tab shows `new`, and a diff of it against `old`.
fn diff_of(old: &str, new: &str) -> Workspace {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text(new)));
    let document = workspace.active_document_id().unwrap();
    workspace
        .open_diff(document, "a.txt".into(), Some("abc1234".into()), Some(old))
        .unwrap();
    workspace
}

/// What the screen shows, a row at a time, split into the left half and the right half.
fn halves(workspace: &mut Workspace) -> Vec<(String, String)> {
    let state = AppState::default();
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    terminal
        .draw(|frame| {
            let placement = Placement::new(ComponentKind::Editor, frame.area());
            workspace.prepare(&placement);
            workspace.render(frame, &state, &placement);
        })
        .unwrap();
    terminal
        .backend()
        .to_string()
        .lines()
        .map(|line| {
            let line = line.trim_matches('"');
            let middle = line
                .char_indices()
                .nth(usize::from(WIDTH / 2))
                .map_or(line.len(), |(at, _)| at);
            let (left, right) = line.split_at(middle);
            (left.to_owned(), right.to_owned())
        })
        .collect()
}

/// The row whose left half has `old` in it, and its right half. The tab bar and the row
/// that names the sides are above the text.
fn row_with_old(rows: &[(String, String)], old: &str) -> (String, String) {
    rows.iter()
        .skip(2)
        .find(|(left, _)| left.contains(old))
        .cloned()
        .unwrap_or_else(|| panic!("no row has {old} on the left: {rows:?}"))
}

#[test]
fn the_old_text_is_on_the_left_and_the_new_on_the_right_in_one_tab() {
    let mut workspace = diff_of("one\ntwo\n", "one\nTWO\n");

    let rows = halves(&mut workspace);

    let (left, right) = row_with_old(&rows, "two");
    assert!(
        !left.contains("TWO") && right.contains("TWO"),
        "{left}|{right}"
    );
    assert!(rows[0].0.contains("(diff)"), "{:?}", rows[0]);
}

#[test]
fn a_removed_line_has_a_filler_row_on_the_right_so_the_rest_lines_up() {
    let mut workspace = diff_of("keep\ngone\nafter\n", "keep\nafter\n");

    let rows = halves(&mut workspace);

    let (gone_left, gone_right) = row_with_old(&rows, "gone");
    assert!(!gone_right.contains("after") && !gone_right.contains("keep"));
    let (after_left, after_right) = row_with_old(&rows, "after");
    assert!(after_left.contains("after") && after_right.contains("after"));
    let _ = gone_left;
}

#[test]
fn an_added_line_has_a_filler_row_on_the_left() {
    let mut workspace = diff_of("keep\nafter\n", "keep\nnew\nafter\n");

    let rows = halves(&mut workspace);

    let at = rows
        .iter()
        .position(|(_, right)| right.contains("new"))
        .expect("the new line is shown");
    assert!(!rows[at].0.contains("after") && !rows[at].0.contains("keep"));
    assert!(rows[at + 1].0.contains("after") && rows[at + 1].1.contains("after"));
}

#[test]
fn typing_on_the_right_changes_the_diff_at_once() {
    let mut workspace = diff_of("a\nb\n", "a\nb\n");
    let rows = halves(&mut workspace);
    let (left, right) = row_with_old(&rows, "b");
    assert!(right.contains('b'), "same text: {left}|{right}");

    workspace.with_editor(|editor| {
        editor.go_to(Position::new(1, 0));
        editor.insert_text("X").unwrap();
    });
    let rows = halves(&mut workspace);

    let (left, right) = row_with_old(&rows, "b");
    assert!(right.contains("Xb"), "{left}|{right}");
}

#[test]
fn the_right_side_is_the_document_of_the_file_and_is_saved_with_it() {
    let mut workspace = diff_of("a\n", "a\n");

    workspace.with_editor(|editor| editor.insert_text("z").unwrap());

    assert!(workspace.active_editor().buffer().is_dirty());
    assert!(workspace.active_is_diff());
}

#[test]
fn the_left_side_cannot_be_edited_and_has_no_cursor_to_type_with() {
    let mut workspace = diff_of("a\n", "b\n");
    let _ = halves(&mut workspace);

    workspace.with_editor(|editor| editor.insert_text("z").unwrap());

    let rows = halves(&mut workspace);
    let (left, _) = row_with_old(&rows, "a");
    assert!(!left.contains('z'), "{left}");
}

#[test]
fn opening_the_same_diff_again_brings_that_tab_up_with_the_new_base() {
    let mut workspace = diff_of("old\n", "new\n");
    let document = workspace.diffs()[0].document;
    workspace.new_file();
    assert!(!workspace.active_is_diff());
    let tabs = workspace.tab_count_for_test();

    workspace
        .open_diff(
            document,
            "a.txt".into(),
            Some("def5678".into()),
            Some("older\n"),
        )
        .unwrap();

    assert_eq!(workspace.tab_count_for_test(), tabs, "no second diff tab");
    assert!(workspace.active_is_diff());
    assert_eq!(workspace.diffs()[0].head.as_deref(), Some("def5678"));
}

#[test]
fn a_file_with_nothing_in_git_has_nothing_on_the_left() {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("fresh\n")));
    let document = workspace.active_document_id().unwrap();
    workspace
        .open_diff(document, "a.txt".into(), None, None)
        .unwrap();

    let rows = halves(&mut workspace);

    let at = rows
        .iter()
        .position(|(_, right)| right.contains("fresh"))
        .unwrap();
    assert!(!rows[at].0.contains("fresh"));
}

#[test]
fn a_text_that_is_too_big_is_refused_and_says_why() {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("a\n")));
    let document = workspace.active_document_id().unwrap();
    let big = "x\n".repeat(MAX_LINES + 1);

    let result = workspace.open_diff(document, "big.txt".into(), None, Some(&big));

    let reason = result.unwrap_err();
    assert!(reason.contains("too large"), "{reason}");
    assert!(!workspace.active_is_diff());
}

#[test]
fn next_and_previous_change_move_the_cursor_to_where_a_change_starts() {
    let mut workspace = diff_of("a\nb\nc\nd\ne\nf\n", "a\nB\nc\nd\ne\nF\n");

    assert_eq!(workspace.diff_jump(true), Some(true));
    assert_eq!(cursor_line(&workspace), 1);
    assert_eq!(workspace.diff_jump(true), Some(true));
    assert_eq!(cursor_line(&workspace), 5);
    assert_eq!(workspace.diff_jump(true), Some(false), "no more changes");
    assert_eq!(workspace.diff_jump(false), Some(true));
    assert_eq!(cursor_line(&workspace), 1);
}

#[test]
fn a_tab_that_is_no_diff_says_so_to_the_jump() {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("a\n")));

    assert_eq!(workspace.diff_jump(true), None);
}

fn cursor_line(workspace: &Workspace) -> usize {
    workspace.active_editor().selections().primary().head().line
}

#[test]
fn a_click_on_the_right_after_a_filler_row_lands_on_the_line_clicked() {
    let mut workspace = diff_of("a\nb\nc\nd\n", "a\nd\n");
    let rows = halves(&mut workspace);
    // The right half shows `a`, a filler, a filler, `d`.
    let row_of_d = rows
        .iter()
        .position(|(left, right)| left.contains('d') && right.contains('d'))
        .unwrap();
    let column = WIDTH / 2 + 8;

    workspace
        .mouse_press(
            column,
            u16::try_from(row_of_d).unwrap(),
            false,
            Clicks::Single,
        )
        .unwrap();

    assert_eq!(cursor_line(&workspace), 1);
}

#[test]
fn the_cursor_stays_in_view_when_filler_rows_take_the_room() {
    // Ten lines were removed in the middle: the right side is full of filler there.
    let old: String = (0..40).map(|n| format!("line {n}\n")).collect();
    let new: String = (0..40)
        .filter(|n| !(10..30).contains(n))
        .map(|n| format!("line {n}\n"))
        .collect();
    let mut workspace = diff_of(&old, &new);

    workspace.with_editor(|editor| {
        editor.move_cursor(Motion::DocumentStart).unwrap();
        for _ in 0..15 {
            editor.move_cursor(Motion::Down).unwrap();
        }
    });
    let rows = halves(&mut workspace);

    // The cursor is on the new text's line 15, which is `line 35`, and the left side
    // shows the same row, though the old text has it at line 35 and not 15.
    let (left, right) = row_with_old(&rows, "line 35");
    assert!(right.contains("line 35"), "{left}|{right}");
}
