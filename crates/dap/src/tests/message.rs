use serde_json::{Value, json};

use crate::message::{Message, MessageError, Rejection};

fn read(value: Value) -> Message {
    Message::from_value(value).unwrap()
}

#[test]
fn a_request_has_its_command_and_arguments() {
    let message = Message::Request {
        seq: 3,
        command: "next".into(),
        arguments: json!({ "threadId": 1 }),
    };

    let value = message.to_value();

    assert_eq!(
        value,
        json!({ "seq": 3, "type": "request", "command": "next", "arguments": { "threadId": 1 } })
    );
    assert_eq!(read(value), message);
}

#[test]
fn a_request_without_arguments_is_written_without_them_and_read_back() {
    let message = Message::Request {
        seq: 1,
        command: "configurationDone".into(),
        arguments: Value::Null,
    };

    let value = message.to_value();

    assert!(value.get("arguments").is_none(), "{value}");
    assert_eq!(read(value), message);
}

#[test]
fn a_good_response_names_the_request_it_answers() {
    let message = Message::Response {
        seq: 8,
        request_seq: 3,
        command: "threads".into(),
        outcome: Ok(json!({ "threads": [] })),
    };

    let value = message.to_value();

    assert_eq!(value["success"], true);
    assert_eq!(value["request_seq"], 3);
    assert_eq!(read(value), message);
}

#[test]
fn a_refusal_is_a_response_that_is_not_a_success_with_a_short_message() {
    let message = Message::Response {
        seq: 9,
        request_seq: 4,
        command: "next".into(),
        outcome: Err(Rejection {
            message: "notStopped".into(),
            body: json!({ "error": { "id": 2, "format": "the program is running" } }),
        }),
    };

    let value = message.to_value();

    assert_eq!(value["success"], false);
    assert_eq!(value["message"], "notStopped");
    assert_eq!(read(value), message);
}

#[test]
fn an_event_has_its_name_and_body() {
    let message = Message::Event {
        seq: 5,
        event: "stopped".into(),
        body: json!({ "reason": "step" }),
    };

    assert_eq!(read(message.to_value()), message);
}

#[test]
fn a_message_that_is_not_one_says_what_is_missing() {
    let cases = [
        (json!([1]), MessageError::NotAnObject),
        (
            json!({ "type": "event", "event": "x" }),
            MessageError::Missing("seq"),
        ),
        (
            json!({ "seq": 1, "event": "x" }),
            MessageError::Missing("type"),
        ),
        (
            json!({ "seq": 1, "type": "request" }),
            MessageError::Missing("command"),
        ),
        (
            json!({ "seq": 1, "type": "event" }),
            MessageError::Missing("event"),
        ),
        (
            json!({ "seq": 1, "type": "response", "command": "c", "success": true }),
            MessageError::Missing("request_seq"),
        ),
        (
            json!({ "seq": 1, "type": "response", "request_seq": 1, "command": "c" }),
            MessageError::Missing("success"),
        ),
        (
            json!({ "seq": 1, "type": "mystery" }),
            MessageError::Kind("mystery".into()),
        ),
    ];
    for (value, error) in cases {
        assert_eq!(Message::from_value(value.clone()), Err(error), "{value}");
    }
}

#[test]
fn a_refusal_with_no_message_or_body_still_reads() {
    let value =
        json!({ "seq": 1, "type": "response", "request_seq": 2, "command": "c", "success": false });

    let Message::Response {
        outcome: Err(rejection),
        ..
    } = read(value)
    else {
        panic!("not a refusal");
    };

    assert_eq!(rejection.message, "");
    assert_eq!(rejection.describe(), "");
}

#[test]
fn what_to_tell_the_user_is_the_adapters_own_words_with_its_placeholders_filled() {
    let rejection = Rejection {
        message: "failed".into(),
        body: json!({ "error": {
            "id": 1,
            "format": "could not set a breakpoint in {file} at line {line}",
            "variables": { "file": "src/a.rs", "line": 12 },
        } }),
    };

    assert_eq!(
        rejection.to_string(),
        "could not set a breakpoint in src/a.rs at line 12"
    );
}

#[test]
fn without_words_of_its_own_the_short_message_is_what_is_told() {
    assert_eq!(Rejection::new("cancelled").describe(), "cancelled");
    let no_format = Rejection {
        message: "failed".into(),
        body: json!({ "error": { "id": 1 } }),
    };
    assert_eq!(no_format.describe(), "failed");
}
