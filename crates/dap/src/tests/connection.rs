use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::test_support::{PATIENCE, fake_adapter, pipe, stopped};
use crate::{Connection, DapError, Event, Update};

#[test]
fn an_answer_is_matched_to_its_request_whatever_the_order() {
    let (client, fake) = fake_adapter(|adapter| {
        let (first, _) = adapter.expect_request("threads");
        let (second, _) = adapter.expect_request("threads");
        // Answered the other way round.
        adapter.respond(
            second,
            "threads",
            json!({ "threads": [{ "id": 2, "name": "two" }] }),
        );
        adapter.respond(
            first,
            "threads",
            json!({ "threads": [{ "id": 1, "name": "one" }] }),
        );
    });

    let first = client.threads().unwrap();
    let second = client.threads().unwrap();

    assert_eq!(second.wait(PATIENCE).unwrap().threads[0].name, "two");
    assert_eq!(first.wait(PATIENCE).unwrap().threads[0].name, "one");
    fake.finish();
}

#[test]
fn a_refusal_is_an_error_that_says_what_the_adapter_said() {
    let (client, fake) = fake_adapter(|adapter| {
        let (seq, _) = adapter.expect_request("next");
        adapter.refuse(
            seq,
            "next",
            "notStopped",
            json!({ "error": { "id": 1, "format": "the program is running" } }),
        );
    });

    let error = client.next(1).unwrap().wait(PATIENCE).unwrap_err();

    assert!(
        matches!(&error, DapError::Rejected { command, .. } if command == "next"),
        "{error}"
    );
    assert_eq!(
        error.to_string(),
        "the debug adapter refused next: the program is running"
    );
    fake.finish();
}

#[test]
fn an_answer_that_is_not_what_the_request_asks_for_is_an_error_not_a_panic() {
    let (client, fake) = fake_adapter(|adapter| {
        let (seq, _) = adapter.expect_request("stackTrace");
        adapter.respond(
            seq,
            "stackTrace",
            json!({ "stackFrames": [{ "id": "not a number" }] }),
        );
    });

    let error = client
        .stack_trace(&crate::StackTraceArguments::default())
        .unwrap()
        .wait(PATIENCE)
        .unwrap_err();

    assert!(
        matches!(&error, DapError::Unexpected { command, .. } if command == "stackTrace"),
        "{error}"
    );
    fake.finish();
}

#[test]
fn an_answer_with_no_body_is_fine_for_a_request_that_wants_none() {
    let (client, fake) = fake_adapter(|adapter| {
        let (seq, _) = adapter.expect_request("pause");
        adapter.respond(seq, "pause", Value::Null);
    });

    assert!(client.pause(1).unwrap().wait(PATIENCE).is_ok());
    fake.finish();
}

#[test]
fn a_request_that_is_not_answered_in_time_times_out_and_the_late_answer_is_dropped() {
    let (client, fake) = fake_adapter(|adapter| {
        let (seq, _) = adapter.expect_request("threads");
        // Too late for the first, in time for the second.
        std::thread::sleep(Duration::from_millis(300));
        adapter.respond(seq, "threads", json!({ "threads": [] }));
        let (seq, _) = adapter.expect_request("threads");
        adapter.respond(seq, "threads", json!({ "threads": [{ "id": 7 }] }));
    });

    let error = client
        .threads()
        .unwrap()
        .wait(Duration::from_millis(50))
        .unwrap_err();
    assert!(matches!(error, DapError::Timeout { .. }), "{error}");
    std::thread::sleep(Duration::from_millis(400));

    let answer = client.threads().unwrap().wait(PATIENCE).unwrap();
    assert_eq!(answer.threads[0].id, 7, "not the answer that was late");
    fake.finish();
}

