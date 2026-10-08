use crate::clipboard::ClipboardError;
use crate::clipboard::system::write_osc52;

#[test]
fn osc52_writes_base64_between_the_escape_codes() {
    let mut out = Vec::new();

    write_osc52(&mut out, "hello").unwrap();

    assert_eq!(out, b"\x1b]52;c;aGVsbG8=\x07");
}

#[test]
fn osc52_encodes_multiline_and_unicode_text() {
    let mut out = Vec::new();

    write_osc52(&mut out, "a\nb \u{e9}").unwrap();

    let sequence = String::from_utf8(out).unwrap();
    assert!(sequence.starts_with("\x1b]52;c;"));
    assert!(sequence.ends_with('\x07'));
    assert!(!sequence.contains('\n'));
}

#[test]
fn osc52_refuses_text_the_terminal_would_truncate() {
    let mut out = Vec::new();
    let huge = "x".repeat(1_000_001);

    let result = write_osc52(&mut out, &huge);

    assert!(matches!(result, Err(ClipboardError::TooLarge(1_000_001))));
    assert!(out.is_empty());
}
