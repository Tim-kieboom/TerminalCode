use ropey::Rope;
use serde_json::{Value, json};

use crate::buffer::Buffer;
use crate::components::editor::{Editor, Motion};
use crate::lsp::completion::parse_completion;
use crate::lsp::{Accepted, ByteChange, ByteEdit, Completion, CompletionList, Refresh};

fn list(items: Value) -> CompletionList {
    parse_completion(&items).unwrap()
}

fn plain(labels: &[&str]) -> CompletionList {
    list(Value::Array(
        labels
            .iter()
            .map(|label| json!({ "label": label }))
            .collect(),
    ))
}

fn rope(text: &str) -> Rope {
    Rope::from_str(text)
}

/// A box for `list` over `text`, for the word that starts at `start` and ends at `cursor`.
fn open(list: CompletionList, text: &str, start: usize, cursor: usize) -> Completion {
    Completion::new(list, &rope(text), start, cursor)
}

fn labels(completion: &Completion) -> Vec<&str> {
    completion.items().map(|item| item.label.as_str()).collect()
}

fn typed(at: usize, len: usize) -> ByteChange {
    ByteChange {
        start: at,
        old_end: at,
        new_end: at + len,
    }
}

fn range(start: (u32, u32), end: (u32, u32)) -> Value {
    json!({
        "start": { "line": start.0, "character": start.1 },
        "end": { "line": end.0, "character": end.1 },
    })
}

// ---- narrowing and order

#[test]
fn only_what_starts_with_the_typed_word_is_shown_whatever_its_case() {
    let completion = open(
        plain(&["push", "pop", "Pull", "unshift", "shrink_to_fit"]),
        "p",
        0,
        1,
    );

    assert_eq!(labels(&completion), ["Pull", "pop", "push"]);
}

#[test]
fn letters_in_order_somewhere_inside_do_not_match() {
    let completion = open(plain(&["push", "shrink_to_fit"]), "psh", 0, 3);

    assert!(completion.is_empty());
    assert_eq!(completion.len(), 0);
    assert_eq!(completion.selected_item(), None);
}

#[test]
fn the_order_is_the_servers_sort_text_then_the_label() {
    let completion = open(
        list(json!([
            { "label": "b", "sortText": "2" },
            { "label": "z", "sortText": "1" },
            { "label": "a", "sortText": "2" },
            { "label": "m" },
        ])),
        "",
        0,
        0,
    );

    // `m` has no sort text, so its label is used: it sorts after the digits.
    assert_eq!(labels(&completion), ["z", "a", "b", "m"]);
}

#[test]
fn the_filter_text_is_what_is_compared_not_the_label() {
    let completion = open(
        list(json!([
            { "label": "Vec<T>", "filterText": "vec" },
            { "label": "vector", "filterText": "vector" },
        ])),
        "ve",
        0,
        2,
    );

    assert_eq!(labels(&completion), ["Vec<T>", "vector"]);
    let none = open(
        list(json!([{ "label": "Vec<T>", "filterText": "vec" }])),
        "<",
        0,
        1,
    );
    assert!(none.is_empty());
}

#[test]
fn right_after_a_trigger_nothing_is_typed_and_everything_is_shown() {
    let completion = open(plain(&["push", "pop"]), "v.", 2, 2);

    assert_eq!(completion.len(), 2);
}

// ---- the highlight

#[test]
fn the_first_item_is_highlighted_and_next_and_previous_go_round() {
    let mut completion = open(plain(&["a", "b", "c"]), "", 0, 0);
    assert_eq!(completion.selected_item().unwrap().label, "a");

    completion.select_next();
    completion.select_next();
    assert_eq!(completion.selected_item().unwrap().label, "c");
    completion.select_next();
    assert_eq!(completion.selected_item().unwrap().label, "a");
    completion.select_previous();
    assert_eq!(completion.selected_item().unwrap().label, "c");
    assert_eq!(completion.selected_index(), 2);
}

#[test]
fn moving_the_highlight_of_an_empty_box_does_nothing() {
    let mut completion = open(plain(&["a"]), "z", 0, 1);

    completion.select_next();
    completion.select_previous();

    assert_eq!(completion.selected_index(), 0);
    assert_eq!(completion.accept(1), None);
}

#[test]
fn typing_more_keeps_the_highlight_on_the_same_item_while_it_still_matches() {
    let text = "p";
    let mut completion = open(plain(&["pa", "pb", "pbx"]), text, 0, 1);
    completion.select_next();
    assert_eq!(completion.selected_item().unwrap().label, "pb");

    completion.apply_change(&typed(1, 1));
    let refresh = completion.retarget(&rope("pb"), 2);

    assert_eq!(refresh, Refresh::Open);
    assert_eq!(labels(&completion), ["pb", "pbx"]);
    assert_eq!(completion.selected_item().unwrap().label, "pb");
}

