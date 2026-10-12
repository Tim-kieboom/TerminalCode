use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use serde_json::{Value, json};

use crate::lsp::{Id, Incoming, LspError};

use super::support::{PATIENCE, connect, connect_with_wake};

#[test]
fn a_request_gets_the_answer_with_its_id() {
    let rig = connect(|server| {
        let (id, params) = server.expect_request("greet");
        assert_eq!(params, json!({ "name": "tim" }));
        server.respond(id, json!({ "hello": "tim" }));
    });

    let answer = rig
        .connection
        .request("greet", json!({ "name": "tim" }))
        .unwrap()
        .wait(PATIENCE)
        .unwrap();

    assert_eq!(answer, json!({ "hello": "tim" }));
    rig.server.finish();
}

#[test]
fn answers_in_another_order_go_to_the_right_requests() {
    let rig = connect(|server| {
        let (first, _) = server.expect_request("first");
        let (second, _) = server.expect_request("second");
        server.respond(second, json!("two"));
        server.respond(first, json!("one"));
    });

    let first = rig.connection.request("first", Value::Null).unwrap();
    let second = rig.connection.request("second", Value::Null).unwrap();

    assert_eq!(second.wait(PATIENCE).unwrap(), json!("two"));
    assert_eq!(first.wait(PATIENCE).unwrap(), json!("one"));
    rig.server.finish();
}

#[test]
fn each_request_has_an_id_of_its_own() {
    let rig = connect(|server| {
        let (first, _) = server.expect_request("a");
        let (second, _) = server.expect_request("b");
        assert_ne!(first, second);
        assert_eq!(first, Id::Number(1));
        server.respond(first, Value::Null);
        server.respond(second, Value::Null);
    });

    let a = rig.connection.request("a", Value::Null).unwrap();
    let b = rig.connection.request("b", Value::Null).unwrap();
    a.wait(PATIENCE).unwrap();
    b.wait(PATIENCE).unwrap();
    rig.server.finish();
}

#[test]
fn an_error_answer_says_which_request_was_refused_and_why() {
    let rig = connect(|server| {
        let (id, _) = server.expect_request("risky");
        server.respond_error(id, -32803, "the server cancelled it");
    });

    let error = rig
        .connection
        .request("risky", Value::Null)
        .unwrap()
        .wait(PATIENCE)
        .unwrap_err();

    let LspError::Rejected { method, error } = error else {
        panic!("{error}");
    };
    assert_eq!(method, "risky");
    assert_eq!(error.code, -32803);
    assert_eq!(error.message, "the server cancelled it");
    rig.server.finish();
}

#[test]
fn a_request_that_is_never_answered_times_out_and_a_late_answer_is_dropped() {
    let (late, release) = mpsc::channel::<()>();
    let rig = connect(move |server| {
        let (id, _) = server.expect_request("slow");
        release.recv().unwrap();
        server.respond(id, json!("too late"));
        let (id, _) = server.expect_request("next");
        server.respond(id, json!("fine"));
    });

    let error = rig
        .connection
        .request("slow", Value::Null)
        .unwrap()
        .wait(Duration::from_millis(50))
        .unwrap_err();
    assert!(
        matches!(error, LspError::Timeout { ref method, .. } if method == "slow"),
        "{error}"
    );
    late.send(()).unwrap();

    // The late answer did not confuse the next request.
    let next = rig
        .connection
        .request("next", Value::Null)
        .unwrap()
        .wait(PATIENCE)
        .unwrap();
    assert_eq!(next, json!("fine"));
    rig.server.finish();
}

#[test]
fn a_notification_reaches_the_server_with_no_id() {
    let rig = connect(|server| {
        let params = server.expect_notification("textDocument/didSave");
        assert_eq!(params, json!({ "uri": "file:///a.rs" }));
    });

    rig.connection
        .notify("textDocument/didSave", json!({ "uri": "file:///a.rs" }))
        .unwrap();

    rig.server.finish();
}

#[test]
fn a_notification_from_the_server_arrives_for_the_app_in_order() {
    let rig = connect(|server| {
        server.notify("one", json!(1));
        server.notify("two", json!(2));
    });

    assert_eq!(
        rig.next_incoming(),
        Incoming::Notification {
            method: "one".to_owned(),
            params: json!(1),
        }
    );
    assert_eq!(
        rig.next_incoming(),
        Incoming::Notification {
            method: "two".to_owned(),
            params: json!(2),
        }
    );
    rig.server.finish();
}

