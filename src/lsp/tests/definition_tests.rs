use serde_json::{Value, json};

use crate::lsp::definition::{offered, request};
use crate::lsp::position::LspPosition;
use crate::lsp::{LspError, parse_definition};

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

#[test]
fn nothing_is_no_locations() {
    assert_eq!(parse_definition(&Value::Null).unwrap(), []);
    assert_eq!(parse_definition(&json!([])).unwrap(), []);
}

#[test]
fn one_location_is_read() {
    let answer = json!({ "uri": "file:///p/a.rs", "range": range((3, 4), (3, 9)) });

    let found = parse_definition(&answer).unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].uri, "file:///p/a.rs");
    assert_eq!((found[0].start, found[0].end), (at(3, 4), at(3, 9)));
}

#[test]
fn a_list_of_locations_is_read_in_order() {
    let answer = json!([
        { "uri": "file:///p/a.rs", "range": range((1, 0), (1, 3)) },
        { "uri": "file:///p/b.rs", "range": range((7, 2), (7, 5)) },
    ]);

    let found = parse_definition(&answer).unwrap();

    let uris: Vec<_> = found.iter().map(|location| location.uri.as_str()).collect();
    assert_eq!(uris, ["file:///p/a.rs", "file:///p/b.rs"]);
}

#[test]
fn a_link_gives_the_range_of_the_name_not_of_the_whole_item() {
    let answer = json!([{
        "targetUri": "file:///p/a.rs",
        "targetRange": range((10, 0), (20, 1)),
        "targetSelectionRange": range((10, 7), (10, 12)),
        "originSelectionRange": range((0, 0), (0, 3)),
    }]);

    let found = parse_definition(&answer).unwrap();

    assert_eq!((found[0].start, found[0].end), (at(10, 7), at(10, 12)));
}

#[test]
fn a_link_without_a_name_range_gives_the_whole_range() {
    let answer = json!([{ "targetUri": "file:///p/a.rs", "targetRange": range((10, 0), (20, 1)) }]);

    let found = parse_definition(&answer).unwrap();

    assert_eq!(found[0].start, at(10, 0));
}

#[test]
fn the_same_place_twice_is_one_and_a_broken_one_is_left_out() {
    let place = json!({ "uri": "file:///p/a.rs", "range": range((1, 0), (1, 3)) });
    let answer =
        json!([place, place, { "uri": "file:///p/b.rs" }, { "range": range((0, 0), (0, 1)) }, 7]);

    let found = parse_definition(&answer).unwrap();

    assert_eq!(found.len(), 1);
}

#[test]
fn an_answer_that_is_not_a_location_is_an_error_naming_the_method() {
    for answer in [json!("a.rs"), json!(3), json!(true)] {
        let Err(LspError::Unexpected { method, .. }) = parse_definition(&answer) else {
            panic!("{answer} was read as a location");
        };
        assert_eq!(method, "textDocument/definition");
    }
}

#[test]
fn a_server_says_whether_it_answers_definition() {
    assert!(offered(&json!({ "definitionProvider": true })));
    assert!(offered(
        &json!({ "definitionProvider": { "workDoneProgress": true } })
    ));
    assert!(!offered(&json!({ "definitionProvider": false })));
    assert!(!offered(&json!({})));
}

#[test]
fn the_request_names_the_document_and_the_position_and_the_answer_comes_back_read() {
    let rig = connect(|server| {
        server.start_up();
        let (id, params) = server.expect_request("textDocument/definition");
        assert_eq!(params["textDocument"], json!({ "uri": "file:///p/a.rs" }));
        assert_eq!(params["position"], json!({ "line": 2, "character": 7 }));
        server.respond(
            id,
            json!({ "uri": "file:///p/b.rs", "range": range((1, 1), (1, 4)) }),
        );
    });
    crate::lsp::initialize(
        &rig.connection,
        std::path::Path::new(super::handshake_tests::root_for_tests()),
        &Value::Null,
        PATIENCE,
    )
    .unwrap();

    let mut pending = request(&rig.connection, "file:///p/a.rs", at(2, 7)).unwrap();
    let deadline = std::time::Instant::now() + PATIENCE;
    let answer = loop {
        if let Some(answer) = pending.try_take() {
            break answer;
        }
        assert!(std::time::Instant::now() < deadline, "no answer");
        std::thread::sleep(std::time::Duration::from_millis(2));
    };

    let found = answer.unwrap();
    assert_eq!(found[0].uri, "file:///p/b.rs");
    rig.server.finish();
}

#[test]
fn the_client_asks_for_links() {
    let capabilities = crate::lsp::handshake::client_capabilities();

    assert_eq!(
        capabilities["textDocument"]["definition"]["linkSupport"],
        json!(true)
    );
}
