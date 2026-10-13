use serde_json::{Value, json};

use crate::lsp::completion::{parse_completion, request};
use crate::lsp::position::LspPosition;
use crate::lsp::{CompletionList, Kind, LspError, Provider, Server, TextEdit, Trigger};

use super::support::{PATIENCE, connect};

fn at(line: u32, character: u32) -> LspPosition {
    LspPosition { line, character }
}

fn range(start: (u32, u32), end: (u32, u32)) -> Value {
    json!({
        "start": { "line": start.0, "character": start.1 },
        "end": { "line": end.0, "character": end.1 },
    })
}

// ---- what the server says it can do

#[test]
fn a_server_with_a_completion_provider_names_its_trigger_characters() {
    let capabilities = json!({
        "completionProvider": { "triggerCharacters": [":", ".", "::", "", 5] },
    });

    let provider = Provider::from_capabilities(&capabilities).unwrap();

    // Only one-character triggers count: `::` is not one, and `:` already covers it.
    assert_eq!(provider.trigger_characters, [':', '.']);
    assert!(provider.triggers_on('.'));
    assert!(!provider.triggers_on('a'));
}

#[test]
fn a_provider_without_triggers_still_offers_completion() {
    let provider = Provider::from_capabilities(&json!({ "completionProvider": {} })).unwrap();

    assert!(provider.trigger_characters.is_empty());
}

#[test]
fn a_server_without_a_provider_offers_no_completion() {
    assert_eq!(Provider::from_capabilities(&json!({})), None);
    assert_eq!(
        Provider::from_capabilities(&json!({ "completionProvider": false })),
        None
    );
}

#[test]
fn the_server_from_the_handshake_reports_its_provider() {
    let server = Server {
        name: None,
        capabilities: json!({ "completionProvider": { "triggerCharacters": ["."] } }),
    };

    assert_eq!(server.completion().unwrap().trigger_characters, ['.']);
}

// ---- reading the answer

#[test]
fn nothing_is_an_empty_complete_list() {
    assert_eq!(
        parse_completion(&Value::Null).unwrap(),
        CompletionList::default()
    );
}

#[test]
fn a_plain_array_is_a_complete_list() {
    let list = parse_completion(&json!([{ "label": "push" }, { "label": "pop" }])).unwrap();

    assert!(!list.incomplete);
    let labels: Vec<_> = list.items.iter().map(|item| item.label.as_str()).collect();
    assert_eq!(labels, ["push", "pop"]);
}

#[test]
fn a_list_object_says_whether_it_is_incomplete() {
    let incomplete =
        parse_completion(&json!({ "isIncomplete": true, "items": [{ "label": "vec" }] })).unwrap();
    let complete = parse_completion(&json!({ "items": [{ "label": "vec" }] })).unwrap();

    assert!(incomplete.incomplete);
    assert!(!complete.incomplete);
}

#[test]
fn an_item_has_every_field_the_box_and_the_edit_need() {
    let item = json!({
        "label": "HashMap",
        "kind": 22,
        "detail": "std::collections",
        "sortText": "0001",
        "filterText": "hashmap",
        "insertText": "HashMap",
        "textEdit": { "range": range((2, 4), (2, 7)), "newText": "HashMap" },
        "additionalTextEdits": [
            { "range": range((0, 0), (0, 0)), "newText": "use std::collections::HashMap;\n" },
        ],
    });

    let list = parse_completion(&json!([item])).unwrap();

    let item = &list.items[0];
    assert_eq!(item.label, "HashMap");
    assert_eq!(item.kind, Kind::Struct);
    assert_eq!(item.detail.as_deref(), Some("std::collections"));
    assert_eq!(item.sort_text, "0001");
    assert_eq!(item.filter_text, "hashmap");
    assert_eq!(item.insert_text.as_deref(), Some("HashMap"));
    assert_eq!(
        item.text_edit,
        Some(TextEdit {
            start: at(2, 4),
            end: at(2, 7),
            new_text: "HashMap".to_owned(),
        })
    );
    assert_eq!(
        item.additional_edits,
        [TextEdit {
            start: at(0, 0),
            end: at(0, 0),
            new_text: "use std::collections::HashMap;\n".to_owned(),
        }]
    );
    assert!(!item.snippet);
}

#[test]
fn what_an_item_leaves_out_falls_back_to_its_label() {
    let list = parse_completion(&json!([{ "label": "len" }])).unwrap();

    let item = &list.items[0];
    assert_eq!(item.sort_text, "len");
    assert_eq!(item.filter_text, "len");
    assert_eq!(item.kind, Kind::Other);
    assert_eq!(item.detail, None);
    assert_eq!(item.insert_text, None);
    assert_eq!(item.text_edit, None);
    assert!(item.additional_edits.is_empty());
}

#[test]
fn the_detail_falls_back_to_the_label_details_and_an_empty_one_is_none() {
    let list = parse_completion(&json!([
        { "label": "a", "labelDetails": { "detail": "(x: u8)" } },
        { "label": "b", "detail": "", "labelDetails": { "detail": "(y)" } },
        { "label": "c", "detail": "" },
    ]))
    .unwrap();

    assert_eq!(list.items[0].detail.as_deref(), Some("(x: u8)"));
    assert_eq!(list.items[1].detail.as_deref(), Some("(y)"));
    assert_eq!(list.items[2].detail, None);
}

