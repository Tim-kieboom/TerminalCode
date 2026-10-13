use serde_json::{Value, json};

use crate::lsp::hover::{offered, request};
use crate::lsp::position::LspPosition;
use crate::lsp::{LspError, parse_hover};

use super::support::{PATIENCE, connect};

fn text(answer: Value) -> Option<String> {
    parse_hover(&answer).unwrap()
}

#[test]
fn nothing_to_say_is_none() {
    assert_eq!(text(Value::Null), None);
    assert_eq!(text(json!({ "contents": "" })), None);
    assert_eq!(text(json!({ "contents": [] })), None);
    assert_eq!(
        text(json!({ "contents": { "kind": "markdown", "value": "  \n" } })),
        None
    );
}

#[test]
fn markup_is_read_as_plain_text() {
    let answer = json!({
        "contents": {
            "kind": "markdown",
            "value": "```rust\nstd::vec::Vec<u32>\n```\n\n---\n\nA **growable** array.",
        },
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 3 } },
    });

    assert_eq!(
        text(answer).unwrap(),
        "std::vec::Vec<u32>\n\nA growable array."
    );
}

#[test]
fn a_plain_string_and_a_marked_string_and_a_list_of_them_are_read() {
    assert_eq!(
        text(json!({ "contents": "just text" })).unwrap(),
        "just text"
    );
    assert_eq!(
        text(json!({ "contents": { "language": "rust", "value": "fn f()" } })).unwrap(),
        "fn f()"
    );
    let list = json!({ "contents": ["first", { "language": "rust", "value": "fn f()" }, 7] });
    assert_eq!(text(list).unwrap(), "first\n\nfn f()");
}

#[test]
fn an_answer_that_is_not_hover_is_an_error_naming_the_method() {
    for answer in [json!("text"), json!(3), json!({ "range": 1 })] {
        let Err(LspError::Unexpected { method, .. }) = parse_hover(&answer) else {
            panic!("{answer} was read as hover");
        };
        assert_eq!(method, "textDocument/hover");
    }
}

#[test]
fn a_server_says_whether_it_answers_hover() {
    assert!(offered(&json!({ "hoverProvider": true })));
    assert!(offered(
        &json!({ "hoverProvider": { "workDoneProgress": false } })
    ));
    assert!(!offered(&json!({ "hoverProvider": false })));
    assert!(!offered(&json!({})));
}

#[test]
fn the_request_names_the_document_and_the_position_and_the_answer_comes_back_read() {
    let rig = connect(|server| {
        server.start_up();
        let (id, params) = server.expect_request("textDocument/hover");
        assert_eq!(params["textDocument"], json!({ "uri": "file:///p/a.rs" }));
        assert_eq!(params["position"], json!({ "line": 2, "character": 7 }));
        server.respond(id, json!({ "contents": "fn f()" }));
    });
    crate::lsp::initialize(
        &rig.connection,
        std::path::Path::new(super::handshake_tests::root_for_tests()),
        &Value::Null,
        PATIENCE,
    )
    .unwrap();

    let mut pending = request(
        &rig.connection,
        "file:///p/a.rs",
        LspPosition {
            line: 2,
            character: 7,
        },
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

    assert_eq!(answer.unwrap().as_deref(), Some("fn f()"));
    rig.server.finish();
}
