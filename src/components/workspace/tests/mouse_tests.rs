use ratatui::layout::Rect;

use crate::buffer::{Buffer, Position};
use crate::components::ComponentKind;
use crate::components::editor::Editor;
use crate::components::workspace::{CloseResult, Workspace};
use crate::event::mouse::Clicks;
use crate::ui::Render;
use crate::ui::layout::{Axis, Placement};

const AREA: Rect = Rect::new(0, 0, 100, 40);

fn layout(workspace: &mut Workspace) {
    workspace.prepare(&Placement::new(ComponentKind::Editor, AREA));
}

fn two_panes(text: &str) -> Workspace {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text(text)));
    workspace.split(Axis::Horizontal);
    layout(&mut workspace);
    workspace
}

fn head(workspace: &Workspace) -> Position {
    workspace.active_editor().selections().primary().head()
}

#[test]
fn clicking_in_another_pane_focuses_it_and_places_the_cursor_there() {
    let mut workspace = two_panes("hello world");
    // Focus is in the right pane; click inside the left one's text:
    // x=0 border, 1..5 gutter, text from x=5; y=0 tab bar, y=1 border, text from y=2.
    assert_eq!(workspace.focused_area(), Rect::new(50, 0, 50, 40));

    workspace.mouse_press(8, 2, false, Clicks::Single).unwrap();

    assert_eq!(workspace.focused_area(), Rect::new(0, 0, 50, 40));
    assert_eq!(head(&workspace), Position::new(0, 3));
}

#[test]
fn clicking_the_tab_bar_switches_tabs_without_moving_the_cursor() {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("one")));
    workspace.open_buffer(Buffer::from_text("two"));
    layout(&mut workspace);
    assert_eq!(workspace.tab_names().1, 1);

    // The first tab label starts at x=0 in the tab bar row.
    workspace.mouse_press(2, 0, false, Clicks::Single).unwrap();

    assert_eq!(workspace.tab_names().1, 0);
    assert_eq!(workspace.active_editor().buffer().text(), "one");
}

#[test]
fn clicking_on_a_border_or_outside_every_pane_does_nothing_harmful() {
    let mut workspace = two_panes("hello");
    let before = workspace.focused_area();

    workspace
        .mouse_press(500, 500, false, Clicks::Single)
        .unwrap();
    workspace.mouse_press(60, 1, false, Clicks::Single).unwrap();

    assert_eq!(workspace.focused_area(), Rect::new(50, 0, 50, 40));
    let _ = before;
}

#[test]
fn the_wheel_scrolls_the_pane_under_the_pointer_without_moving_focus() {
    let text = (0..200)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let mut workspace = two_panes(&text);
    workspace.prepare(&Placement::new(ComponentKind::Editor, AREA));
    let focused = workspace.focused_area();

    // Pointer over the left pane while focus is on the right.
    workspace.with_editor_at(10, 10, |editor| editor.scroll_lines(2));

    assert_eq!(workspace.focused_area(), focused);
    assert_eq!(workspace.active_editor().scroll().top, 0);
    workspace.with_editor_at(60, 10, |editor| editor.scroll_lines(2));
    assert_eq!(workspace.active_editor().scroll().top, 6);
}

#[test]
fn the_wheel_over_nothing_is_ignored() {
    let mut workspace = two_panes("x");

    assert!(
        workspace
            .with_editor_at(500, 500, |editor| editor.scroll_lines(1))
            .is_none()
    );
}

#[test]
fn dragging_extends_the_selection_in_the_focused_pane() {
    let mut workspace = two_panes("hello world");
    workspace.mouse_press(5, 2, false, Clicks::Single).unwrap();

    workspace
        .with_editor(|editor| editor.mouse_drag(10, 2))
        .unwrap();

    let selection = *workspace.active_editor().selections().primary();
    assert_eq!(selection.start(), Position::new(0, 0));
    assert_eq!(selection.end(), Position::new(0, 5));
}

// ---- middle click closes a tab

/// Three tabs, "a", "b", "c" (c active), laid out.
fn three_tabs() -> Workspace {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("a")));
    workspace.open_buffer(Buffer::from_text("b"));
    workspace.open_buffer(Buffer::from_text("c"));
    layout(&mut workspace);
    workspace
}

/// A cell inside tab `index` of the focused pane's tab bar.
fn on_tab(workspace: &Workspace, index: usize) -> (u16, u16) {
    let rect = workspace.tab_areas()[index];
    (rect.x + 1, rect.y)
}

fn text(workspace: &Workspace) -> String {
    workspace.active_editor().buffer().text()
}

