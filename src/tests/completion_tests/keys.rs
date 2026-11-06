use super::*;

#[test]
fn tab_writes_the_highlighted_suggestion_in_place_of_the_word() {
    let mut rig = Rig::with_answer(items(&["vec", "vector"]));
    rig.type_text("ve");
    rig.wait_for_box();

    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "vec");
    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn accepting_does_not_open_the_box_again() {
    let mut rig = Rig::with_answer(items(&["vec", "vector"]));
    rig.type_text("ve");
    rig.wait_for_box();
    rig.requests();

    rig.press(KeyCode::Tab);
    rig.frame();

    assert!(rig.app.state().completion_box().is_none());
    assert!(rig.requests().is_empty());
}

#[test]
fn one_undo_takes_the_written_suggestion_back_to_what_was_typed() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("ve");
    rig.wait_for_box();
    rig.press(KeyCode::Tab);

    rig.app.state_mut().edit(|editor| editor.undo().unwrap());

    assert_eq!(rig.text(), "ve");
}

#[test]
fn up_and_down_move_the_highlight_round_the_list() {
    let mut rig = Rig::with_answer(items(&["va", "vb", "vc"]));
    rig.type_text("v");
    rig.wait_for_box();
    assert_eq!(rig.highlighted().as_deref(), Some("va"));

    rig.press(KeyCode::Down);
    assert_eq!(rig.highlighted().as_deref(), Some("vb"));
    rig.press(KeyCode::Up);
    rig.press(KeyCode::Up);
    assert_eq!(rig.highlighted().as_deref(), Some("vc"));
    // The cursor did not move.
    assert_eq!(rig.text(), "v");
    rig.press(KeyCode::Down);
    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "va");
}

#[test]
fn escape_closes_the_box_and_leaves_the_text() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.press(KeyCode::Esc);

    assert!(rig.app.state().completion_box().is_none());
    assert_eq!(rig.text(), "v");
}

#[test]
fn enter_closes_the_box_and_is_a_newline() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.press(KeyCode::Enter);

    assert!(rig.app.state().completion_box().is_none());
    assert_eq!(rig.text(), "v\n");
}

#[test]
fn without_a_box_tab_indents_as_always() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("x");
    rig.pump_until(|_| true);
    assert!(rig.app.state().completion_box().is_none() || rig.shown().is_empty());
    rig.press(KeyCode::Esc);

    rig.press(KeyCode::Tab);

    assert!(rig.text().len() > 1, "{:?}", rig.text());
    assert_ne!(rig.text(), "xvec");
}

#[test]
fn enter_picks_the_suggestion_once_the_highlight_was_moved() {
    let mut rig = Rig::with_answer(items(&["va", "vb", "vc"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.press(KeyCode::Down);
    rig.press(KeyCode::Enter);

    assert_eq!(rig.text(), "vb", "written, and no newline");
    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn enter_picks_after_up_too_and_after_going_round_to_the_first() {
    let mut rig = Rig::with_answer(items(&["va", "vb", "vc"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.press(KeyCode::Up);
    assert_eq!(rig.highlighted().as_deref(), Some("vc"));
    rig.press(KeyCode::Down);
    assert_eq!(rig.highlighted().as_deref(), Some("va"));
    rig.press(KeyCode::Enter);

    assert_eq!(rig.text(), "va", "moved, though back where it began");
}

#[test]
fn enter_with_the_highlight_untouched_is_still_a_newline() {
    let mut rig = Rig::with_answer(items(&["va", "vb"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.press(KeyCode::Enter);

    assert_eq!(rig.text(), "v\n");
}

#[test]
fn typing_on_keeps_the_choice_while_the_item_is_still_listed() {
    let mut rig = Rig::with_answer(items(&["vec", "vector"]));
    rig.type_text("ve");
    rig.wait_for_box();
    rig.press(KeyCode::Down);
    assert_eq!(rig.highlighted().as_deref(), Some("vector"));

    rig.type_text("c");
    assert_eq!(rig.highlighted().as_deref(), Some("vector"));
    rig.press(KeyCode::Enter);

    assert_eq!(rig.text(), "vector");
}

#[test]
fn typing_a_letter_that_drops_the_chosen_item_starts_over_and_enter_is_a_newline() {
    let mut rig = Rig::with_answer(items(&["vec", "vex"]));
    rig.type_text("ve");
    rig.wait_for_box();
    rig.press(KeyCode::Down);
    assert_eq!(rig.highlighted().as_deref(), Some("vex"));

    rig.type_text("c");
    assert_eq!(rig.highlighted().as_deref(), Some("vec"));
    rig.press(KeyCode::Enter);

    assert_eq!(rig.text(), "vec\n");
}

#[test]
fn a_modified_enter_is_not_a_pick() {
    let mut rig = Rig::with_answer(items(&["va", "vb"]));
    rig.type_text("v");
    rig.wait_for_box();
    rig.press(KeyCode::Down);

    rig.app.handle_input(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
    ));

    assert_ne!(rig.text(), "vb");
}

fn ctrl_space(rig: &mut Rig) {
    rig.app.handle_input(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL),
    ));
    rig.frame();
}

#[test]
fn ctrl_space_shows_the_suggestions_with_nothing_typed() {
    let mut rig = Rig::with_answer(items(&["alpha", "beta"]));

    ctrl_space(&mut rig);
    rig.wait_for_box();

    assert_eq!(rig.shown(), ["alpha", "beta"]);
    assert_eq!(rig.text(), "", "no space was typed");
    let requests = rig.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["context"], json!({ "triggerKind": 1 }));
}

#[test]
fn ctrl_space_in_a_word_narrows_the_list_to_it() {
    let mut rig = Rig::with_answer(items(&["alpha", "alphabet", "beta"]));
    rig.type_text("al");
    rig.wait_for_box();
    rig.press(KeyCode::Esc);
    assert!(rig.app.state().completion_box().is_none());

    ctrl_space(&mut rig);
    rig.wait_for_box();

    assert_eq!(rig.shown(), ["alpha", "alphabet"]);
}

#[test]
fn the_box_from_ctrl_space_stays_open_and_a_suggestion_can_be_picked() {
    let mut rig = Rig::with_answer(items(&["alpha", "beta"]));
    ctrl_space(&mut rig);
    rig.wait_for_box();
    rig.frame();
    assert!(
        rig.app.state().completion_box().is_some(),
        "not closed for being empty"
    );

    rig.press(KeyCode::Down);
    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "beta");
}

#[test]
fn ctrl_space_with_a_server_that_has_no_suggestions_says_so_and_asks_nothing() {
    let mut rig = Rig::start(
        json!({ "textDocumentSync": 2 }),
        "",
        answering(items(&["a"])),
    );

    ctrl_space(&mut rig);

    assert!(rig.app.state().completion_box().is_none());
    assert!(
        rig.app
            .state()
            .notifications()
            .texts()
            .iter()
            .any(|text| text.contains("no language server with suggestions")),
    );
    assert!(rig.requests().is_empty());
}
