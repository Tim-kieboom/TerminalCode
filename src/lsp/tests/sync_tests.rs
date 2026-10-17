use std::path::PathBuf;

use ropey::Rope;
use serde_json::{Value, json};

use crate::ids::DocumentId;
use crate::lsp::ServerKey;
use crate::lsp::position::LspPosition;
use crate::lsp::sync::{DocSnapshot, DocumentSync, Outgoing, language_id};

fn key(name: &str) -> ServerKey {
    ServerKey {
        name: name.to_owned(),
        root: PathBuf::from("/p"),
    }
}

fn doc(id: u32, uri: &str, text: &str, version: u64, saves: u64) -> DocSnapshot {
    DocSnapshot {
        id: DocumentId::new(id),
        uri: uri.to_owned(),
        language_id: "rust".to_owned(),
        text: Rope::from_str(text),
        version,
        saves,
        reloads: 0,
    }
}

fn methods(messages: &[Outgoing]) -> Vec<&str> {
    messages.iter().map(|message| message.method).collect()
}

const A: &str = "file:///p/a.rs";
const B: &str = "file:///p/b.rs";

#[test]
fn a_document_the_server_has_not_seen_is_opened_with_all_its_text() {
    let mut sync = DocumentSync::default();

    let messages = sync.messages(&key("rust"), 1, &[doc(1, A, "fn main() {}\n", 0, 0)]);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].method, "textDocument/didOpen");
    assert_eq!(
        messages[0].params,
        json!({ "textDocument": { "uri": A, "languageId": "rust", "version": 0, "text": "fn main() {}\n" } })
    );
}

#[test]
fn nothing_is_sent_when_nothing_changed() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "text", 0, 0)]);

    assert!(
        sync.messages(&key("rust"), 1, &[doc(1, A, "text", 0, 0)])
            .is_empty()
    );
    assert!(
        sync.messages(&key("rust"), 1, &[doc(1, A, "text", 0, 0)])
            .is_empty()
    );
}

#[test]
fn typing_is_one_change_with_the_range_in_the_text_the_server_has() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "fn main() {}\n", 0, 0)]);

    let messages = sync.messages(&key("rust"), 1, &[doc(1, A, "fn mxain() {}\n", 1, 0)]);

    assert_eq!(methods(&messages), ["textDocument/didChange"]);
    assert_eq!(
        messages[0].params,
        json!({
            "textDocument": { "uri": A, "version": 1 },
            "contentChanges": [{
                "range": { "start": { "line": 0, "character": 4 }, "end": { "line": 0, "character": 4 } },
                "text": "x",
            }],
        })
    );
}

#[test]
fn a_burst_of_typing_between_two_looks_is_one_message() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "ab", 0, 0)]);

    // Five edits happened (the version went 0 to 5), and the server is told once.
    let messages = sync.messages(&key("rust"), 1, &[doc(1, A, "abcdef", 5, 0)]);

    assert_eq!(methods(&messages), ["textDocument/didChange"]);
    assert_eq!(
        messages[0].params["contentChanges"][0]["text"],
        json!("cdef")
    );
    assert_eq!(messages[0].params["textDocument"]["version"], json!(5));
}

#[test]
fn the_next_change_is_worked_out_against_what_the_server_was_told_last() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "a", 0, 0)]);
    sync.messages(&key("rust"), 1, &[doc(1, A, "ab", 1, 0)]);

    let messages = sync.messages(&key("rust"), 1, &[doc(1, A, "abc", 2, 0)]);

    assert_eq!(
        messages[0].params["contentChanges"][0]["range"]["start"],
        json!({ "line": 0, "character": 2 })
    );
}

#[test]
fn a_change_after_a_wide_character_counts_utf_16_units() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "a😀b", 0, 0)]);

    let messages = sync.messages(&key("rust"), 1, &[doc(1, A, "a😀xb", 1, 0)]);

    assert_eq!(
        messages[0].params["contentChanges"][0]["range"]["start"],
        json!({ "line": 0, "character": 3 })
    );
}

