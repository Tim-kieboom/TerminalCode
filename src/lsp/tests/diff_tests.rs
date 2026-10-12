use ropey::Rope;

use crate::lsp::diff::{Change, change_between};
use crate::lsp::position::{LspPosition, from_lsp};

fn rope(text: &str) -> Rope {
    Rope::from_str(text)
}

fn change(old: &str, new: &str) -> Option<Change> {
    change_between(&rope(old), &rope(new))
}

fn position(line: u32, character: u32) -> LspPosition {
    LspPosition { line, character }
}

/// Applies `change` to `old` the way a server does.
fn apply(old: &str, change: &Change) -> String {
    let rope = rope(old);
    let (start, end) = (from_lsp(&rope, change.start), from_lsp(&rope, change.end));
    format!("{}{}{}", &old[..start], change.text, &old[end..])
}

#[track_caller]
fn assert_turns_into(old: &str, new: &str) {
    match change(old, new) {
        None => assert_eq!(old, new),
        Some(change) => assert_eq!(apply(old, &change), new, "{old:?} -> {new:?}: {change:?}"),
    }
}

#[test]
fn the_same_text_is_no_change() {
    assert_eq!(change("", ""), None);
    assert_eq!(change("abc", "abc"), None);
    assert_eq!(change("a\nb\n", "a\nb\n"), None);
}

#[test]
fn typing_a_character_is_an_insertion_where_it_was_typed() {
    let change = change("fn main() {}", "fn mxain() {}").unwrap();

    assert_eq!(change.start, position(0, 4));
    assert_eq!(change.end, position(0, 4));
    assert_eq!(change.text, "x");
}

#[test]
fn deleting_a_character_replaces_it_with_nothing() {
    let change = change("hello", "hllo").unwrap();

    assert_eq!((change.start, change.end), (position(0, 1), position(0, 2)));
    assert_eq!(change.text, "");
}

#[test]
fn replacing_text_names_the_old_range_and_the_new_text() {
    let change = change("let value = 1;", "let total = 1;").unwrap();

    assert_eq!((change.start, change.end), (position(0, 4), position(0, 9)));
    assert_eq!(change.text, "total");
}

#[test]
fn the_range_is_in_the_old_text_and_across_lines() {
    let change = change("one\ntwo\nthree\n", "one\nTWO\nTHREE\n").unwrap();

    assert_eq!((change.start, change.end), (position(1, 0), position(2, 5)));
    assert_eq!(change.text, "TWO\nTHREE");
}

#[test]
fn an_insertion_at_the_start_and_an_append_at_the_end() {
    let first = change("bc", "abc").unwrap();
    assert_eq!(
        (first.start, first.end, first.text.as_str()),
        (position(0, 0), position(0, 0), "a")
    );

    let last = change("ab", "abc").unwrap();
    assert_eq!(
        (last.start, last.end, last.text.as_str()),
        (position(0, 2), position(0, 2), "c")
    );
}

#[test]
fn from_nothing_to_text_and_back() {
    let all = change("", "hello\n").unwrap();
    assert_eq!(
        (all.start, all.end, all.text.as_str()),
        (position(0, 0), position(0, 0), "hello\n")
    );

    let none = change("hello\n", "").unwrap();
    assert_eq!((none.start, none.end), (position(0, 0), position(1, 0)));
    assert_eq!(none.text, "");
}

#[test]
fn repeated_text_does_not_confuse_the_start_and_the_end() {
    for (old, new) in [
        ("aaa", "aaaa"),
        ("aaaa", "aaa"),
        ("abab", "ababab"),
        ("a", "aa"),
        ("xyyx", "xyyyx"),
    ] {
        assert_turns_into(old, new);
    }
}

#[test]
fn a_character_that_shares_its_first_bytes_with_another_is_replaced_whole() {
    // é is C3 A9 and è is C3 A8: the bytes start the same, the characters do not.
    let change = change("café", "cafè").unwrap();

    assert_eq!((change.start, change.end), (position(0, 3), position(0, 4)));
    assert_eq!(change.text, "è");
    assert_turns_into("日本", "日木");
    assert_turns_into("a😀b", "a😁b");
}

#[test]
fn text_that_ends_with_the_same_bytes_as_another_is_replaced_whole() {
    assert_turns_into("éa", "èa");
    assert_turns_into("x😀", "x😁");
    assert_turns_into("日", "月");
}

#[test]
fn a_crlf_is_never_cut_in_two() {
    // Inserting a line feed after a carriage return makes it a line break.
    for (old, new) in [
        ("a\r\nb", "a\r\n\r\nb"),
        ("a\rb", "a\r\nb"),
        ("a\r\nb", "a\rb"),
        ("a\r\n", "a\r\n\r\n"),
        ("\r\n", ""),
    ] {
        let change = change(old, new).unwrap();
        let rope = rope(old);
        let (start, end) = (from_lsp(&rope, change.start), from_lsp(&rope, change.end));

        assert!(
            !(start > 0 && start < old.len() && &old.as_bytes()[start - 1..=start] == b"\r\n"),
            "{old:?} -> {new:?} starts inside a CRLF: {change:?}"
        );
        assert!(
            !(end > 0 && end < old.len() && &old.as_bytes()[end - 1..=end] == b"\r\n"),
            "{old:?} -> {new:?} ends inside a CRLF: {change:?}"
        );
        assert_turns_into(old, new);
    }
}

#[test]
fn a_change_in_a_text_with_wide_characters_counts_utf_16_units() {
    // The emoji is two units, so the edit after it is at unit 3.
    let change = change("a😀b", "a😀xb").unwrap();

    assert_eq!((change.start, change.end), (position(0, 3), position(0, 3)));
    assert_eq!(change.text, "x");
}

#[test]
fn a_big_text_with_a_change_in_the_middle_finds_it() {
    let old = "some line of text that goes on\n".repeat(50_000);
    let mut new = old.clone();
    let middle = new.len() / 2 - new.len() / 2 % 31;
    new.insert_str(middle, "INSERTED");

    let change = change(&old, &new).unwrap();

    assert_eq!(change.text, "INSERTED");
    assert_eq!(change.start, change.end);
    assert_eq!(apply(&old, &change), new);
}

#[test]
fn any_two_texts_are_turned_into_each_other_by_their_change() {
    // Small texts from a few characters that are awkward in UTF-8 and in lines.
    let alphabet = ['a', 'b', '\n', '\r', 'é', '😀', '日', '\u{301}', ' '];
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = |bound: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % bound as u64) as usize
    };
    let random_text = |len: usize, next: &mut dyn FnMut(usize) -> usize| -> String {
        (0..len).map(|_| alphabet[next(alphabet.len())]).collect()
    };

    for _ in 0..3000 {
        let old = random_text(next(14), &mut next);
        // A new text that is mostly the old one, edited, or a different one.
        let new = if next(4) == 0 {
            random_text(next(14), &mut next)
        } else {
            let chars: Vec<char> = old.chars().collect();
            let from = next(chars.len() + 1);
            let to = (from + next(4)).min(chars.len());
            let mut new: String = chars[..from].iter().collect();
            new.push_str(&random_text(next(4), &mut next));
            new.extend(&chars[to..]);
            new
        };
        assert_turns_into(&old, &new);
    }
}