#[test]
fn when_the_highlighted_item_stops_matching_the_highlight_starts_again() {
    let mut completion = open(plain(&["pa", "pb"]), "p", 0, 1);
    completion.select_next();

    completion.apply_change(&typed(1, 1));
    completion.retarget(&rope("pa"), 2);

    assert_eq!(completion.selected_item().unwrap().label, "pa");
}

// ---- when the box closes

#[test]
fn a_cursor_before_the_start_of_the_word_closes_the_box() {
    let mut completion = open(plain(&["a"]), "foo.b", 4, 5);

    assert_eq!(completion.retarget(&rope("foo.b"), 3), Refresh::Closed);
}

#[test]
fn a_character_that_is_not_part_of_a_word_closes_the_box() {
    let mut completion = open(plain(&["a"]), "ab", 0, 2);

    completion.apply_change(&typed(2, 1));

    assert_eq!(completion.retarget(&rope("ab("), 3), Refresh::Closed);
}

#[test]
fn letters_digits_and_underscores_keep_it_open_and_deleting_back_to_the_start_too() {
    let mut completion = open(plain(&["a_1"]), "a", 0, 1);

    completion.apply_change(&typed(1, 2));
    assert_eq!(completion.retarget(&rope("a_1"), 3), Refresh::Open);
    let deleted = ByteChange {
        start: 0,
        old_end: 3,
        new_end: 0,
    };
    completion.apply_change(&deleted);
    assert_eq!(completion.retarget(&rope(""), 0), Refresh::Open);
    assert_eq!(completion.word_start(), 0);
}

#[test]
fn text_typed_before_the_word_moves_its_start() {
    let mut completion = open(plain(&["ab"]), "x a", 2, 3);

    completion.apply_change(&typed(0, 2));

    assert_eq!(completion.word_start(), 4);
    assert_eq!(completion.retarget(&rope("yyx a"), 5), Refresh::Open);
}

// ---- when the list has to be asked for again

#[test]
fn a_complete_list_is_filtered_as_the_word_grows_not_asked_for_again() {
    let mut completion = open(plain(&["vec", "vector"]), "v", 0, 1);

    completion.apply_change(&typed(1, 1));
    completion.retarget(&rope("ve"), 2);

    assert!(!completion.wants_new_list());
}

#[test]
fn an_incomplete_list_is_asked_for_again_when_the_word_grows_but_not_before() {
    let incomplete = list(json!({ "isIncomplete": true, "items": [{ "label": "vec" }] }));
    let mut completion = open(incomplete, "v", 0, 1);
    assert!(!completion.wants_new_list());

    completion.apply_change(&typed(1, 1));
    completion.retarget(&rope("ve"), 2);

    assert!(completion.wants_new_list());
}

#[test]
fn a_word_shorter_than_the_one_asked_for_always_needs_a_new_list() {
    let mut completion = open(plain(&["vec"]), "ve", 0, 2);

    let deleted = ByteChange {
        start: 1,
        old_end: 2,
        new_end: 1,
    };
    completion.apply_change(&deleted);
    completion.retarget(&rope("v"), 1);

    assert!(completion.wants_new_list());
}

#[test]
fn a_box_made_from_a_newer_list_can_keep_the_highlight_of_the_old_one() {
    let mut old = open(plain(&["ve", "vec", "vect"]), "v", 0, 1);
    old.select_next();
    old.apply_change(&typed(1, 1));
    old.retarget(&rope("ve"), 2);

    let mut newer = open(plain(&["vec", "vect", "vector"]), "ve", 0, 2);
    newer.keep_highlight_of(&old);

    assert_eq!(labels(&newer), ["vec", "vect", "vector"]);
    assert_eq!(newer.selected_item().unwrap().label, "vec");
    assert!(!newer.wants_new_list());
    assert_eq!(newer.typed(), "ve");
}

// ---- what picking writes

fn accepted_text(text: &str, accepted: &Accepted) -> String {
    let mut text = text.to_owned();
    for edit in accepted.edits.iter().rev() {
        text.replace_range(edit.range.clone(), &edit.text);
    }
    text
}

#[test]
fn picking_replaces_the_typed_word_with_the_label() {
    let completion = open(plain(&["push"]), "v.pu", 2, 4);

    let accepted = completion.accept(4).unwrap();

    assert_eq!(
        accepted.edits,
        [ByteEdit {
            range: 2..4,
            text: "push".to_owned()
        }]
    );
    assert_eq!(accepted.cursor, 6);
    assert_eq!(accepted_text("v.pu", &accepted), "v.push");
}

