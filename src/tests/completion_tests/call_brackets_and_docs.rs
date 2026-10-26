use super::*;

#[test]
fn a_call_with_an_argument_is_written_with_the_placeholder_selected() {
    let mut rig = Rig::with_answer(json!([call_item("push", "push(${1:value})", None)]));
    rig.type_text("pu");
    rig.wait_for_box();

    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "push(value)");
    let selection = *rig.app.state().editor().selections().primary();
    assert_eq!(
        (selection.anchor().column, selection.head().column),
        (5, 10)
    );
}

#[test]
fn a_call_that_takes_nothing_leaves_the_cursor_after_the_brackets() {
    let mut rig = Rig::with_answer(json!([call_item("len", "len()$0", None)]));
    rig.type_text("le");
    rig.wait_for_box();

    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "len()");
    assert_eq!(cursor_column(&rig), 5);
}

#[test]
fn typing_replaces_the_selected_placeholder_after_picking() {
    let mut rig = Rig::with_answer(json!([call_item("push", "push(${1:value})", None)]));
    rig.type_text("pu");
    rig.wait_for_box();
    rig.press(KeyCode::Tab);

    rig.press(KeyCode::Char('x'));

    assert_eq!(rig.text(), "push(x)");
}

#[test]
fn deleting_the_selected_placeholder_leaves_empty_brackets() {
    let mut rig = Rig::with_answer(json!([call_item("push", "push(${1:value})", None)]));
    rig.type_text("pu");
    rig.wait_for_box();
    rig.press(KeyCode::Tab);

    rig.press(KeyCode::Backspace);

    assert_eq!(rig.text(), "push()");
}

#[test]
fn the_documentation_of_the_highlighted_item_shows_above_the_list_as_plain_text() {
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n",
        answering(json!([
            call_item(
                "pa",
                "pa($0)",
                Some("Adds **one** `item`.\n\nPanics: never.")
            ),
            call_item("pb", "pb()$0", Some("Removes the last item.")),
        ])),
    );
    rig.type_text("p");
    rig.wait_for_box();

    let screen = rig.screen();

    assert!(screen.contains("Adds one item."), "{screen}");
    assert!(screen.contains("Panics: never."), "{screen}");
    assert!(!screen.contains("**"), "{screen}");
    let rows: Vec<&str> = screen.lines().collect();
    let docs_row = rows
        .iter()
        .position(|row| row.contains("Adds one item."))
        .unwrap();
    let list_row = rows.iter().position(|row| row.contains("m pa")).unwrap();
    assert!(docs_row < list_row, "{screen}");
}

#[test]
fn moving_the_highlight_changes_the_documentation() {
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n",
        answering(json!([
            call_item("pop", "pop()$0", Some("Removes the last item.")),
            call_item("push", "push($0)", Some("Adds an item.")),
        ])),
    );
    rig.type_text("p");
    rig.wait_for_box();
    assert!(rig.screen().contains("Removes the last item."));

    rig.press(KeyCode::Down);
    let screen = rig.screen();

    assert!(screen.contains("Adds an item."), "{screen}");
    assert!(!screen.contains("Removes the last item."), "{screen}");
}

#[test]
fn an_item_without_documentation_has_no_documentation_box() {
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n",
        answering(json!([call_item("pop", "pop()$0", None)])),
    );
    rig.type_text("p");
    rig.wait_for_box();

    let screen = rig.screen();

    // Only the list is drawn: one box of three rows.
    assert_eq!(screen.matches('╭').count(), 1, "{screen}");
}

#[test]
fn the_documentation_shows_eight_lines_and_says_how_many_there_are() {
    let long: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n\n\n\n\n\n\n",
        answering(json!([call_item("pop", "pop()$0", Some(&long.join("\n")))])),
    );
    rig.type_text("p");
    rig.wait_for_box();

    let screen = rig.screen();

    assert!(screen.contains("line 1 "), "{screen}");
    assert!(!screen.contains("line 9"), "{screen}");
    // Which lines show, of how many.
    assert!(screen.contains("1-8/20"), "{screen}");
}

#[test]
fn near_the_top_of_the_screen_the_documentation_goes_under_the_list() {
    let mut rig = Rig::start(
        provider(),
        "",
        answering(json!([call_item(
            "pop",
            "pop()$0",
            Some("Removes the last item.")
        )])),
    );
    rig.type_text("p");
    rig.wait_for_box();

    let screen = rig.screen();

    let rows: Vec<&str> = screen.lines().collect();
    let docs = rows
        .iter()
        .position(|row| row.contains("Removes the last item."))
        .unwrap();
    let list = rows.iter().position(|row| row.contains("m pop")).unwrap();
    assert!(docs > list, "{screen}");
}

#[test]
fn with_room_for_only_part_of_the_documentation_it_shows_what_fits() {
    let long: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    // The cursor is in the middle of the 20 rows, so neither side has ten rows.
    let mut rig = Rig::start(
        provider(),
        "\n\n\n\n\n\n\n",
        answering(json!([call_item("pop", "pop()$0", Some(&long.join("\n")))])),
    );
    // The row for the menu icon is not what is being measured.
    rig.app
        .state_mut()
        .set_shown(&crate::ui::ComponentKind::MenuBar, false);
    rig.type_text("p");
    rig.wait_for_box();

    let screen = rig.screen();

    assert!(screen.contains("line 1 "), "{screen}");
    assert!(screen.contains("/20 "), "{screen}");
    assert!(!screen.contains("line 7"), "{screen}");
}
