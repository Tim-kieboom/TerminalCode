use super::*;

#[test]
fn the_wheel_over_the_documentation_scrolls_it_and_not_the_editor() {
    let mut rig = docs_rig();
    let (column, row) = cell_of(&mut rig, "line 2");
    let top_before = rig.app.state().editor().scroll().top;

    wheel(
        &mut rig,
        crossterm::event::MouseEventKind::ScrollDown,
        column,
        row,
    );

    let screen = rig.screen();
    assert!(screen.contains("line 4 "), "{screen}");
    assert!(!screen.contains("line 1 "), "{screen}");
    assert!(screen.contains("4-11/20"), "{screen}");
    assert_eq!(rig.app.state().editor().scroll().top, top_before);
    assert!(rig.app.state().completion_box().is_some());

    wheel(
        &mut rig,
        crossterm::event::MouseEventKind::ScrollUp,
        column,
        row,
    );
    assert!(rig.screen().contains("line 1 "));
}

#[test]
fn the_documentation_stops_at_the_last_page() {
    let mut rig = docs_rig();
    let (column, row) = cell_of(&mut rig, "line 2");

    for _ in 0..20 {
        wheel(
            &mut rig,
            crossterm::event::MouseEventKind::ScrollDown,
            column,
            row,
        );
    }

    let screen = rig.screen();
    assert!(screen.contains("line 20"), "{screen}");
    assert!(screen.contains("13-20/20"), "{screen}");
}

#[test]
fn the_page_keys_scroll_the_documentation_while_the_suggestions_are_open() {
    let mut rig = docs_rig();
    let cursor = rig.app.state().editor().selections().primary().head();
    // The box is known to be drawn after a frame.
    rig.screen();

    rig.press(KeyCode::PageDown);

    let screen = rig.screen();
    assert!(screen.contains("line 9 "), "{screen}");
    assert!(screen.contains("9-16/20"), "{screen}");
    assert!(rig.app.state().completion_box().is_some());
    assert_eq!(
        rig.app.state().editor().selections().primary().head(),
        cursor
    );
    rig.press(KeyCode::PageUp);
    assert!(rig.screen().contains("line 1 "));
}

#[test]
fn the_page_keys_are_the_editors_when_the_documentation_fits() {
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n",
        answering(json!([call_item("pop", "pop()$0", Some("Short."))])),
    );
    rig.type_text("p");
    rig.wait_for_box();
    rig.screen();

    rig.press(KeyCode::PageUp);
    rig.frame();

    // The cursor went up, out of the word: the box is gone.
    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn another_highlighted_item_starts_its_documentation_at_the_top() {
    let long: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    let text = long.join("\n");
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n\n\n\n\n\n\n\n",
        answering(json!([
            call_item("pa", "pa()$0", Some(&text)),
            call_item("pb", "pb()$0", Some(&text)),
        ])),
    );
    rig.type_text("p");
    rig.wait_for_box();
    rig.screen();
    rig.press(KeyCode::PageDown);
    assert!(rig.screen().contains("9-16/20"));

    rig.press(KeyCode::Down);

    let screen = rig.screen();
    assert!(screen.contains("1-8/20"), "{screen}");
    assert!(screen.contains("line 1 "), "{screen}");
}