#[test]
fn a_version_that_moved_with_the_same_text_sends_nothing_but_is_remembered() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "ab", 0, 0)]);

    // Typed "c" and deleted it again.
    assert!(
        sync.messages(&key("rust"), 1, &[doc(1, A, "ab", 2, 0)])
            .is_empty()
    );
    let messages = sync.messages(&key("rust"), 1, &[doc(1, A, "abz", 3, 0)]);

    assert_eq!(messages[0].params["textDocument"]["version"], json!(3));
}

#[test]
fn a_save_is_told_once_after_the_changes() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "a", 0, 0)]);

    let messages = sync.messages(&key("rust"), 1, &[doc(1, A, "ab", 1, 1)]);
    assert_eq!(
        methods(&messages),
        ["textDocument/didChange", "textDocument/didSave"]
    );
    assert_eq!(messages[1].params, json!({ "textDocument": { "uri": A } }));

    assert!(
        sync.messages(&key("rust"), 1, &[doc(1, A, "ab", 1, 1)])
            .is_empty()
    );
    let again = sync.messages(&key("rust"), 1, &[doc(1, A, "ab", 1, 2)]);
    assert_eq!(methods(&again), ["textDocument/didSave"]);
}

#[test]
fn a_document_that_is_gone_is_closed_and_opened_again_if_it_comes_back() {
    let mut sync = DocumentSync::default();
    sync.messages(
        &key("rust"),
        1,
        &[doc(1, A, "a", 0, 0), doc(2, B, "b", 0, 0)],
    );

    let closed = sync.messages(&key("rust"), 1, &[doc(2, B, "b", 0, 0)]);
    assert_eq!(methods(&closed), ["textDocument/didClose"]);
    assert_eq!(closed[0].params, json!({ "textDocument": { "uri": A } }));

    let reopened = sync.messages(
        &key("rust"),
        1,
        &[doc(1, A, "a", 0, 0), doc(2, B, "b", 0, 0)],
    );
    assert_eq!(methods(&reopened), ["textDocument/didOpen"]);
}

#[test]
fn a_document_with_a_new_name_is_closed_under_the_old_one_first() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "a", 0, 0)]);

    let messages = sync.messages(&key("rust"), 1, &[doc(1, B, "a", 0, 0)]);

    assert_eq!(
        methods(&messages),
        ["textDocument/didClose", "textDocument/didOpen"]
    );
    assert_eq!(messages[0].params["textDocument"]["uri"], json!(A));
    assert_eq!(messages[1].params["textDocument"]["uri"], json!(B));
}

#[test]
fn a_server_that_started_again_is_told_everything_from_the_start() {
    let mut sync = DocumentSync::default();
    sync.messages(
        &key("rust"),
        1,
        &[doc(1, A, "a", 0, 0), doc(2, B, "b", 0, 0)],
    );

    let messages = sync.messages(
        &key("rust"),
        2,
        &[doc(1, A, "a", 0, 0), doc(2, B, "b", 0, 0)],
    );

    assert_eq!(
        methods(&messages),
        ["textDocument/didOpen", "textDocument/didOpen"]
    );
    assert!(
        sync.messages(
            &key("rust"),
            2,
            &[doc(1, A, "a", 0, 0), doc(2, B, "b", 0, 0)]
        )
        .is_empty()
    );
}

#[test]
fn servers_are_told_apart() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "a", 0, 0)]);

    let other = sync.messages(&key("other"), 1, &[doc(1, A, "a", 0, 0)]);

    assert_eq!(
        methods(&other),
        ["textDocument/didOpen"],
        "a second server has seen nothing"
    );
    assert!(
        sync.messages(&key("rust"), 1, &[doc(1, A, "a", 0, 0)])
            .is_empty()
    );
}

#[test]
fn a_server_with_no_documents_closes_what_it_had() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "a", 0, 0)]);

    let messages = sync.messages(&key("rust"), 1, &[]);

    assert_eq!(methods(&messages), ["textDocument/didClose"]);
}

#[test]
fn forgetting_everything_opens_everything_again() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "a", 0, 0)]);

    sync.clear();

    assert_eq!(
        methods(&sync.messages(&key("rust"), 1, &[doc(1, A, "a", 0, 0)])),
        ["textDocument/didOpen"]
    );
}

