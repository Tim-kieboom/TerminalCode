use crate::buffer::Buffer;
use crate::buffer::line_ending::LineEnding;

#[test]
fn text_without_line_breaks_is_lf() {
    assert_eq!(LineEnding::detect("abc"), LineEnding::Lf);
}

#[test]
fn lf_file_is_detected() {
    assert_eq!(LineEnding::detect("a\nb\r\nc"), LineEnding::Lf);
}

#[test]
fn crlf_file_is_detected() {
    assert_eq!(LineEnding::detect("a\r\nb\nc"), LineEnding::Crlf);
}

#[test]
fn lone_carriage_return_is_not_a_line_break() {
    assert_eq!(LineEnding::detect("a\rb"), LineEnding::Lf);
}

#[test]
fn line_ending_strings_match_the_variant() {
    assert_eq!(LineEnding::Lf.as_str(), "\n");
    assert_eq!(LineEnding::Crlf.as_str(), "\r\n");
}

#[test]
fn buffer_remembers_the_detected_line_ending() {
    assert_eq!(Buffer::from_text("a\r\nb").line_ending(), LineEnding::Crlf);
}
