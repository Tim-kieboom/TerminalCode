//! Starting an adapter that is a program.

use crate::test_support::PATIENCE;
use crate::{Client, DapError, Event, Update};

#[test]
fn a_program_that_is_not_there_is_an_error_naming_it() {
    let error = Client::spawn("trcode-no-such-adapter", &[], || {})
        .err()
        .unwrap();

    assert!(
        matches!(&error, DapError::Spawn { program, .. } if program == "trcode-no-such-adapter"),
        "{error}"
    );
    assert!(
        error.to_string().contains("trcode-no-such-adapter"),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
fn an_adapter_that_exits_at_once_closes_the_session() {
    let client = Client::spawn("sh", &["-c".to_owned(), "exit 0".to_owned()], || {}).unwrap();

    let update = client.wait_update(PATIENCE);

    assert!(matches!(update, Some(Update::Closed(_))), "{update:?}");
    assert!(
        client.threads().is_err(),
        "a request after the end fails at once"
    );
}

#[cfg(unix)]
#[test]
fn what_a_program_writes_to_its_output_is_read_as_the_adapters_messages() {
    let body = r#"{"seq":1,"type":"event","event":"initialized"}"#;
    let script = format!(
        "printf 'Content-Length: {}\\r\\n\\r\\n{}'; cat > /dev/null",
        body.len(),
        body
    );
    let client = Client::spawn("sh", &["-c".to_owned(), script], || {}).unwrap();

    let update = client.wait_update(PATIENCE);

    assert!(
        matches!(update, Some(Update::Event(Event::Initialized))),
        "{update:?}"
    );
    // Dropping the client ends the program, which would otherwise wait for its input for good.
    drop(client);
}