#[test]
fn the_text_edit_the_server_gave_decides_what_is_written() {
    let completion = open(
        list(json!([{
            "label": "push(…)",
            "insertText": "ignored",
            "textEdit": { "range": range((0, 2), (0, 4)), "newText": "push" },
        }])),
        "v.pu",
        2,
        4,
    );

    let accepted = completion.accept(4).unwrap();

    assert_eq!(accepted_text("v.pu", &accepted), "v.push");
}

#[test]
fn without_a_text_edit_the_insert_text_comes_before_the_label() {
    let completion = open(
        list(json!([{ "label": "push(…)", "insertText": "push" }])),
        "v.pu",
        2,
        4,
    );

    assert_eq!(
        accepted_text("v.pu", &completion.accept(4).unwrap()),
        "v.push"
    );
}

#[test]
fn a_snippet_is_written_as_its_plain_label() {
    let completion = open(
        list(json!([{
            "label": "push",
            "insertTextFormat": 2,
            "textEdit": { "range": range((0, 2), (0, 4)), "newText": "push(${1:value})" },
        }])),
        "v.pu",
        2,
        4,
    );

    assert_eq!(
        accepted_text("v.pu", &completion.accept(4).unwrap()),
        "v.push"
    );
}

#[test]
fn what_was_typed_after_the_list_was_made_is_replaced_too() {
    let mut completion = open(
        list(json!([{
            "label": "new",
            "textEdit": { "range": range((0, 5), (0, 7)), "newText": "new" },
        }])),
        "Vec::ne",
        5,
        7,
    );
    // The server saw `Vec::ne` and named columns 5..7; `w` is typed after.
    completion.apply_change(&typed(7, 1));
    completion.retarget(&rope("Vec::new"), 8);

    let accepted = completion.accept(8).unwrap();

    assert_eq!(accepted_text("Vec::new", &accepted), "Vec::new");
    assert_eq!(accepted.edits[0].range, 5..8);
}

#[test]
fn an_edit_the_server_placed_before_the_cursor_word_keeps_its_start() {
    // The server replaces from the `::` as well.
    let completion = open(
        list(json!([{
            "label": "new",
            "textEdit": { "range": range((0, 3), (0, 7)), "newText": "::new" },
        }])),
        "Vec::ne",
        5,
        7,
    );

    let accepted = completion.accept(7).unwrap();

    assert_eq!(accepted.edits[0].range, 3..7);
    assert_eq!(accepted_text("Vec::ne", &accepted), "Vec::new");
    assert_eq!(accepted.cursor, 8);
}

#[test]
fn positions_count_utf_16_units_not_bytes() {
    // The emoji is four bytes and two UTF-16 units; `ne` starts at unit 3.
    let text = "😀 ne";
    let completion = open(
        list(json!([{
            "label": "new",
            "textEdit": { "range": range((0, 3), (0, 5)), "newText": "new" },
        }])),
        text,
        5,
        7,
    );

    let accepted = completion.accept(7).unwrap();

    assert_eq!(accepted.edits[0].range, 5..7);
    assert_eq!(accepted_text(text, &accepted), "😀 new");
}

#[test]
fn an_edit_that_took_the_range_the_server_named_falls_back_to_the_word() {
    let mut completion = open(
        list(json!([{
            "label": "new",
            "textEdit": { "range": range((0, 5), (0, 7)), "newText": "new" },
        }])),
        "Vec::ne",
        5,
        7,
    );
    // The `e` is deleted: the edit reaches into the range the server named.
    completion.apply_change(&ByteChange {
        start: 6,
        old_end: 7,
        new_end: 6,
    });
    completion.retarget(&rope("Vec::n"), 6);

    let accepted = completion.accept(6).unwrap();

    assert_eq!(accepted.edits[0].range, 5..6);
}

// ---- the import that goes with an item

const IMPORT: &str = "use std::collections::HashMap;\n";

fn hash_map() -> CompletionList {
    list(json!([{
        "label": "HashMap",
        "kind": 22,
        "textEdit": { "range": range((2, 11), (2, 15)), "newText": "HashMap" },
        "additionalTextEdits": [
            { "range": range((0, 0), (0, 0)), "newText": IMPORT },
        ],
    }]))
}

const SOURCE: &str = "fn main() {\n\n    let m: Hash";

#[test]
fn picking_writes_the_word_and_the_import_as_edits_in_text_order() {
    let start = SOURCE.len() - 4;
    let completion = open(hash_map(), SOURCE, start, SOURCE.len());

    let accepted = completion.accept(SOURCE.len()).unwrap();

    assert_eq!(accepted.edits.len(), 2);
    assert_eq!(accepted.edits[0].range, 0..0);
    assert_eq!(accepted.edits[0].text, IMPORT);
    assert_eq!(accepted.edits[1].range, start..SOURCE.len());
    assert_eq!(
        accepted_text(SOURCE, &accepted),
        format!("{IMPORT}fn main() {{\n\n    let m: HashMap")
    );
    // The cursor is after the word, past the import added before it.
    assert_eq!(
        accepted.cursor,
        IMPORT.len() + SOURCE.len() - 4 + "HashMap".len()
    );
}