#[test]
fn a_reload_from_disk_that_replaces_the_text_is_one_change() {
    let mut sync = DocumentSync::default();
    sync.messages(&key("rust"), 1, &[doc(1, A, "old text\nline 2\n", 0, 0)]);

    let messages = sync.messages(
        &key("rust"),
        1,
        &[doc(1, A, "new text\nline 2\nline 3\n", 1, 0)],
    );

    assert_eq!(methods(&messages), ["textDocument/didChange"]);
    // What the server ends up with is the new text, whatever the range.
    let change: &Value = &messages[0].params["contentChanges"][0];
    assert!(change["text"].as_str().unwrap().contains("new"));
}

#[test]
fn language_ids_are_what_servers_call_them() {
    assert_eq!(language_id("rs"), "rust");
    assert_eq!(language_id("RS"), "rust");
    assert_eq!(language_id("py"), "python");
    assert_eq!(language_id("md"), "markdown");
    assert_eq!(language_id("sh"), "shellscript");
    assert_eq!(
        language_id("unheardof"),
        "unheardof",
        "an extension nobody knows is its own name"
    );
}

// ---- what the sync says about the texts, and placing an answer about an older one

fn at_version(id: u32, text: &str, version: u64) -> DocSnapshot {
    doc(id, A, text, version, 0)
}

fn point(line: u32, character: u32) -> LspPosition {
    LspPosition { line, character }
}

#[test]
fn a_change_is_reported_with_its_bytes_and_an_open_is_a_reset() {
    let mut sync = DocumentSync::default();
    let opened = sync.update(&key("rust"), 1, &[doc(1, A, "hello world", 0, 0)]);
    assert_eq!(opened.resets, [DocumentId::new(1)]);
    assert!(opened.changes.is_empty());

    let typed = sync.update(&key("rust"), 1, &[doc(1, A, "hello big world", 1, 0)]);

    assert!(typed.resets.is_empty());
    assert_eq!(typed.changes.len(), 1);
    let (id, change) = typed.changes[0];
    assert_eq!(id, DocumentId::new(1));
    assert_eq!((change.start, change.old_end, change.new_end), (6, 6, 10));
}

#[test]
fn a_document_that_is_closed_or_served_by_a_restarted_server_is_a_reset() {
    let mut sync = DocumentSync::default();
    sync.update(
        &key("rust"),
        1,
        &[doc(1, A, "a", 0, 0), doc(2, B, "b", 0, 0)],
    );

    let closed = sync.update(&key("rust"), 1, &[doc(2, B, "b", 0, 0)]);
    assert_eq!(closed.resets, [DocumentId::new(1)]);

    let restarted = sync.update(&key("rust"), 2, &[doc(2, B, "b", 0, 0)]);
    assert!(restarted.resets.contains(&DocumentId::new(2)));
}

#[test]
fn a_text_read_again_from_the_file_is_a_reset_and_not_a_change_to_follow() {
    let mut sync = DocumentSync::default();
    sync.update(&key("rust"), 1, &[doc(1, A, "old text", 0, 0)]);
    let mut reloaded = doc(1, A, "new text", 1, 0);
    reloaded.reloads = 1;

    let output = sync.update(&key("rust"), 1, &[reloaded]);

    assert_eq!(
        methods(&output.messages),
        ["textDocument/didChange"],
        "the server is still told"
    );
    assert_eq!(output.resets, [DocumentId::new(1)]);
    assert!(output.changes.is_empty());
}

#[test]
fn a_reload_to_the_same_text_is_still_a_reset() {
    let mut sync = DocumentSync::default();
    sync.update(&key("rust"), 1, &[doc(1, A, "same", 0, 0)]);
    let mut reloaded = doc(1, A, "same", 1, 0);
    reloaded.reloads = 1;

    let output = sync.update(&key("rust"), 1, &[reloaded]);

    assert!(output.messages.is_empty());
    assert_eq!(output.resets, [DocumentId::new(1)]);
}

#[test]
fn positions_in_the_latest_text_are_bytes_of_it() {
    let mut sync = DocumentSync::default();
    sync.update(&key("rust"), 1, &[at_version(1, "fn main() {}\n", 0)]);

    let placed = sync
        .ranges_in_told_text(
            &key("rust"),
            DocumentId::new(1),
            None,
            &[(point(0, 3), point(0, 7))],
        )
        .unwrap();

    assert_eq!(placed, vec![Some(3..7)]);
}

