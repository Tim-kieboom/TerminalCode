use serde_json::json;

use crate::message::MessageError;
use crate::{Id, Message, RpcError};

fn parse(value: serde_json::Value) -> Result<Message, MessageError> {
    Message::from_value(value)
}

#[test]
fn a_message_with_a_method_and_an_id_is_a_request() {
    let message = parse(json!({ "jsonrpc": "2.0", "id": 7, "method": "a/b", "params": [1] }));

    assert_eq!(
        message,
        Ok(Message::Request {
            id: Id::Number(7),
            method: "a/b".to_owned(),
            params: json!([1]),
        })
    );
}

#[test]
fn a_message_with_a_method_and_no_id_is_a_notification() {
    let message = parse(json!({ "jsonrpc": "2.0", "method": "a/b" }));

    assert_eq!(
        message,
        Ok(Message::Notification {
            method: "a/b".to_owned(),
            params: json!(null),
        })
    );
}

#[test]
fn an_id_may_be_a_string() {
    let message = parse(json!({ "id": "abc", "method": "m" }));

    assert!(matches!(message, Ok(Message::Request { id: Id::String(text), .. }) if text == "abc"));
}

#[test]
fn a_result_is_an_answer_even_when_it_is_null() {
    let message = parse(json!({ "id": 1, "result": null }));

    assert_eq!(
        message,
        Ok(Message::Response {
            id: Id::Number(1),
            outcome: Ok(json!(null)),
        })
    );
}

#[test]
fn an_error_is_an_answer_with_its_code_message_and_data() {
    let message = parse(json!({
        "id": 2,
        "error": { "code": -32601, "message": "nope", "data": { "x": 1 } },
    }));

    assert_eq!(
        message,
        Ok(Message::Response {
            id: Id::Number(2),
            outcome: Err(RpcError {
                code: -32601,
                message: "nope".to_owned(),
                data: Some(json!({ "x": 1 })),
            }),
        })
    );
}

#[test]
fn an_error_that_is_not_an_object_is_kept_as_text() {
    let message = parse(json!({ "id": 2, "error": "bad" }));

    let Ok(Message::Response {
        outcome: Err(error),
        ..
    }) = message
    else {
        panic!("{message:?}");
    };
    assert!(error.message.contains("bad"), "{error}");
}

#[test]
fn an_answer_with_no_id_cannot_be_given_to_anyone() {
    let message = parse(json!({ "id": null, "error": { "code": -32700, "message": "parse" } }));

    assert_eq!(message, Err(MessageError::Unroutable));
}

#[test]
fn what_is_not_a_message_is_an_error() {
    assert_eq!(parse(json!([1, 2])), Err(MessageError::NotAnObject));
    assert_eq!(parse(json!({ "id": 1 })), Err(MessageError::Unknown));
    assert_eq!(
        parse(json!({ "id": 1.5, "method": "m" })),
        Err(MessageError::BadId)
    );
    assert_eq!(
        parse(json!({ "id": [1], "result": 1 })),
        Err(MessageError::BadId)
    );
}

#[test]
fn messages_turn_back_into_json_and_read_again_as_they_were() {
    for message in [
        Message::Request {
            id: Id::Number(3),
            method: "m".to_owned(),
            params: json!({ "a": 1 }),
        },
        Message::Notification {
            method: "n".to_owned(),
            params: json!([]),
        },
        Message::Response {
            id: Id::String("x".to_owned()),
            outcome: Ok(json!({ "ok": true })),
        },
        Message::Response {
            id: Id::Number(4),
            outcome: Err(RpcError {
                code: 1,
                message: "m".to_owned(),
                data: None,
            }),
        },
    ] {
        assert_eq!(
            parse(message.to_value()),
            Ok(message.clone()),
            "{message:?}"
        );
    }
}

#[test]
fn a_null_params_is_left_out_and_every_message_says_2_0() {
    let value = Message::Notification {
        method: "exit".to_owned(),
        params: json!(null),
    }
    .to_value();

    assert_eq!(value, json!({ "jsonrpc": "2.0", "method": "exit" }));
}