#[test]
fn the_import_stays_at_the_top_when_more_is_typed_after_the_list_was_made() {
    let start = SOURCE.len() - 4;
    let mut completion = open(hash_map(), SOURCE, start, SOURCE.len());
    let after = format!("{SOURCE}M");
    completion.apply_change(&typed(SOURCE.len(), 1));
    completion.retarget(&rope(&after), after.len());

    let accepted = completion.accept(after.len()).unwrap();

    assert_eq!(
        accepted_text(&after, &accepted),
        format!("{IMPORT}fn main() {{\n\n    let m: HashMap")
    );
}

#[test]
fn text_typed_at_the_very_start_goes_after_the_import_point_not_before_it() {
    let start = SOURCE.len() - 4;
    let mut completion = open(hash_map(), SOURCE, start, SOURCE.len());
    let after = format!("//\n{SOURCE}");
    completion.apply_change(&typed(0, 3));
    completion.retarget(&rope(&after), after.len());

    let accepted = completion.accept(after.len()).unwrap();

    // The point at the start stays before what was typed there.
    assert_eq!(accepted.edits[0].range, 0..0);
}

#[test]
fn an_import_that_an_edit_took_is_left_out_and_the_word_is_still_written() {
    let start = SOURCE.len() - 4;
    let mut completion = open(
        list(json!([{
            "label": "HashMap",
            "additionalTextEdits": [
                { "range": range((0, 0), (0, 6)), "newText": "use x;" },
            ],
        }])),
        SOURCE,
        start,
        SOURCE.len(),
    );
    // Part of the range of the import is deleted.
    completion.apply_change(&ByteChange {
        start: 2,
        old_end: 4,
        new_end: 2,
    });
    completion.retarget(&rope(&SOURCE[2..]), SOURCE.len() - 2);

    let accepted = completion.accept(SOURCE.len() - 2).unwrap();

    assert_eq!(accepted.edits.len(), 1);
}

#[test]
fn an_extra_edit_inside_the_word_is_not_applied() {
    let start = SOURCE.len() - 4;
    let completion = open(
        list(json!([{
            "label": "HashMap",
            "additionalTextEdits": [
                { "range": range((2, 12), (2, 14)), "newText": "XX" },
            ],
        }])),
        SOURCE,
        start,
        SOURCE.len(),
    );

    let accepted = completion.accept(SOURCE.len()).unwrap();

    assert_eq!(accepted.edits.len(), 1);
}

// ---- applied to an editor: one undo step

fn editor_at_end(text: &str) -> Editor {
    let mut editor = Editor::new(Buffer::from_text(text));
    editor.move_cursor(Motion::DocumentEnd).unwrap();
    editor
}

fn apply(editor: &mut Editor, accepted: &Accepted) {
    let edits: Vec<_> = accepted
        .edits
        .iter()
        .map(|edit| (edit.range.clone(), edit.text.as_str()))
        .collect();
    editor.replace_ranges(&edits, accepted.cursor).unwrap();
}

#[test]
fn the_word_and_the_import_go_in_together_and_one_undo_takes_both_out() {
    let mut editor = editor_at_end(SOURCE);
    let start = SOURCE.len() - 4;
    let completion = open(hash_map(), SOURCE, start, SOURCE.len());
    let accepted = completion.accept(SOURCE.len()).unwrap();

    apply(&mut editor, &accepted);

    let expected = format!("{IMPORT}fn main() {{\n\n    let m: HashMap");
    assert_eq!(editor.buffer().text(), expected);
    let cursor = editor.selections().primary().head();
    assert_eq!(
        editor.buffer().position_to_byte(cursor).unwrap(),
        expected.len()
    );

    editor.undo().unwrap();

    assert_eq!(editor.buffer().text(), SOURCE);
    editor.redo().unwrap();
    assert_eq!(editor.buffer().text(), expected);
}

#[test]
fn picking_after_more_typing_leaves_one_undo_step_too() {
    let mut editor = editor_at_end(SOURCE);
    let start = SOURCE.len() - 4;
    let mut completion = open(hash_map(), SOURCE, start, SOURCE.len());
    editor.insert_text("M").unwrap();
    let now = editor.buffer().snapshot();
    completion.apply_change(&typed(SOURCE.len(), 1));
    completion.retarget(&now, now.len_bytes());
    let before_pick = editor.buffer().text();

    let accepted = completion.accept(now.len_bytes()).unwrap();
    apply(&mut editor, &accepted);
    editor.undo().unwrap();

    assert_eq!(editor.buffer().text(), before_pick);
}