#[test]
fn positions_in_an_older_text_are_moved_through_the_changes_sent_since() {
    let mut sync = DocumentSync::default();
    sync.update(&key("rust"), 1, &[at_version(1, "fn main() {}\n", 0)]);
    sync.update(&key("rust"), 1, &[at_version(1, "// fn main() {}\n", 1)]);
    sync.update(
        &key("rust"),
        1,
        &[at_version(1, "// fn main(x: u8) {}\n", 2)],
    );

    // The server answers about version 0, where "main" was at columns 3 to 7.
    let placed = sync
        .ranges_in_told_text(
            &key("rust"),
            DocumentId::new(1),
            Some(0),
            &[(point(0, 3), point(0, 7))],
        )
        .unwrap();

    assert_eq!(
        placed,
        vec![Some(6..10)],
        "three bytes were typed in front of it"
    );
}

#[test]
fn a_range_a_later_change_took_is_none_and_the_others_stay() {
    let mut sync = DocumentSync::default();
    sync.update(
        &key("rust"),
        1,
        &[at_version(1, "let abc = 1;\nlet xyz = 2;\n", 0)],
    );
    // "abc" is replaced.
    sync.update(
        &key("rust"),
        1,
        &[at_version(1, "let q = 1;\nlet xyz = 2;\n", 1)],
    );

    let placed = sync
        .ranges_in_told_text(
            &key("rust"),
            DocumentId::new(1),
            Some(0),
            &[(point(0, 4), point(0, 7)), (point(1, 4), point(1, 7))],
        )
        .unwrap();

    assert_eq!(placed[0], None);
    // "xyz" was at bytes 17 to 20 (line 0 has 13 bytes with its break) and "abc" lost two bytes.
    assert_eq!(placed[1], Some(15..18));
}

#[test]
fn an_answer_about_a_version_that_is_not_kept_gives_nothing() {
    let mut sync = DocumentSync::default();
    sync.update(&key("rust"), 1, &[at_version(1, "a", 5)]);

    assert!(
        sync.ranges_in_told_text(&key("rust"), DocumentId::new(1), Some(4), &[])
            .is_none()
    );
    assert!(
        sync.ranges_in_told_text(&key("rust"), DocumentId::new(1), Some(6), &[])
            .is_none()
    );
    assert!(
        sync.ranges_in_told_text(&key("rust"), DocumentId::new(9), None, &[])
            .is_none()
    );
    assert!(
        sync.ranges_in_told_text(&key("other"), DocumentId::new(1), None, &[])
            .is_none()
    );
}

#[test]
fn only_so_many_old_versions_are_kept() {
    let mut sync = DocumentSync::default();
    let mut text = String::from("x");
    sync.update(&key("rust"), 1, &[at_version(1, &text, 0)]);
    for version in 1..=40 {
        text.push('x');
        sync.update(&key("rust"), 1, &[at_version(1, &text, version)]);
    }

    let old = sync.ranges_in_told_text(&key("rust"), DocumentId::new(1), Some(0), &[]);
    let recent = sync.ranges_in_told_text(&key("rust"), DocumentId::new(1), Some(30), &[]);

    assert!(old.is_none(), "version 0 is too old to be kept");
    assert!(recent.is_some());
}

#[test]
fn a_version_that_moved_without_a_change_still_finds_the_last_text_sent() {
    let mut sync = DocumentSync::default();
    sync.update(&key("rust"), 1, &[at_version(1, "ab", 3)]);
    // Typed and undone: the version moved, the text did not, nothing was sent.
    sync.update(&key("rust"), 1, &[at_version(1, "ab", 5)]);

    let placed = sync.ranges_in_told_text(
        &key("rust"),
        DocumentId::new(1),
        Some(3),
        &[(point(0, 1), point(0, 2))],
    );

    assert_eq!(placed, Some(vec![Some(1..2)]));
}

#[test]
fn a_server_that_is_dropped_gives_back_the_documents_it_had() {
    let mut sync = DocumentSync::default();
    sync.update(&key("rust"), 1, &[at_version(1, "a", 0)]);

    assert_eq!(sync.servers(), [key("rust")]);
    assert_eq!(sync.drop_server(&key("rust")), [DocumentId::new(1)]);
    assert!(sync.servers().is_empty());
    assert!(sync.drop_server(&key("rust")).is_empty());
}
