use serde_json::{Value, json};

use crate::position::LspPosition;
use crate::rename::{offered, prepare_offered, request_prepare, request_rename};
use crate::{LspError, parse_prepare, parse_workspace_edit};

use crate::test_support::{PATIENCE, connect};

fn at(line: u32, character: u32) -> LspPosition {
    LspPosition { line, character }
}

fn range(start: (u32, u32), end: (u32, u32)) -> Value {
    json!({
        "start": { "line": start.0, "character": start.1 },
        "end": { "line": end.0, "character": end.1 },
    })
}

fn edit(start: (u32, u32), end: (u32, u32), text: &str) -> Value {
    json!({ "range": range(start, end), "newText": text })
}

#[test]
fn nothing_to_prepare_is_none() {
    assert_eq!(parse_prepare(&Value::Null).unwrap(), None);
}

#[test]
fn a_range_is_a_name_without_a_placeholder() {
    let found = parse_prepare(&range((1, 4), (1, 9))).unwrap().unwrap();

    assert_eq!(found.range, Some((at(1, 4), at(1, 9))));
    assert_eq!(found.placeholder, None);
}

#[test]
fn a_range_with_a_placeholder_gives_both() {
    let answer = json!({ "range": range((1, 4), (1, 9)), "placeholder": "value" });

    let found = parse_prepare(&answer).unwrap().unwrap();

    assert_eq!(found.range, Some((at(1, 4), at(1, 9))));
    assert_eq!(found.placeholder.as_deref(), Some("value"));
}

#[test]
fn the_default_behavior_leaves_the_word_to_the_client() {
    let found = parse_prepare(&json!({ "defaultBehavior": true }))
        .unwrap()
        .unwrap();

    assert_eq!(found.range, None);
}

#[test]
fn a_prepare_answer_of_another_shape_is_an_error_naming_the_method() {
    for answer in [json!("a"), json!(3), json!({ "range": 3 })] {
        let Err(LspError::Unexpected { method, .. }) = parse_prepare(&answer) else {
            panic!("{answer} was read as a name");
        };
        assert_eq!(method, "textDocument/prepareRename");
    }
}

#[test]
fn no_edit_is_an_empty_edit() {
    let edit = parse_workspace_edit(&Value::Null).unwrap();

    assert!(edit.files.is_empty() && edit.operations.is_empty());
}

#[test]
fn changes_by_uri_are_read() {
    let answer = json!({ "changes": {
        "file:///p/a.rs": [edit((0, 3), (0, 6), "bar"), edit((2, 0), (2, 3), "bar")],
        "file:///p/b.rs": [edit((5, 1), (5, 4), "bar")],
    }});

    let found = parse_workspace_edit(&answer).unwrap();

    assert_eq!(found.files.len(), 2);
    let a = found
        .files
        .iter()
        .find(|f| f.uri == "file:///p/a.rs")
        .unwrap();
    assert_eq!(a.edits.len(), 2);
    assert_eq!((a.edits[0].start, a.edits[0].end), (at(0, 3), at(0, 6)));
    assert_eq!(a.edits[0].new_text, "bar");
}

#[test]
fn document_changes_are_read_and_a_file_named_twice_is_one() {
    let answer = json!({ "documentChanges": [
        { "textDocument": { "uri": "file:///p/a.rs", "version": 3 }, "edits": [edit((0, 3), (0, 6), "bar")] },
        { "textDocument": { "uri": "file:///p/a.rs", "version": 3 }, "edits": [edit((4, 3), (4, 6), "bar")] },
    ]});

    let found = parse_workspace_edit(&answer).unwrap();

    assert_eq!(found.files.len(), 1);
    assert_eq!(found.files[0].edits.len(), 2);
}

#[test]
fn operations_on_files_are_listed_for_the_client_to_refuse() {
    let answer = json!({ "documentChanges": [
        { "kind": "rename", "oldUri": "file:///p/foo.rs", "newUri": "file:///p/bar.rs" },
        { "kind": "create", "uri": "file:///p/new.rs" },
        { "kind": "delete", "uri": "file:///p/old.rs" },
    ]});

    let found = parse_workspace_edit(&answer).unwrap();

    assert_eq!(
        found.operations,
        [
            "rename file:///p/foo.rs to file:///p/bar.rs",
            "create file:///p/new.rs",
            "delete file:///p/old.rs",
        ]
    );
}

#[test]
fn a_broken_edit_is_an_error() {
    for answer in [
        json!("edit"),
        json!({ "changes": { "file:///p/a.rs": 3 } }),
        json!({ "changes": { "file:///p/a.rs": [{ "newText": "x" }] } }),
        json!({ "changes": { "file:///p/a.rs": [{ "range": range((0, 0), (0, 1)) }] } }),
        json!({ "documentChanges": [{ "edits": [] }] }),
    ] {
        let Err(LspError::Unexpected { method, .. }) = parse_workspace_edit(&answer) else {
            panic!("{answer} was read as an edit");
        };
        assert_eq!(method, "textDocument/rename");
    }
}

#[test]
fn a_server_says_whether_it_renames_and_whether_it_prepares() {
    assert!(offered(&json!({ "renameProvider": true })));
    assert!(offered(
        &json!({ "renameProvider": { "prepareProvider": true } })
    ));
    assert!(!offered(&json!({ "renameProvider": false })));
    assert!(!offered(&json!({})));

    assert!(prepare_offered(
        &json!({ "renameProvider": { "prepareProvider": true } })
    ));
    assert!(!prepare_offered(&json!({ "renameProvider": true })));
    assert!(!prepare_offered(&json!({ "renameProvider": {} })));
}

#[test]
fn the_requests_carry_the_position_and_the_new_name() {
    let rig = connect(|server| {
        server.start_up();
        let (id, params) = server.expect_request("textDocument/prepareRename");
        assert_eq!(params["textDocument"], json!({ "uri": "file:///p/a.rs" }));
        assert_eq!(params["position"], json!({ "line": 2, "character": 7 }));
        server.respond(
            id,
            json!({ "range": range((2, 4), (2, 9)), "placeholder": "value" }),
        );

        let (id, params) = server.expect_request("textDocument/rename");
        assert_eq!(params["position"], json!({ "line": 2, "character": 7 }));
        assert_eq!(params["newName"], json!("renamed"));
        server.respond(
            id,
            json!({ "changes": { "file:///p/a.rs": [edit((2, 4), (2, 9), "renamed")] } }),
        );
    });
    crate::initialize(
        &rig.connection,
        std::path::Path::new(super::handshake_tests::root_for_tests()),
        &Value::Null,
        PATIENCE,
    )
    .unwrap();

    let mut prepare = request_prepare(&rig.connection, "file:///p/a.rs", at(2, 7)).unwrap();
    let prepared = loop {
        if let Some(answer) = prepare.try_take() {
            break answer.unwrap().unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert_eq!(prepared.placeholder.as_deref(), Some("value"));

    let mut rename =
        request_rename(&rig.connection, "file:///p/a.rs", at(2, 7), "renamed").unwrap();
    let edit = loop {
        if let Some(answer) = rename.try_take() {
            break answer.unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert_eq!(edit.files[0].edits[0].new_text, "renamed");
    rig.server.finish();
}

#[test]
fn the_client_declares_rename_with_prepare_and_documents() {
    let capabilities = crate::handshake::client_capabilities();

    assert_eq!(
        capabilities["textDocument"]["rename"]["prepareSupport"],
        json!(true)
    );
    assert_eq!(
        capabilities["workspace"]["workspaceEdit"]["documentChanges"],
        json!(true)
    );
}
