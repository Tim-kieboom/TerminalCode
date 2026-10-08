use crate::buffer::Buffer;
use crate::components::editor::{Editor, Motion};
use crate::components::workspace::{CloseResult, Workspace};

fn fresh(text: &str) -> Workspace {
    Workspace::new(Editor::new(Buffer::from_text(text)))
}

fn text(workspace: &Workspace) -> String {
    workspace.active_editor().buffer().text()
}

fn open(workspace: &mut Workspace, text: &str) {
    workspace.open_buffer(Buffer::from_text(text));
}

#[test]
fn a_new_workspace_has_one_pane_with_one_tab() {
    let workspace = fresh("hello");

    assert_eq!(workspace.pane_count(), 1);
    assert_eq!(workspace.document_count(), 1);
    assert_eq!(workspace.tab_names(), (vec!["[no name]".to_owned()], 0));
}

#[test]
fn opening_a_buffer_adds_and_activates_a_tab() {
    let mut workspace = fresh("one");

    open(&mut workspace, "two");

    assert_eq!(text(&workspace), "two");
    assert_eq!(workspace.tab_names().0.len(), 2);
    assert_eq!(workspace.tab_names().1, 1);
    assert_eq!(workspace.document_count(), 2);
}

#[test]
fn tabs_cycle_forward_and_backward_with_wraparound() {
    let mut workspace = fresh("one");
    open(&mut workspace, "two");
    open(&mut workspace, "three");

    workspace.next_tab();
    assert_eq!(text(&workspace), "one");
    workspace.previous_tab();
    assert_eq!(text(&workspace), "three");
    workspace.previous_tab();
    assert_eq!(text(&workspace), "two");
}

#[test]
fn each_tab_keeps_its_own_cursor() {
    let mut workspace = fresh("abc");
    workspace
        .with_editor(|e| e.move_cursor(Motion::LineEnd))
        .unwrap();
    open(&mut workspace, "xyz");
    workspace
        .with_editor(|e| e.move_cursor(Motion::Right))
        .unwrap();

    workspace.previous_tab();

    assert_eq!(
        workspace
            .active_editor()
            .selections()
            .primary()
            .head()
            .column,
        3
    );
    workspace.next_tab();
    assert_eq!(
        workspace
            .active_editor()
            .selections()
            .primary()
            .head()
            .column,
        1
    );
}

#[test]
fn activate_tab_clamps_to_the_last_tab() {
    let mut workspace = fresh("one");
    open(&mut workspace, "two");

    workspace.activate_tab(0);
    assert_eq!(text(&workspace), "one");

    workspace.activate_tab(99);
    assert_eq!(text(&workspace), "two");
}

#[test]
fn closing_a_clean_tab_removes_it_and_its_document() {
    let mut workspace = fresh("one");
    open(&mut workspace, "two");

    assert_eq!(workspace.close_tab(), CloseResult::Closed);

    assert_eq!(text(&workspace), "one");
    assert_eq!(workspace.document_count(), 1);
}

#[test]
fn closing_the_last_tab_empties_the_editor_area() {
    let mut workspace = fresh("only");

    assert_eq!(workspace.close_tab(), CloseResult::Closed);

    assert_eq!(workspace.pane_count(), 1);
    assert!(!workspace.has_tabs());
    assert_eq!(workspace.document_count(), 0);
    assert_eq!(workspace.tab_names().0.len(), 0);
    assert_eq!(text(&workspace), "");
}

#[test]
fn a_new_file_after_closing_everything_works_again() {
    let mut workspace = fresh("only");
    workspace.close_tab();

    workspace.new_file();

    assert!(workspace.has_tabs());
    assert_eq!(workspace.tab_names(), (vec!["[no name]".to_owned()], 0));
    workspace.with_editor(|e| e.insert_text("typed")).unwrap();
    assert_eq!(text(&workspace), "typed");
}

#[test]
fn closing_the_last_modified_tab_still_asks_first() {
    let mut workspace = fresh("only");
    workspace.with_editor(|e| e.insert_text("!")).unwrap();

    assert!(matches!(workspace.close_tab(), CloseResult::Unsaved(_)));
    assert!(workspace.has_tabs());

    assert_eq!(workspace.close_tab(), CloseResult::Closed);
    assert!(!workspace.has_tabs());
}

#[test]
fn nothing_panics_when_there_are_no_tabs() {
    let mut workspace = fresh("only");
    workspace.close_tab();

    workspace.next_tab();
    workspace.previous_tab();
    workspace.activate_tab(3);
    workspace.split(crate::ui::layout::Axis::Horizontal);
    workspace.focus_next_pane();
    assert_eq!(workspace.close_tab(), CloseResult::Closed);
    let result = workspace.with_editor(|e| e.insert_text("lost"));

    assert!(result.is_ok());
    assert_eq!(workspace.pane_count(), 1);
    assert_eq!(workspace.document_count(), 0);
    assert_eq!(text(&workspace), "");
}

#[test]
fn closing_a_modified_tab_asks_first_and_the_second_close_discards() {
    let mut workspace = fresh("one");
    open(&mut workspace, "two");
    workspace.with_editor(|e| e.insert_text("!")).unwrap();

    let first = workspace.close_tab();
    assert!(matches!(first, CloseResult::Unsaved(name) if name.contains("[+]")));
    assert_eq!(text(&workspace), "!two");

    assert_eq!(workspace.close_tab(), CloseResult::Closed);
    assert_eq!(text(&workspace), "one");
}

#[test]
fn doing_anything_else_cancels_the_discard_confirmation() {
    let mut workspace = fresh("one");
    open(&mut workspace, "two");
    workspace.with_editor(|e| e.insert_text("!")).unwrap();
    assert!(matches!(workspace.close_tab(), CloseResult::Unsaved(_)));

    workspace.with_editor(|e| e.insert_text("?")).unwrap();

    assert!(matches!(workspace.close_tab(), CloseResult::Unsaved(_)));
}

#[test]
fn new_file_opens_an_empty_untitled_tab() {
    let mut workspace = fresh("one");

    workspace.new_file();

    assert_eq!(text(&workspace), "");
    assert_eq!(workspace.tab_names().0, ["[no name]", "[no name]"]);
}

#[test]
fn closing_the_middle_tab_activates_the_one_that_took_its_place() {
    let mut workspace = fresh("a");
    open(&mut workspace, "b");
    open(&mut workspace, "c");
    workspace.activate_tab(1);

    workspace.close_tab();

    assert_eq!(text(&workspace), "c");
}

#[test]
fn closing_the_last_of_several_tabs_activates_the_previous() {
    let mut workspace = fresh("a");
    open(&mut workspace, "b");

    workspace.close_tab();

    assert_eq!(text(&workspace), "a");
}
