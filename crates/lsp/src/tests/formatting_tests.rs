use serde_json::{Value, json};

use crate::formatting::{offered, request};
use crate::test_support::{PATIENCE, connect};
use crate::{LspError, parse_formatting};

#[test]
fn nothing_to_change_is_no_edits() {
    assert!(parse_formatting(&Value::Null).unwrap().is_empty());
    assert!(parse_formatting(&json!([])).unwrap().is_empty());
}

#[test]
fn edits_are_read_in_order() {
    let answer = json!([
        { "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 3 } },
          "newText": "fn" },
        { "range": { "start": { "line": 2, "character": 1 }, "end": { "line": 2, "character": 1 } },
          "newText": "\n" },
    ]);

    let found = parse_formatting(&answer).unwrap();

    assert_eq!(found.len(), 2);
    assert_eq!(found[0].new_text, "fn");
    assert_eq!((found[1].start.line, found[1].start.character), (2, 1));
}

#[test]
fn an_answer_of_another_shape_is_an_error_naming_the_method() {
    for answer in [
        json!("x"),
        json!({ "range": 1 }),
        json!([{ "newText": "a" }]),
    ] {
        let Err(LspError::Unexpected { method, .. }) = parse_formatting(&answer) else {
            panic!("{answer} was read as edits");
        };
        assert_eq!(method, "textDocument/formatting");
    }
}

#[test]
fn a_server_says_whether_it_formats() {
    assert!(offered(&json!({ "documentFormattingProvider": true })));
    assert!(offered(
        &json!({ "documentFormattingProvider": { "workDoneProgress": false } })
    ));
    assert!(!offered(&json!({ "documentFormattingProvider": false })));
    assert!(!offered(&json!({})));
}

#[test]
fn the_request_names_the_document_and_the_options() {
    let rig = connect(|server| {
        server.start_up();
        let (id, params) = server.expect_request("textDocument/formatting");
        assert_eq!(params["textDocument"], json!({ "uri": "file:///p/a.rs" }));
        assert_eq!(
            params["options"],
            json!({ "tabSize": 4, "insertSpaces": true })
        );
        server.respond(id, json!([]));
    });
    crate::initialize(
        &rig.connection,
        std::path::Path::new(super::handshake_tests::root_for_tests()),
        &Value::Null,
        PATIENCE,
    )
    .unwrap();

    let mut pending = request(&rig.connection, "file:///p/a.rs", 4, true).unwrap();
    let deadline = std::time::Instant::now() + PATIENCE;
    let answer = loop {
        if let Some(answer) = pending.try_take() {
            break answer;
        }
        assert!(std::time::Instant::now() < deadline, "no answer");
        std::thread::sleep(std::time::Duration::from_millis(2));
    };

    assert!(answer.unwrap().is_empty());
    rig.server.finish();
}

#[test]
fn the_client_declares_formatting() {
    let capabilities = crate::handshake::client_capabilities();

    assert!(capabilities["textDocument"]["formatting"].is_object());
}

#[test]
fn a_server_says_whether_it_formats_a_range() {
    assert!(super::super::formatting::range_offered(
        &json!({ "documentRangeFormattingProvider": true })
    ));
    assert!(!super::super::formatting::range_offered(
        &json!({ "documentFormattingProvider": true })
    ));
}

#[test]
fn the_range_request_carries_the_range() {
    let rig = connect(|server| {
        server.start_up();
        let (id, params) = server.expect_request("textDocument/rangeFormatting");
        assert_eq!(
            params["range"]["start"],
            json!({ "line": 1, "character": 0 })
        );
        assert_eq!(params["range"]["end"], json!({ "line": 3, "character": 5 }));
        assert_eq!(
            params["options"],
            json!({ "tabSize": 2, "insertSpaces": false })
        );
        server.respond(id, Value::Null);
    });
    crate::initialize(
        &rig.connection,
        std::path::Path::new(super::handshake_tests::root_for_tests()),
        &Value::Null,
        PATIENCE,
    )
    .unwrap();
    let at = |line, character| crate::position::LspPosition { line, character };

    let mut pending = super::super::formatting::request_range(
        &rig.connection,
        "file:///p/a.rs",
        at(1, 0),
        at(3, 5),
        2,
        false,
    )
    .unwrap();
    let deadline = std::time::Instant::now() + PATIENCE;
    let answer = loop {
        if let Some(answer) = pending.try_take() {
            break answer;
        }
        assert!(std::time::Instant::now() < deadline, "no answer");
        std::thread::sleep(std::time::Duration::from_millis(2));
    };

    assert!(answer.unwrap().is_empty());
    rig.server.finish();
}

#[test]
fn the_client_declares_range_formatting() {
    let capabilities = crate::handshake::client_capabilities();

    assert!(capabilities["textDocument"]["rangeFormatting"].is_object());
}