#[test]
fn an_insert_or_replace_edit_gives_its_insert_half() {
    let edit = json!({
        "newText": "push",
        "insert": range((1, 2), (1, 4)),
        "replace": range((1, 2), (1, 9)),
    });

    let list = parse_completion(&json!([{ "label": "push", "textEdit": edit }])).unwrap();

    let edit = list.items[0].text_edit.as_ref().unwrap();
    assert_eq!((edit.start, edit.end), (at(1, 2), at(1, 4)));
}

#[test]
fn an_item_marked_as_a_snippet_says_so() {
    let list = parse_completion(&json!([
        { "label": "a", "insertTextFormat": 2 },
        { "label": "b", "insertTextFormat": 1 },
    ]))
    .unwrap();

    assert!(list.items[0].snippet);
    assert!(!list.items[1].snippet);
}

#[test]
fn every_kind_has_its_own_tag_where_it_matters() {
    let kinds: Vec<Kind> = (1..=25)
        .map(|code| {
            let list = parse_completion(&json!([{ "label": "x", "kind": code }])).unwrap();
            list.items[0].kind
        })
        .collect();

    assert!(!kinds.contains(&Kind::Other), "{kinds:?}");
    assert_eq!(kinds[2].tag(), 'f');
    assert_eq!(kinds[21].tag(), 's');
    assert_eq!(Kind::Other.tag(), '?');
    // Field and property share a tag; nothing else does.
    let mut tags: Vec<char> = kinds.iter().map(|kind| kind.tag()).collect();
    tags.sort_unstable();
    tags.dedup();
    assert_eq!(tags.len(), 24);
}

#[test]
fn an_item_without_a_label_or_a_broken_edit_is_dropped_not_the_whole_list() {
    let list = parse_completion(&json!([
        { "kind": 3 },
        { "label": "ok", "textEdit": { "range": "nowhere", "newText": "ok" } },
        { "label": 7 },
    ]))
    .unwrap();

    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].label, "ok");
    assert_eq!(list.items[0].text_edit, None);
}

#[test]
fn an_answer_that_is_not_a_list_is_an_error_naming_the_method() {
    for answer in [json!("push"), json!(3), json!({ "isIncomplete": true })] {
        let Err(LspError::Unexpected { method, .. }) = parse_completion(&answer) else {
            panic!("{answer} was read as a list");
        };
        assert_eq!(method, "textDocument/completion");
    }
}

// ---- over a connection

#[test]
fn the_request_names_the_document_the_position_and_why_it_is_made() {
    let rig = connect(|server| {
        server.start_up();
        for expected in [
            json!({ "triggerKind": 1 }),
            json!({ "triggerKind": 2, "triggerCharacter": "." }),
            json!({ "triggerKind": 3 }),
        ] {
            let (id, params) = server.expect_request("textDocument/completion");
            assert_eq!(params["textDocument"], json!({ "uri": "file:///p/a.rs" }));
            assert_eq!(params["position"], json!({ "line": 3, "character": 9 }));
            assert_eq!(params["context"], expected);
            server.respond(id, Value::Null);
        }
    });
    crate::lsp::initialize(
        &rig.connection,
        std::path::Path::new(super::handshake_tests::root_for_tests()),
        &Value::Null,
        PATIENCE,
    )
    .unwrap();

    for trigger in [
        Trigger::Typing,
        Trigger::Character('.'),
        Trigger::Incomplete,
    ] {
        let pending = request(&rig.connection, "file:///p/a.rs", at(3, 9), trigger).unwrap();
        assert_eq!(pending.wait(PATIENCE).unwrap(), CompletionList::default());
    }

    rig.server.finish();
}

#[test]
fn the_list_the_server_sends_comes_back_read() {
    let rig = connect(|server| {
        server.start_up();
        let (id, _) = server.expect_request("textDocument/completion");
        server.respond(
            id,
            json!({ "isIncomplete": true, "items": [{ "label": "vec", "kind": 3 }] }),
        );
    });
    crate::lsp::initialize(
        &rig.connection,
        std::path::Path::new(super::handshake_tests::root_for_tests()),
        &Value::Null,
        PATIENCE,
    )
    .unwrap();

    let list = request(&rig.connection, "file:///p/a.rs", at(0, 1), Trigger::Typing)
        .unwrap()
        .wait(PATIENCE)
        .unwrap();

    assert!(list.incomplete);
    assert_eq!(list.items[0].label, "vec");
    assert_eq!(list.items[0].kind, Kind::Function);
    rig.server.finish();
}

#[test]
fn a_server_that_refuses_the_request_is_an_error_not_an_empty_box() {
    let rig = connect(|server| {
        server.start_up();
        let (id, _) = server.expect_request("textDocument/completion");
        server.respond_error(id, -32801, "content modified");
    });
    crate::lsp::initialize(
        &rig.connection,
        std::path::Path::new(super::handshake_tests::root_for_tests()),
        &Value::Null,
        PATIENCE,
    )
    .unwrap();

    let result = request(&rig.connection, "file:///p/a.rs", at(0, 1), Trigger::Typing)
        .unwrap()
        .wait(PATIENCE);

    assert!(
        matches!(result, Err(LspError::Rejected { .. })),
        "{result:?}"
    );
    rig.server.finish();
}
