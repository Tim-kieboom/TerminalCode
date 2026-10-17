use super::*;

#[test]
fn an_item_with_data_is_resolved_when_picked_and_its_import_goes_in_with_the_word() {
    let data = json!({ "resolve_edits": [import_edit()] });
    let mut rig = Rig::start(provider(), HASH_SOURCE, hash_map_with_data(data));
    rig.type_text("h");
    rig.wait_for_box();
    everything_sent(&rig);

    rig.press(KeyCode::Tab);

    assert_eq!(
        rig.text(),
        "use std::collections::HashMap;\nfn f() {\n    let m: HashMap"
    );
    let sent = everything_sent(&rig);
    let resolve = sent
        .iter()
        .position(|message| matches!(message, Message::Request { method, .. } if method == "completionItem/resolve"))
        .expect("the item was resolved");
    // The server knew the text as it was before it was asked.
    let changed = sent
        .iter()
        .position(|message| matches!(message, Message::Notification { method, .. } if method == "textDocument/didChange"));
    assert!(changed.is_none() || changed < Some(resolve), "{sent:?}");
    let Message::Request { params, .. } = &sent[resolve] else {
        unreachable!()
    };
    assert_eq!(params["label"], json!("HashMap"));
    rig.app.state_mut().edit(|editor| editor.undo().unwrap());
    assert_eq!(rig.text(), format!("{HASH_SOURCE}h"));
}

#[test]
fn a_server_that_refuses_to_resolve_gets_the_word_written_without_the_import_and_the_user_is_told()
{
    let data = json!({ "fail": true });
    let mut rig = Rig::start(provider(), HASH_SOURCE, hash_map_with_data(data));
    rig.type_text("h");
    rig.wait_for_box();

    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "fn f() {\n    let m: HashMap");
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("did not say what to import"), "{told}");
}

#[test]
fn an_item_without_data_is_not_resolved_when_the_server_does_not_resolve() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("ve");
    rig.wait_for_box();
    everything_sent(&rig);

    rig.press(KeyCode::Tab);

    let sent = everything_sent(&rig);
    assert!(
        !sent.iter().any(|message| matches!(message, Message::Request { method, .. } if method == "completionItem/resolve")),
        "{sent:?}"
    );
}

#[test]
fn a_server_that_says_it_resolves_has_every_item_resolved() {
    let capabilities = json!({
        "textDocumentSync": 2,
        "completionProvider": { "resolveProvider": true },
    });
    let mut rig = Rig::start(capabilities, "", answering(items(&["vec"])));
    rig.type_text("ve");
    rig.wait_for_box();
    everything_sent(&rig);

    rig.press(KeyCode::Tab);

    let sent = everything_sent(&rig);
    assert!(
        sent.iter().any(|message| matches!(message, Message::Request { method, .. } if method == "completionItem/resolve")),
        "{sent:?}"
    );
    assert_eq!(rig.text(), "vec");
}