#[test]
fn the_app_is_woken_for_each_notification_and_for_the_end() {
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&woken);
    let rig = connect_with_wake(
        |server| {
            server.notify("a", Value::Null);
            server.notify("b", Value::Null);
        },
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
        },
    );

    rig.next_incoming();
    rig.next_incoming();
    assert!(matches!(rig.next_incoming(), Incoming::Closed(_)));
    rig.server.finish();

    // The wake comes after the message is sent, so the last one can be a moment behind.
    let deadline = std::time::Instant::now() + PATIENCE;
    while woken.load(Ordering::SeqCst) < 3 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(woken.load(Ordering::SeqCst), 3);
}

#[test]
fn a_request_from_the_server_is_answered_by_the_client_itself() {
    let rig = connect(|server| {
        server.request(
            40,
            "window/workDoneProgress/create",
            json!({ "token": "t" }),
        );
        let (id, outcome) = server.expect_response();
        assert_eq!(id, Id::Number(40));
        assert_eq!(outcome, Ok(Value::Null));

        server.request(41, "workspace/configuration", json!({ "items": [{}, {}] }));
        let (id, outcome) = server.expect_response();
        assert_eq!(id, Id::Number(41));
        assert_eq!(outcome, Ok(json!([null, null])));
    });

    rig.server.finish();
}

#[test]
fn a_request_the_client_does_not_know_is_answered_with_an_error_not_ignored() {
    let rig = connect(|server| {
        server.request(9, "custom/unknown", Value::Null);
        let (id, outcome) = server.expect_response();
        assert_eq!(id, Id::Number(9));
        assert_eq!(outcome.unwrap_err().code, -32601);
    });

    rig.server.finish();
}

#[test]
fn a_request_made_while_the_server_asks_for_something_still_gets_its_answer() {
    let rig = connect(|server| {
        let (id, _) = server.expect_request("mine");
        server.request(5, "client/registerCapability", Value::Null);
        server.respond(id, json!("answered"));
        let (asked, outcome) = server.expect_response();
        assert_eq!((asked, outcome), (Id::Number(5), Ok(Value::Null)));
    });

    let answer = rig
        .connection
        .request("mine", Value::Null)
        .unwrap()
        .wait(PATIENCE)
        .unwrap();

    assert_eq!(answer, json!("answered"));
    rig.server.finish();
}

#[test]
fn the_server_hanging_up_fails_the_requests_waiting_and_says_so() {
    let rig = connect(|server| {
        server.expect_request("never answered");
    });

    let pending = rig
        .connection
        .request("never answered", Value::Null)
        .unwrap();
    let error = pending.wait(PATIENCE).unwrap_err();

    assert!(matches!(error, LspError::Closed(_)), "{error}");
    assert!(matches!(rig.next_incoming(), Incoming::Closed(_)));
    rig.server.finish();
}

#[test]
fn after_the_end_every_request_and_notification_fails_at_once() {
    let rig = connect(|_| {});
    assert!(matches!(rig.next_incoming(), Incoming::Closed(_)));

    let request = rig.connection.request("late", Value::Null);
    let notify = rig.connection.notify("late", Value::Null);

    assert!(matches!(request, Err(LspError::Closed(_))));
    assert!(matches!(notify, Err(LspError::Closed(_))));
    rig.server.finish();
}

#[test]
fn closed_is_the_last_thing_sent_and_says_why_the_stream_ended() {
    let rig = connect(|server| {
        server.send_raw(b"Content-Length: 99\r\n\r\n{");
    });

    let Incoming::Closed(reason) = rig.next_incoming() else {
        panic!("expected the end");
    };

    assert!(reason.contains("middle of a message"), "{reason}");
    assert!(
        rig.incoming
            .recv_timeout(Duration::from_millis(100))
            .is_err()
    );
    rig.server.finish();
}

#[test]
fn a_message_that_is_not_one_is_skipped_and_the_session_goes_on() {
    let rig = connect(|server| {
        server.send_raw(b"Content-Length: 7\r\n\r\n[1,2,3]");
        server.send_raw(b"Content-Length: 22\r\n\r\n{\"id\":null,\"result\":1}");
        server.notify("still alive", Value::Null);
    });

    assert!(matches!(
        rig.next_incoming(),
        Incoming::Notification { ref method, .. } if method == "still alive"
    ));
    rig.server.finish();
}

#[test]
fn a_broken_header_ends_the_session_with_the_reason() {
    let rig = connect(|server| {
        server.send_raw(b"Content-Length: lots\r\n\r\n");
    });

    let Incoming::Closed(reason) = rig.next_incoming() else {
        panic!("expected the end");
    };

    assert!(reason.contains("lots"), "{reason}");
    rig.server.finish();
}
