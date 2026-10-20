use ratatui::layout::Rect;

use crate::buffer::Buffer;
use crate::components::editor::{Editor, Motion};
use crate::components::workspace::{CloseResult, FocusDirection, Workspace};
use crate::ui::ComponentKind;
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
    workspace.prepare(&Placement::new(ComponentKind::Editor, area));
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

// --- moving the line between two panes ---

#[test]
fn a_split_starts_in_half_and_the_line_can_be_dragged() {
    let mut workspace = fresh("x");
    workspace.split(Axis::Horizontal);
    layout(&mut workspace, AREA);
    let widths = |workspace: &Workspace| -> Vec<u16> {
        workspace
            .pane_areas()
            .iter()
            .map(|area| area.width)
            .collect()
    };
    assert_eq!(widths(&workspace), [50, 50]);

    // The line is the last column of the first pane and the first of the second.
    let divider = workspace
        .divider_at(AREA, 49, 10)
        .expect("the border on the left");
    assert_eq!(divider.axis, Axis::Horizontal);
    assert!(
        workspace.divider_at(AREA, 50, 10).is_some(),
        "and the one on the right"
    );
    assert!(workspace.divider_at(AREA, 30, 10).is_none());
    assert!(
        workspace.divider_at(AREA, 49, 40).is_none(),
        "below the editor area"
    );

    workspace.drag_divider(&divider, 70, 10);
    layout(&mut workspace, AREA);
    assert_eq!(widths(&workspace), [70, 30]);

    // Never all the way: each keeps at least a tenth.
    workspace.drag_divider(&divider, 99, 10);
    layout(&mut workspace, AREA);
    assert_eq!(widths(&workspace), [90, 10]);
    workspace.drag_divider(&divider, 0, 10);
    layout(&mut workspace, AREA);
    assert_eq!(widths(&workspace), [10, 90]);
}

#[test]
fn a_stacked_split_is_dragged_by_rows() {
    let mut workspace = fresh("x");
    workspace.split(Axis::Vertical);
    layout(&mut workspace, AREA);
    let divider = workspace
        .divider_at(AREA, 10, 20)
        .expect("the rows at the middle");
    assert_eq!(divider.axis, Axis::Vertical);
    workspace.drag_divider(&divider, 10, 10);
    layout(&mut workspace, AREA);
    let heights: Vec<u16> = workspace.pane_areas().iter().map(|a| a.height).collect();
    assert_eq!(heights, [10, 30]);
}

#[test]
fn the_deepest_line_wins_where_splits_meet() {
    let mut workspace = fresh("x");
    workspace.split(Axis::Horizontal);
    workspace.split(Axis::Vertical);
    layout(&mut workspace, AREA);
    // The second pane (right half) is split in two rows: its line is only in its own half.
    let inner = workspace
        .divider_at(AREA, 70, 20)
        .expect("the line inside the right half");
    assert_eq!(inner.axis, Axis::Vertical);
    assert!(
        workspace.divider_at(AREA, 20, 20).is_none(),
        "the left half is whole"
    );
    let outer = workspace
        .divider_at(AREA, 50, 5)
        .expect("the line between the halves");
    assert_eq!(outer.axis, Axis::Horizontal);
}

#[test]
fn a_pane_grows_and_shrinks_at_the_cost_of_the_one_next_to_it() {
    let mut workspace = fresh("x");
    assert!(!workspace.resize_focused(50), "one pane has no line");
    workspace.split(Axis::Horizontal);
    layout(&mut workspace, AREA);
    // The new pane, on the right, has the focus.
    assert!(workspace.resize_focused(100));
    layout(&mut workspace, AREA);
    let widths: Vec<u16> = workspace.pane_areas().iter().map(|a| a.width).collect();
    assert_eq!(widths, [40, 60]);
    assert!(workspace.resize_focused(-200));
    layout(&mut workspace, AREA);
    let widths: Vec<u16> = workspace.pane_areas().iter().map(|a| a.width).collect();
    assert_eq!(widths, [60, 40]);
}
