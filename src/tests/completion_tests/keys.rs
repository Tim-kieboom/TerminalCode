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
