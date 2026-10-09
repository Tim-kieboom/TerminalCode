use crate::components::editor::{Editor, Scroll};

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

mod clamping {
    use ratatui::layout::Rect;

    use crate::components::editor::ViewState;

    fn view_scrolled_to(top: usize, height: u16) -> ViewState {
        let mut view = ViewState::default();
        view.set_viewport(Rect::new(0, 0, 80, height));
        view.scroll.top = top;
        view
    }

    #[test]
    fn a_view_scrolled_past_the_end_of_a_shorter_text_shows_the_last_page() {
        let mut view = view_scrolled_to(500, 4);

        view.clamp_scroll(10);

        assert_eq!(view.scroll().top, 6, "lines 7 to 10 fill the viewport");
    }

    #[test]
    fn a_text_shorter_than_the_viewport_shows_from_its_first_line() {
        let mut view = view_scrolled_to(500, 20);

        view.clamp_scroll(3);

        assert_eq!(view.scroll().top, 0);
    }

    #[test]
    fn a_view_inside_the_text_is_left_alone() {
        let mut view = view_scrolled_to(5, 4);

        view.clamp_scroll(10);

        assert_eq!(view.scroll().top, 5);
    }

    #[test]
    fn a_view_scrolled_to_the_last_line_with_the_wheel_stays_there() {
        let mut view = view_scrolled_to(9, 4);

        view.clamp_scroll(10);

        assert_eq!(view.scroll().top, 9);
    }

    #[test]
    fn an_empty_text_scrolls_to_the_top() {
        let mut view = view_scrolled_to(5, 4);

        view.clamp_scroll(0);
        assert_eq!(view.scroll().top, 0);
        view.clamp_scroll(1);
        assert_eq!(view.scroll().top, 0);
    }
}