#[test]
fn middle_clicking_an_inactive_tab_closes_it_and_keeps_the_active_one() {
    let mut workspace = three_tabs();
    let (x, y) = on_tab(&workspace, 0);

    let result = workspace.middle_press(x, y);

    assert_eq!(result, Some(CloseResult::Closed));
    assert_eq!(workspace.tab_names().0.len(), 2);
    assert_eq!(text(&workspace), "c");
    assert_eq!(workspace.tab_names().1, 1);
}

#[test]
fn middle_clicking_a_tab_after_the_active_one_leaves_the_active_index_alone() {
    let mut workspace = three_tabs();
    workspace.activate_tab(0);
    layout(&mut workspace);
    let (x, y) = on_tab(&workspace, 2);

    workspace.middle_press(x, y);

    assert_eq!(text(&workspace), "a");
    assert_eq!(workspace.tab_names().1, 0);
    assert_eq!(workspace.tab_names().0.len(), 2);
}

#[test]
fn middle_clicking_the_active_tab_activates_its_neighbor() {
    let mut workspace = three_tabs();
    workspace.activate_tab(1);
    layout(&mut workspace);
    let (x, y) = on_tab(&workspace, 1);

    workspace.middle_press(x, y);

    assert_eq!(text(&workspace), "c");
}

#[test]
fn middle_clicking_text_or_empty_space_closes_nothing() {
    let mut workspace = three_tabs();

    // In the editor's text, and far right in the tab bar, past the last tab.
    assert_eq!(workspace.middle_press(20, 5), None);
    assert_eq!(workspace.middle_press(90, 0), None);
    assert_eq!(workspace.middle_press(500, 500), None);

    assert_eq!(workspace.tab_names().0.len(), 3);
}

#[test]
fn middle_clicking_a_modified_tab_needs_a_second_middle_click() {
    let mut workspace = three_tabs();
    workspace.with_editor(|e| e.insert_text("!")).unwrap();
    layout(&mut workspace);
    let (x, y) = on_tab(&workspace, 2);

    let first = workspace.middle_press(x, y);
    assert!(matches!(first, Some(CloseResult::Unsaved(name)) if name.contains("[+]")));
    assert_eq!(workspace.tab_names().0.len(), 3);

    assert_eq!(workspace.middle_press(x, y), Some(CloseResult::Closed));
    assert_eq!(workspace.tab_names().0.len(), 2);
}

#[test]
fn middle_clicking_a_different_tab_in_between_cancels_the_confirmation() {
    let mut workspace = three_tabs();
    workspace.with_editor(|e| e.insert_text("!")).unwrap();
    layout(&mut workspace);
    let modified = on_tab(&workspace, 2);
    let other = on_tab(&workspace, 0);
    workspace.middle_press(modified.0, modified.1);

    workspace.middle_press(other.0, other.1);
    layout(&mut workspace);
    let modified = on_tab(&workspace, 1);

    assert!(matches!(
        workspace.middle_press(modified.0, modified.1),
        Some(CloseResult::Unsaved(_))
    ));
}

#[test]
fn middle_clicking_the_only_tab_empties_the_editor_area() {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("only")));
    layout(&mut workspace);
    let (x, y) = on_tab(&workspace, 0);

    workspace.middle_press(x, y);

    assert!(!workspace.has_tabs());
    assert_eq!(workspace.tab_names().0.len(), 0);
}

#[test]
fn the_mouse_does_nothing_in_an_empty_editor_area() {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("only")));
    workspace.close_tab();
    layout(&mut workspace);

    workspace.mouse_press(20, 5, false, Clicks::Single).unwrap();
    assert_eq!(workspace.middle_press(20, 0), None);
    assert!(
        workspace
            .with_editor_at(20, 5, |editor| editor.scroll_lines(1))
            .is_none()
    );

    assert!(!workspace.has_tabs());
}

#[test]
fn middle_clicking_the_tab_of_an_unfocused_pane_closes_it_without_moving_focus() {
    let mut workspace = two_panes("shared");
    // Focus is in the right pane; its sibling on the left has one tab.
    let focused = workspace.focused_area();
    assert_eq!(focused, Rect::new(50, 0, 50, 40));

    // The left pane's only tab sits at x=0 of its own tab bar.
    let result = workspace.middle_press(2, 0);

    assert_eq!(result, Some(CloseResult::Closed));
    assert_eq!(workspace.pane_count(), 1);
    assert_eq!(text(&workspace), "shared");
}

#[test]
fn closing_an_unfocused_pane_keeps_the_focused_one_focused() {
    let mut workspace = two_panes("shared");
    workspace.split(Axis::Vertical);
    layout(&mut workspace);
    let focused_before = workspace.focused_area();

    // Close the left pane (full height) from its tab bar.
    workspace.middle_press(2, 0);
    layout(&mut workspace);

    assert_eq!(workspace.pane_count(), 2);
    let focused_after = workspace.focused_area();
    assert_eq!(focused_after.height, focused_before.height);
    assert_eq!(focused_after.y, focused_before.y);
}
