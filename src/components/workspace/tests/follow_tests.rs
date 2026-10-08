use crate::buffer::{Buffer, Position};
use crate::components::editor::{Editor, Motion};
use crate::components::workspace::Workspace;
use crate::ui::layout::Axis;

/// Two panes (a split) on one document; focus is in the second.
fn split_workspace(text: &str) -> Workspace {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text(text)));
    workspace.split(Axis::Horizontal);
    workspace
}

fn head(workspace: &Workspace) -> Position {
    workspace.active_editor().selections().primary().head()
}

fn text(workspace: &Workspace) -> String {
    workspace.active_editor().buffer().text()
}

fn goto(workspace: &mut Workspace, line: usize, column: usize) {
    workspace.with_editor(|e| {
        e.move_cursor(Motion::DocumentStart).unwrap();
        for _ in 0..line {
            e.move_cursor(Motion::Down).unwrap();
        }
        for _ in 0..column {
            e.move_cursor(Motion::Right).unwrap();
        }
    });
}

#[test]
fn an_edit_in_one_pane_shows_in_the_other() {
    let mut workspace = split_workspace("hello");
    goto(&mut workspace, 0, 5);
    workspace.with_editor(|e| e.insert_text(" world")).unwrap();

    workspace.focus_next_pane();

    assert_eq!(text(&workspace), "hello world");
}

#[test]
fn the_other_cursor_shifts_when_text_is_inserted_before_it() {
    let mut workspace = split_workspace("abc\ndef");
    goto(&mut workspace, 1, 2);
    workspace.focus_next_pane();
    goto(&mut workspace, 0, 0);
    workspace.with_editor(|e| e.insert_text("XX\nYY")).unwrap();

    workspace.focus_next_pane();

    assert_eq!(head(&workspace), Position::new(2, 2));
}

#[test]
fn the_other_cursor_stays_when_the_edit_is_after_it() {
    let mut workspace = split_workspace("abc\ndef");
    goto(&mut workspace, 0, 1);
    workspace.focus_next_pane();
    goto(&mut workspace, 1, 3);
    workspace.with_editor(|e| e.insert_text("!!!")).unwrap();

    workspace.focus_next_pane();

    assert_eq!(head(&workspace), Position::new(0, 1));
}

#[test]
fn a_cursor_at_the_insertion_point_stays_before_the_new_text() {
    let mut workspace = split_workspace("abc");
    goto(&mut workspace, 0, 1);
    workspace.focus_next_pane();
    goto(&mut workspace, 0, 1);
    workspace.with_editor(|e| e.insert_text("XYZ")).unwrap();

    workspace.focus_next_pane();

    assert_eq!(head(&workspace), Position::new(0, 1));
}

#[test]
fn a_cursor_inside_deleted_text_moves_to_where_the_deletion_began() {
    let mut workspace = split_workspace("0123456789");
    goto(&mut workspace, 0, 5);
    workspace.focus_next_pane();
    workspace.with_editor(|e| {
        e.move_cursor(Motion::DocumentStart).unwrap();
        for _ in 0..3 {
            e.move_cursor(Motion::Right).unwrap();
        }
        for _ in 0..5 {
            e.select(Motion::Right).unwrap();
        }
        e.delete_backward().unwrap();
    });

    workspace.focus_next_pane();

    assert_eq!(text(&workspace), "01289");
    assert_eq!(head(&workspace), Position::new(0, 3));
}

#[test]
fn the_other_selection_keeps_covering_the_same_text() {
    let mut workspace = split_workspace("one two three");
    goto(&mut workspace, 0, 4);
    workspace.with_editor(|e| {
        for _ in 0..3 {
            e.select(Motion::Right).unwrap();
        }
    });
    workspace.focus_next_pane();
    goto(&mut workspace, 0, 0);
    workspace.with_editor(|e| e.insert_text("ZZ ")).unwrap();

    workspace.focus_next_pane();

    let selection = *workspace.active_editor().selections().primary();
    assert_eq!(selection.start(), Position::new(0, 7));
    assert_eq!(selection.end(), Position::new(0, 10));
}

#[test]
fn undo_in_either_pane_undoes_the_shared_history() {
    let mut workspace = split_workspace("");
    workspace
        .with_editor(|e| e.insert_text("typed here"))
        .unwrap();
    workspace.focus_next_pane();

    workspace.with_editor(|e| e.undo()).unwrap();

    assert_eq!(text(&workspace), "");
    workspace.focus_next_pane();
    assert_eq!(text(&workspace), "");
}

#[test]
fn undo_moves_the_other_cursor_too() {
    let mut workspace = split_workspace("abc");
    goto(&mut workspace, 0, 3);
    workspace.focus_next_pane();
    goto(&mut workspace, 0, 0);
    workspace.with_editor(|e| e.insert_text("12345")).unwrap();
    workspace.focus_next_pane();
    assert_eq!(head(&workspace), Position::new(0, 8));
    workspace.focus_next_pane();

    workspace.with_editor(|e| e.undo()).unwrap();
    workspace.focus_next_pane();

    assert_eq!(head(&workspace), Position::new(0, 3));
}

#[test]
fn typing_bursts_of_two_panes_are_not_merged() {
    let mut workspace = split_workspace("");
    workspace.with_editor(|e| e.insert_text("a")).unwrap();
    workspace.focus_next_pane();
    goto(&mut workspace, 0, 1);
    workspace.with_editor(|e| e.insert_text("b")).unwrap();
    workspace.focus_next_pane();
    goto(&mut workspace, 0, 2);
    workspace.with_editor(|e| e.insert_text("c")).unwrap();

    workspace.with_editor(|e| e.undo()).unwrap();
    assert_eq!(text(&workspace), "ab");
    workspace.with_editor(|e| e.undo()).unwrap();
    assert_eq!(text(&workspace), "a");
}

#[test]
fn views_of_different_documents_do_not_affect_each_other() {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("abc")));
    goto(&mut workspace, 0, 2);
    workspace.open_buffer(Buffer::from_text("xyz"));
    workspace.with_editor(|e| e.insert_text("!!")).unwrap();

    workspace.previous_tab();

    assert_eq!(text(&workspace), "abc");
    assert_eq!(head(&workspace), Position::new(0, 2));
}

#[test]
fn a_tab_in_the_same_pane_on_the_same_document_follows_too() {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("hello")));
    goto(&mut workspace, 0, 5);
    workspace.split(Axis::Horizontal);
    workspace.focus_next_pane();

    workspace.with_editor(|e| {
        e.move_cursor(Motion::DocumentStart).unwrap();
        e.insert_text(">> ").unwrap();
    });

    workspace.focus_next_pane();
    assert_eq!(head(&workspace), Position::new(0, 8));
}
