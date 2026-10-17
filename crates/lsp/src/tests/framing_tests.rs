use serde_json::json;

use crate::framing::{FrameError, MAX_BODY, read_message, write_message};

use crate::test_support::reader;

fn framed(body: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

#[test]
fn a_message_is_written_with_its_length_in_bytes() {
    let mut out = Vec::new();

    write_message(&mut out, &json!({ "text": "é€😀" })).unwrap();

    let text = String::from_utf8(out).unwrap();
    let (header, body) = text.split_once("\r\n\r\n").unwrap();
    assert_eq!(header, format!("Content-Length: {}", body.len()));
    assert_eq!(body, r#"{"text":"é€😀"}"#);
    assert!(body.len() > body.chars().count(), "the length counts bytes");
}

#[test]
fn what_is_written_reads_back() {
    let message = json!({ "jsonrpc": "2.0", "id": 1, "method": "m", "params": { "a": [1, 2] } });
    let mut out = Vec::new();
    write_message(&mut out, &message).unwrap();

    assert_eq!(read_message(&mut reader(&out)).unwrap(), Some(message));
}

#[test]
fn messages_one_after_the_other_are_read_one_by_one() {
    let mut bytes = framed(r#"{"n":1}"#);
    bytes.extend(framed(r#"{"n":2}"#));
    let mut stream = reader(&bytes);

    assert_eq!(read_message(&mut stream).unwrap(), Some(json!({ "n": 1 })));
    assert_eq!(read_message(&mut stream).unwrap(), Some(json!({ "n": 2 })));
    assert_eq!(read_message(&mut stream).unwrap(), None);
}

#[test]
fn an_empty_stream_is_a_clean_end() {
    assert_eq!(read_message(&mut reader(b"")).unwrap(), None);
}

#[test]
fn the_header_name_is_not_case_sensitive_and_other_headers_are_ignored() {
    let body = r#"{"ok":true}"#;
    let bytes = format!(
        "content-type: application/vscode-jsonrpc; charset=utf-8\r\nCONTENT-LENGTH:   {}  \r\n\r\n{body}",
        body.len()
    );

    let read = read_message(&mut reader(bytes.as_bytes())).unwrap();

    assert_eq!(read, Some(json!({ "ok": true })));
}

#[test]
fn multibyte_text_in_a_body_is_read_by_bytes() {
    let bytes = framed(r#"{"text":"日本語 😀"}"#);

    let read = read_message(&mut reader(&bytes)).unwrap();

    assert_eq!(read, Some(json!({ "text": "日本語 😀" })));
}

#[test]
fn a_message_without_a_length_is_an_error() {
    let error = read_message(&mut reader(b"Content-Type: x\r\n\r\n{}")).unwrap_err();

    assert!(matches!(error, FrameError::NoLength), "{error}");
}

#[test]
fn a_length_that_is_not_a_number_is_an_error() {
    let error = read_message(&mut reader(b"Content-Length: many\r\n\r\n{}")).unwrap_err();

    assert!(matches!(error, FrameError::Header(_)), "{error}");
    assert!(error.to_string().contains("many"), "{error}");
}

#[test]
fn a_header_line_with_no_colon_is_an_error() {
    let error = read_message(&mut reader(b"Content-Length 2\r\n\r\n{}")).unwrap_err();

    assert!(matches!(error, FrameError::Header(_)), "{error}");
}

#[test]
fn a_body_that_stops_short_is_truncated() {
    let error = read_message(&mut reader(b"Content-Length: 50\r\n\r\n{\"a\":1}")).unwrap_err();

    assert!(matches!(error, FrameError::Truncated), "{error}");
}

#[test]
fn headers_that_stop_short_are_truncated() {
    for bytes in [&b"Content-Length: 2\r\n"[..], b"Content-Len"] {
        let error = read_message(&mut reader(bytes)).unwrap_err();

        assert!(matches!(error, FrameError::Truncated), "{bytes:?}: {error}");
    }
}

#[test]
fn a_length_past_the_limit_is_refused_without_reading_the_body() {
    let header = format!("Content-Length: {}\r\n\r\n", MAX_BODY + 1);

    let error = read_message(&mut reader(header.as_bytes())).unwrap_err();

    assert!(matches!(error, FrameError::TooLarge(_)), "{error}");
}

#[test]
fn a_length_that_lies_about_a_huge_body_costs_nothing() {
    let header = format!("Content-Length: {MAX_BODY}\r\n\r\n{{}}");

    let error = read_message(&mut reader(header.as_bytes())).unwrap_err();

    assert!(matches!(error, FrameError::Truncated), "{error}");
}

#[test]
fn a_body_that_is_not_json_is_an_error() {
    let error = read_message(&mut reader(&framed("not json"))).unwrap_err();

    assert!(matches!(error, FrameError::Json(_)), "{error}");
}

#[test]
fn a_header_line_that_never_ends_is_an_error() {
    let mut bytes = b"Content-Length: ".to_vec();
    bytes.extend(vec![b'9'; 20_000]);

    let error = read_message(&mut reader(&bytes)).unwrap_err();

    assert!(matches!(error, FrameError::Header(_)), "{error}");
}