#[test]
fn an_answer_can_be_looked_for_without_waiting() {
    let (client, fake) = fake_adapter(|adapter| {
        let (seq, _) = adapter.expect_request("threads");
        std::thread::sleep(Duration::from_millis(100));
        adapter.respond(seq, "threads", json!({ "threads": [] }));
    });

    let mut reply = client.threads().unwrap();

    assert!(reply.try_take().is_none(), "it has not come");
    let until = Instant::now() + PATIENCE;
    let answer = loop {
        if let Some(answer) = reply.try_take() {
            break answer;
        }
        assert!(Instant::now() < until, "it never came");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(answer.unwrap().threads.is_empty());
    fake.finish();
}

#[test]
fn events_arrive_in_order_and_the_client_is_woken() {
    let (client, fake) = fake_adapter(|adapter| {
        adapter.event("initialized", Value::Null);
        adapter.event("stopped", stopped("breakpoint", 1));
        adapter.event("output", json!({ "category": "stdout", "output": "hi\n" }));
    });

    let updates: Vec<Update> = (0..3)
        .map(|_| client.wait_update(PATIENCE).expect("an update"))
        .collect();

    assert!(matches!(updates[0], Update::Event(Event::Initialized)));
    assert!(matches!(&updates[1], Update::Event(Event::Stopped(s)) if s.reason == "breakpoint"));
    assert!(matches!(&updates[2], Update::Event(Event::Output(o)) if o.output == "hi\n"));
    assert!(fake.wakes() >= 3);
    fake.finish();
}

#[test]
fn nothing_waiting_is_none_and_waiting_for_nothing_ends_at_the_timeout() {
    let (client, fake) = fake_adapter(|adapter| {
        // Stays until the client hangs up.
        while adapter.next().is_some() {}
    });

    assert!(client.next_update().is_none());
    let before = Instant::now();
    assert!(client.wait_update(Duration::from_millis(80)).is_none());
    assert!(before.elapsed() >= Duration::from_millis(70));
    drop(client);
    fake.finish();
}

#[test]
fn an_adapter_that_hangs_up_fails_the_requests_waiting_and_every_later_one() {
    let (client, fake) = fake_adapter(|adapter| {
        adapter.expect_request("threads");
        // Leaves without answering.
    });

    let waiting = client.threads().unwrap();
    let error = waiting.wait(PATIENCE).unwrap_err();
    assert!(matches!(error, DapError::Closed(_)), "{error}");
    fake.finish();

    let later = client
        .threads()
        .err()
        .expect("a request after the end fails at once");
    assert!(matches!(later, DapError::Closed(_)), "{later}");
    assert!(matches!(
        client.wait_update(PATIENCE),
        Some(Update::Closed(_))
    ));
}

#[test]
fn a_message_that_is_not_one_is_skipped_and_the_session_goes_on() {
    let (client, fake) = fake_adapter(|adapter| {
        // Valid JSON that is not a message, then one that is.
        let body = br#"{"hello":"world"}"#;
        let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        framed.extend_from_slice(body);
        adapter.send_raw(&framed);
        adapter.event("initialized", Value::Null);
    });

    let update = client.wait_update(PATIENCE);

    assert!(
        matches!(update, Some(Update::Event(Event::Initialized))),
        "{update:?}"
    );
    fake.finish();
}

#[test]
fn a_stream_that_breaks_closes_the_session_and_says_why() {
    let (client, fake) = fake_adapter(|adapter| {
        adapter.send_raw(b"Content-Length: many\r\n\r\n{}");
        // Stays, so the end is not the hang up but the broken stream.
        while adapter.next().is_some() {}
    });

    let update = client.wait_update(PATIENCE);

    let Some(Update::Closed(reason)) = update else {
        panic!("{update:?}");
    };
    assert!(reason.contains("many"), "{reason}");
    drop(client);
    fake.finish();
}

#[test]
fn dropping_the_client_hangs_up_on_the_adapter() {
    let (client, fake) = fake_adapter(|adapter| {
        assert!(adapter.next().is_none(), "the client went away");
    });

    drop(client);

    fake.finish();
}

#[test]
fn the_numbers_of_the_requests_count_from_one() {
    let (client, fake) = fake_adapter(|adapter| {
        let (first, _) = adapter.expect_request("threads");
        let (second, _) = adapter.expect_request("pause");
        assert_eq!((first, second), (1, 2));
        adapter.respond(first, "threads", json!({}));
        adapter.respond(second, "pause", json!({}));
    });

    let a = client.threads().unwrap();
    let b = client.pause(1).unwrap();

    a.wait(PATIENCE).unwrap();
    b.wait(PATIENCE).unwrap();
    fake.finish();
}

/// A stream to an adapter that never reads: a write waits until the test lets go of `release`.
struct StuckWriter(mpsc::Receiver<()>);

impl std::io::Write for StuckWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let _ = self.0.recv();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn an_adapter_that_never_reads_does_not_block_the_caller() {
    let (release, stuck) = mpsc::channel();
    let (reader, _adapter_writer) = pipe();
    let (connection, _incoming) = Connection::start(reader, StuckWriter(stuck), || {});
    let started = Instant::now();

    let big = json!({ "text": "x".repeat(200_000) });
    let failure = (0..10_000)
        .map(|_| connection.request::<Value>("evaluate", big.clone()))
        .find_map(Result::err);

    assert!(started.elapsed() < PATIENCE);
    assert!(matches!(failure, Some(DapError::Closed(_))));
    drop(release);
}
