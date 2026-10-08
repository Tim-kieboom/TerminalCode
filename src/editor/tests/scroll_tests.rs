use crate::editor::{Editor, Scroll};

#[test]
fn visible_target_does_not_scroll() {
    let mut editor = Editor::default();

    editor.scroll_to_show(5, 10, 20, 80);

    assert_eq!(editor.scroll(), Scroll { top: 0, left: 0 });
}

#[test]
fn target_below_the_viewport_scrolls_just_enough() {
    let mut editor = Editor::default();

    editor.scroll_to_show(25, 0, 10, 80);

    assert_eq!(editor.scroll().top, 16);
}

#[test]
fn target_above_the_viewport_scrolls_up_to_it() {
    let mut editor = Editor::default();
    editor.scroll_to_show(25, 0, 10, 80);

    editor.scroll_to_show(3, 0, 10, 80);

    assert_eq!(editor.scroll().top, 3);
}

#[test]
fn horizontal_scroll_follows_the_display_column() {
    let mut editor = Editor::default();

    editor.scroll_to_show(0, 100, 10, 40);
    assert_eq!(editor.scroll().left, 61);

    editor.scroll_to_show(0, 5, 10, 40);
    assert_eq!(editor.scroll().left, 5);
}

#[test]
fn zero_sized_viewport_does_not_panic_or_scroll_away() {
    let mut editor = Editor::default();

    editor.scroll_to_show(10, 10, 0, 0);

    assert_eq!(editor.scroll(), Scroll { top: 0, left: 0 });
}
