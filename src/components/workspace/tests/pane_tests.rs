use ratatui::layout::Rect;

use crate::buffer::Buffer;
use crate::components::ComponentKind;
use crate::components::editor::{Editor, Motion};
use crate::components::workspace::{CloseResult, FocusDirection, Workspace};
use crate::ui::Render;
use crate::ui::layout::{Axis, Placement};

fn fresh(text: &str) -> Workspace {
    Workspace::new(Editor::new(Buffer::from_text(text)))
}

fn text(workspace: &Workspace) -> String {
    workspace.active_editor().buffer().text()
}

/// Runs a layout pass so panes know their screen areas.
fn layout(workspace: &mut Workspace, area: Rect) {
    workspace.prepare(&Placement {
        kind: ComponentKind::Editor,
        area,
    });
}

const AREA: Rect = Rect::new(0, 0, 100, 40);

#[test]
fn splitting_adds_a_pane_showing_the_same_document() {
    let mut workspace = fresh("shared");

    workspace.split(Axis::Horizontal);

    assert_eq!(workspace.pane_count(), 2);
    assert_eq!(workspace.document_count(), 1);
    assert_eq!(text(&workspace), "shared");
}

#[test]
fn the_new_pane_starts_with_the_same_cursor_and_takes_focus() {
    let mut workspace = fresh("abc\ndef");
    workspace
        .with_editor(|e| e.move_cursor(Motion::Down))
        .unwrap();
    workspace
        .with_editor(|e| e.move_cursor(Motion::Right))
        .unwrap();

    workspace.split(Axis::Vertical);

    let head = workspace.active_editor().selections().primary().head();
    assert_eq!((head.line, head.column), (1, 1));
}

#[test]
fn panes_are_laid_out_side_by_side_or_stacked() {
    let mut workspace = fresh("x");
    workspace.split(Axis::Horizontal);
    layout(&mut workspace, AREA);

    let areas: Vec<_> = workspace.pane_areas();
    assert_eq!(areas, [Rect::new(0, 0, 50, 40), Rect::new(50, 0, 50, 40)]);

    let mut stacked = fresh("x");
    stacked.split(Axis::Vertical);
    layout(&mut stacked, AREA);
    assert_eq!(
        stacked.pane_areas(),
        [Rect::new(0, 0, 100, 20), Rect::new(0, 20, 100, 20)]
    );
}

#[test]
fn nested_splits_divide_the_focused_pane() {
    let mut workspace = fresh("x");
    workspace.split(Axis::Horizontal);
    workspace.split(Axis::Vertical);
    layout(&mut workspace, AREA);

    assert_eq!(
        workspace.pane_areas(),
        [
            Rect::new(0, 0, 50, 40),
            Rect::new(50, 0, 50, 20),
            Rect::new(50, 20, 50, 20)
        ]
    );
}

#[test]
fn focus_next_pane_cycles_in_reading_order() {
    let mut workspace = fresh("x");
    workspace.split(Axis::Horizontal);
    workspace.split(Axis::Vertical);
    layout(&mut workspace, AREA);
    let order = workspace.focused_area();
    assert_eq!(order, Rect::new(50, 20, 50, 20));

    workspace.focus_next_pane();
    assert_eq!(workspace.focused_area(), Rect::new(0, 0, 50, 40));
    workspace.focus_next_pane();
    assert_eq!(workspace.focused_area(), Rect::new(50, 0, 50, 20));
    workspace.focus_next_pane();
    assert_eq!(workspace.focused_area(), Rect::new(50, 20, 50, 20));
}

#[test]
fn directional_focus_moves_to_the_neighbor_in_that_direction() {
    let mut workspace = fresh("x");
    workspace.split(Axis::Horizontal);
    workspace.split(Axis::Vertical);
    layout(&mut workspace, AREA);

    workspace.focus_direction(FocusDirection::Up);
    assert_eq!(workspace.focused_area(), Rect::new(50, 0, 50, 20));

    workspace.focus_direction(FocusDirection::Left);
    assert_eq!(workspace.focused_area(), Rect::new(0, 0, 50, 40));

    workspace.focus_direction(FocusDirection::Right);
    assert_eq!(workspace.focused_area(), Rect::new(50, 0, 50, 20));

    workspace.focus_direction(FocusDirection::Down);
    assert_eq!(workspace.focused_area(), Rect::new(50, 20, 50, 20));
}

#[test]
fn directional_focus_stays_put_when_nothing_is_there() {
    let mut workspace = fresh("x");
    workspace.split(Axis::Horizontal);
    layout(&mut workspace, AREA);
    let before = workspace.focused_area();

    workspace.focus_direction(FocusDirection::Right);
    workspace.focus_direction(FocusDirection::Up);
    workspace.focus_direction(FocusDirection::Down);

    assert_eq!(workspace.focused_area(), before);
}

#[test]
fn closing_the_only_tab_of_a_split_pane_closes_the_pane() {
    let mut workspace = fresh("shared");
    workspace.split(Axis::Horizontal);
    layout(&mut workspace, AREA);

    assert_eq!(workspace.close_tab(), CloseResult::Closed);
    layout(&mut workspace, AREA);

    assert_eq!(workspace.pane_count(), 1);
    assert_eq!(workspace.document_count(), 1);
    assert_eq!(workspace.pane_areas(), [AREA]);
    assert_eq!(text(&workspace), "shared");
}

#[test]
fn closing_a_view_of_a_modified_shared_document_needs_no_confirmation() {
    let mut workspace = fresh("shared");
    workspace.with_editor(|e| e.insert_text("!")).unwrap();
    workspace.split(Axis::Horizontal);

    assert_eq!(workspace.close_tab(), CloseResult::Closed);

    assert_eq!(text(&workspace), "!shared");
}

#[test]
fn closing_the_last_view_of_a_modified_document_asks_first() {
    let mut workspace = fresh("shared");
    workspace.with_editor(|e| e.insert_text("!")).unwrap();
    workspace.split(Axis::Horizontal);
    workspace.close_tab();

    assert!(matches!(workspace.close_tab(), CloseResult::Unsaved(_)));
}

#[test]
fn each_pane_has_its_own_tabs() {
    let mut workspace = fresh("a");
    workspace.split(Axis::Horizontal);
    workspace.open_buffer(Buffer::from_text("b"));

    assert_eq!(workspace.tab_names().0.len(), 2);
    workspace.focus_next_pane();
    assert_eq!(workspace.tab_names().0.len(), 1);
}
