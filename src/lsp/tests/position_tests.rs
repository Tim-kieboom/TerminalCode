use ropey::Rope;

use crate::lsp::position::{LspPosition, from_lsp, to_lsp};

fn position(line: u32, character: u32) -> LspPosition {
    LspPosition { line, character }
}

fn rope(text: &str) -> Rope {
    Rope::from_str(text)
}

/// The byte of the first `needle` in `text`.
fn at(text: &str, needle: &str) -> usize {
    text.find(needle).unwrap()
}

#[test]
fn ascii_positions_are_line_and_column() {
    let text = "fn main() {\n    let x = 1;\n}\n";

    assert_eq!(to_lsp(&rope(text), 0), position(0, 0));
    assert_eq!(to_lsp(&rope(text), at(text, "main")), position(0, 3));
    assert_eq!(to_lsp(&rope(text), at(text, "let")), position(1, 4));
    assert_eq!(to_lsp(&rope(text), at(text, "}")), position(2, 0));
}

#[test]
fn the_end_of_the_text_is_the_start_of_the_empty_last_line() {
    let text = "a\nb\n";

    assert_eq!(to_lsp(&rope(text), text.len()), position(2, 0));
    assert_eq!(to_lsp(&rope("no newline"), 10), position(0, 10));
    assert_eq!(to_lsp(&rope(""), 0), position(0, 0));
}

#[test]
fn a_byte_past_the_end_is_the_end() {
    assert_eq!(to_lsp(&rope("abc"), 99), position(0, 3));
}

#[test]
fn a_tab_is_one_character_not_its_drawn_width() {
    let text = "\tx";

    assert_eq!(to_lsp(&rope(text), at(text, "x")), position(0, 1));
}

#[test]
fn a_character_outside_the_basic_plane_counts_two_units() {
    // U+1F600 is two UTF-16 code units and four bytes.
    let text = "a😀b";

    assert_eq!(to_lsp(&rope(text), at(text, "b")), position(0, 3));
    assert_eq!(to_lsp(&rope(text), at(text, "😀")), position(0, 1));
}

#[test]
fn a_character_in_the_basic_plane_counts_one_unit_however_many_bytes() {
    let text = "é日本語x";

    assert_eq!(to_lsp(&rope(text), at(text, "x")), position(0, 4));
}

#[test]
fn a_combining_accent_is_a_character_of_its_own() {
    // "e" then U+0301: two characters, two units, one grapheme for the editor.
    let text = "e\u{301}x";

    assert_eq!(to_lsp(&rope(text), at(text, "x")), position(0, 2));
}

#[test]
fn a_crlf_line_break_ends_the_line_and_the_next_line_starts_at_zero() {
    let text = "ab\r\ncd\r\n";

    assert_eq!(to_lsp(&rope(text), at(text, "cd")), position(1, 0));
    assert_eq!(to_lsp(&rope(text), at(text, "d")), position(1, 1));
    assert_eq!(to_lsp(&rope(text), text.len()), position(2, 0));
}

#[test]
fn a_byte_in_the_middle_of_a_character_is_its_start() {
    let text = "a😀b";

    assert_eq!(to_lsp(&rope(text), 2), position(0, 1));
}

#[test]
fn a_position_gives_its_byte_back() {
    let text = "a😀b\né日\r\n\tx";

    assert_eq!(from_lsp(&rope(text), position(0, 3)), at(text, "b"));
    assert_eq!(from_lsp(&rope(text), position(1, 1)), at(text, "日"));
    assert_eq!(from_lsp(&rope(text), position(2, 1)), at(text, "x"));
    assert_eq!(from_lsp(&rope(text), position(0, 0)), 0);
}

#[test]
fn every_character_boundary_survives_the_round_trip() {
    for text in [
        "",
        "plain\nlines\n",
        "a😀b\né日\r\n\tx\n",
        "e\u{301}\u{301}x\r\n\r\n",
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\n",
        "no end of line",
        "\n\n\n",
    ] {
        let rope = rope(text);
        for byte in (0..=text.len()).filter(|byte| text.is_char_boundary(*byte)) {
            // A byte between \r and \n is not a position the protocol has.
            if byte > 0 && byte < text.len() && &text.as_bytes()[byte - 1..=byte] == b"\r\n" {
                continue;
            }
            assert_eq!(
                from_lsp(&rope, to_lsp(&rope, byte)),
                byte,
                "{text:?} at {byte}"
            );
        }
    }
}

#[test]
fn a_line_past_the_end_is_the_end_of_the_text() {
    let text = "ab\ncd";

    assert_eq!(from_lsp(&rope(text), position(9, 0)), text.len());
    assert_eq!(
        from_lsp(&rope(text), position(2, 0)),
        text.len().min(text.len())
    );
}

#[test]
fn a_character_past_the_end_of_its_line_is_the_end_of_the_line_before_its_break() {
    let text = "ab\ncd\r\nef";

    assert_eq!(from_lsp(&rope(text), position(0, 50)), at(text, "\n"));
    assert_eq!(from_lsp(&rope(text), position(1, 50)), at(text, "\r"));
    assert_eq!(from_lsp(&rope(text), position(2, 50)), text.len());
}

#[test]
fn a_character_in_the_middle_of_a_surrogate_pair_is_the_start_of_that_character() {
    let text = "a😀b";

    // Unit 2 is the second half of the emoji.
    assert_eq!(from_lsp(&rope(text), position(0, 2)), at(text, "😀"));
}

#[test]
fn an_empty_text_has_one_empty_line() {
    assert_eq!(from_lsp(&rope(""), position(0, 0)), 0);
    assert_eq!(from_lsp(&rope(""), position(0, 7)), 0);
    assert_eq!(from_lsp(&rope(""), position(3, 0)), 0);
}

#[test]
fn an_enormous_position_does_not_wrap() {
    assert_eq!(from_lsp(&rope("abc"), position(u32::MAX, u32::MAX)), 3);
    assert_eq!(from_lsp(&rope("abc"), position(0, u32::MAX)), 3);
}
