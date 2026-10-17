use super::*;

#[test]
fn typing_the_first_letter_of_a_word_asks_the_server_and_shows_its_answer() {
    let mut rig = Rig::with_answer(items(&["vec", "vector", "value"]));

    rig.type_text("v");
    rig.wait_for_box();

    let requests = rig.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["context"], json!({ "triggerKind": 1 }));
    assert_eq!(
        requests[0]["position"],
        json!({ "line": 0, "character": 1 })
    );
    // The server's order; `value` sorts by its label like the others.
    assert_eq!(rig.shown(), ["value", "vec", "vector"]);
    assert_eq!(rig.highlighted().as_deref(), Some("value"));
}

#[test]
fn the_box_is_drawn_under_the_cursor_with_kind_name_and_note() {
    let mut rig = Rig::with_answer(items(&["vec", "vector"]));
    rig.type_text("v");
    rig.wait_for_box();

    let screen = rig.screen();

    assert!(screen.contains("f vec"), "{screen}");
    assert!(screen.contains("fn vec()"), "{screen}");
    assert!(screen.contains("f vector"), "{screen}");
}

#[test]
fn only_one_request_goes_out_while_the_word_is_typed_before_the_answer() {
    let mut rig = Rig::with_answer(items(&["vec", "vector"]));

    rig.type_text("vec");
    rig.wait_for_box();

    assert_eq!(rig.requests().len(), 1);
    // The list is for `v` and was narrowed to `vec` as it arrived.
    assert_eq!(rig.shown(), ["vec", "vector"]);
}

#[test]
fn a_complete_list_is_narrowed_as_more_is_typed_without_asking_again() {
    let mut rig = Rig::with_answer(items(&["vec", "vector", "value"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.type_text("ec");

    assert_eq!(rig.shown(), ["vec", "vector"]);
    assert_eq!(rig.requests().len(), 1);
}

#[test]
fn an_incomplete_list_is_asked_for_again_when_the_word_grows() {
    let incomplete = json!({ "isIncomplete": true, "items": items(&["vec", "vector"]) });
    let mut rig = Rig::with_answer(incomplete);
    rig.type_text("v");
    rig.wait_for_box();

    rig.type_text("e");
    rig.pump_until(|_| true);

    let requests = rig.requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert_eq!(requests[1]["context"], json!({ "triggerKind": 3 }));
    assert_eq!(
        requests[1]["position"],
        json!({ "line": 0, "character": 2 })
    );
}

#[test]
fn a_trigger_character_asks_for_everything_that_may_follow_it() {
    let mut rig = Rig::with_answer(items(&["push", "pop"]));

    rig.type_text(".");
    rig.wait_for_box();

    let requests = rig.requests();
    assert_eq!(
        requests.last().unwrap()["context"],
        json!({ "triggerKind": 2, "triggerCharacter": "." })
    );
    // Nothing is typed of the word yet and the box is there all the same.
    assert_eq!(rig.shown(), ["pop", "push"]);
}

#[test]
fn a_word_that_starts_with_a_digit_asks_nothing() {
    let mut rig = Rig::with_answer(items(&["vec"]));

    rig.type_text("12");

    assert!(rig.requests().is_empty());
    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn a_server_that_offers_no_completion_is_not_asked() {
    let mut rig = Rig::start(
        json!({ "textDocumentSync": 2 }),
        "",
        answering(items(&["vec"])),
    );

    rig.type_text("v");

    assert!(rig.requests().is_empty());
    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn text_that_was_not_typed_asks_nothing() {
    let mut rig = Rig::with_answer(items(&["vec"]));

    // A paste (here, the editor's own insert, not a key) is not typing.
    rig.app
        .state_mut()
        .edit(|editor| editor.insert_text("vec").unwrap());
    rig.frame();

    assert!(rig.requests().is_empty());
}
