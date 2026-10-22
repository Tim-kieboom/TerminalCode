use std::collections::HashMap;
use std::fs;
use std::path::Path;

use ropey::Rope;

use super::*;
use crate::lsp::uri::file_uri;
use crate::lsp::{FileEdits, LspPosition};

fn at(line: u32, character: u32) -> LspPosition {
    LspPosition { line, character }
}

/// Replaces `length` units at `line`, `character`.
fn edit(line: u32, character: u32, length: u32, new: &str) -> RenameEdit {
    RenameEdit {
        start: at(line, character),
        end: at(line, character + length),
        new_text: new.to_owned(),
    }
}

fn edits_of(path: &Path, edits: Vec<RenameEdit>) -> WorkspaceEdit {
    WorkspaceEdit {
        files: vec![FileEdits {
            uri: file_uri(path).unwrap(),
            edits,
        }],
        renames: Vec::new(),
        operations: Vec::new(),
    }
}

fn nothing_open(_: &Path) -> Option<OpenDocument> {
    None
}

fn plan_in(root: &Path, edit: &WorkspaceEdit) -> Result<Plan, EditError> {
    plan(
        edit,
        Check::Rename("old"),
        root,
        &HashMap::new(),
        nothing_open,
    )
}

fn project() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn a_file_that_is_not_open_gets_its_text_edited_and_written() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "let old = old + 1;\n").unwrap();
    let edit = edits_of(&path, vec![edit(0, 4, 3, "new"), edit(0, 10, 3, "new")]);

    let mut plan = plan_in(dir.path(), &edit).unwrap();
    assert_eq!((plan.places(), plan.files()), (2, 1));
    assert_eq!(fs::read_to_string(&path).unwrap(), "let old = old + 1;\n");
    plan.write_closed().unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "let new = new + 1;\n");
}

#[test]
fn crlf_line_endings_are_kept() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "one\r\nlet old;\r\ntwo\r\n").unwrap();
    let edit = edits_of(&path, vec![edit(1, 4, 3, "new")]);

    plan_in(dir.path(), &edit).unwrap().write_closed().unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "one\r\nlet new;\r\ntwo\r\n"
    );
}

#[test]
fn a_byte_order_mark_is_kept() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "\u{feff}one\nold\n").unwrap();
    let edit = edits_of(&path, vec![edit(1, 0, 3, "new")]);

    plan_in(dir.path(), &edit).unwrap().write_closed().unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "\u{feff}one\nnew\n");
}

#[test]
fn positions_count_utf16_units_after_accents_cjk_and_emoji() {
    let dir = project();
    let path = dir.path().join("a.rs");
    // é is one unit, 漢 one, 😀 two: `old` starts at unit 6 and byte 10.
    fs::write(&path, "é漢😀 old\n").unwrap();
    let edit = edits_of(&path, vec![edit(0, 5, 3, "new")]);

    plan_in(dir.path(), &edit).unwrap().write_closed().unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "é漢😀 new\n");
}

#[test]
fn several_edits_in_one_file_are_made_from_the_end() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "old old\nold\n").unwrap();
    let edit = edits_of(
        &path,
        vec![
            edit(1, 0, 3, "longer"),
            edit(0, 4, 3, "x"),
            edit(0, 0, 3, "longer"),
        ],
    );

    plan_in(dir.path(), &edit).unwrap().write_closed().unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "longer x\nlonger\n");
}

#[test]
fn text_that_is_not_the_name_means_the_server_saw_another_file() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "let other = 1;\n").unwrap();
    let edit = edits_of(&path, vec![edit(0, 4, 3, "new")]);

    let error = plan_in(dir.path(), &edit).unwrap_err();

    assert!(matches!(error, EditError::Stale(name) if name == "a.rs"));
}

#[test]
fn a_raw_identifier_counts_as_the_name() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "let r#old = 1;\n").unwrap();
    let edit = edits_of(&path, vec![edit(0, 4, 5, "new")]);

    assert!(plan_in(dir.path(), &edit).is_ok());
}

#[test]
fn overlapping_edits_are_refused() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "old\n").unwrap();
    let edit = edits_of(&path, vec![edit(0, 0, 3, "a"), edit(0, 0, 3, "b")]);

    let error = plan_in(dir.path(), &edit).unwrap_err();

    assert!(matches!(error, EditError::Overlap(_)));
}

#[test]
fn a_file_that_is_not_utf8_is_refused() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, b"old \xff\xfe\n").unwrap();
    let edit = edits_of(&path, vec![edit(0, 0, 3, "new")]);

    let error = plan_in(dir.path(), &edit).unwrap_err();

    assert!(matches!(error, EditError::NotUtf8(name) if name == "a.rs"));
}

#[test]
fn a_file_that_is_gone_or_read_only_is_refused() {
    let dir = project();
    let gone = dir.path().join("gone.rs");
    let error = plan_in(dir.path(), &edits_of(&gone, vec![edit(0, 0, 3, "new")])).unwrap_err();
    assert!(matches!(error, EditError::Missing(_)));

    let path = dir.path().join("a.rs");
    fs::write(&path, "old\n").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let error = plan_in(dir.path(), &edits_of(&path, vec![edit(0, 0, 3, "new")])).unwrap_err();
    assert!(matches!(error, EditError::ReadOnly(_)));
}

#[test]
fn a_file_outside_the_project_is_refused() {
    let dir = project();
    let outside = project();
    let path = outside.path().join("a.rs");
    fs::write(&path, "old\n").unwrap();

    let error = plan_in(dir.path(), &edits_of(&path, vec![edit(0, 0, 3, "new")])).unwrap_err();

    assert!(matches!(error, EditError::Outside(_)));
}

#[cfg(unix)]
#[test]
fn a_link_that_leads_out_of_the_project_is_refused() {
    let dir = project();
    let outside = project();
    let target = outside.path().join("a.rs");
    fs::write(&target, "old\n").unwrap();
    let link = dir.path().join("link.rs");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let error = plan_in(dir.path(), &edits_of(&link, vec![edit(0, 0, 3, "new")])).unwrap_err();

    assert!(matches!(error, EditError::Outside(_)));
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n");
}

#[cfg(unix)]
#[test]
fn a_link_inside_the_project_is_written_through() {
    let dir = project();
    let target = dir.path().join("a.rs");
    fs::write(&target, "old\n").unwrap();
    let link = dir.path().join("link.rs");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    plan_in(dir.path(), &edits_of(&link, vec![edit(0, 0, 3, "new")]))
        .unwrap()
        .write_closed()
        .unwrap();

    assert_eq!(fs::read_to_string(&target).unwrap(), "new\n");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn an_operation_on_a_file_is_refused_before_anything_is_looked_at() {
    let dir = project();
    let mut edit = edits_of(&dir.path().join("a.rs"), vec![edit(0, 0, 3, "new")]);
    edit.operations.push("rename a.rs to b.rs".to_owned());

    let error = plan_in(dir.path(), &edit).unwrap_err();

    assert!(matches!(error, EditError::Operation(text) if text == "rename a.rs to b.rs"));
}

#[test]
fn an_edit_that_names_no_file_is_refused() {
    let dir = project();
    let edit = WorkspaceEdit {
        files: vec![FileEdits {
            uri: "https://example.com/a.rs".to_owned(),
            edits: Vec::new(),
        }],
        renames: Vec::new(),
        operations: Vec::new(),
    };

    assert!(matches!(
        plan_in(dir.path(), &edit),
        Err(EditError::NotAFile(_))
    ));
}

fn open_document(id: u32, version: u64, text: &str, read_only: bool) -> OpenDocument {
    OpenDocument {
        id: DocumentId::new(id),
        version,
        text: Rope::from_str(text),
        read_only,
    }
}

#[test]
fn an_open_document_is_planned_by_bytes_and_its_file_is_not_touched() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "disk\n").unwrap();
    let sent = HashMap::from([(DocumentId::new(7), 3)]);
    let edit = edits_of(&path, vec![edit(0, 4, 3, "new")]);

    let plan = plan(&edit, Check::Rename("old"), dir.path(), &sent, |_| {
        Some(open_document(7, 3, "let old;\n", false))
    })
    .unwrap();

    assert_eq!(plan.open().len(), 1);
    assert_eq!(plan.open()[0].document, DocumentId::new(7));
    assert_eq!(plan.open()[0].replacements, [(4..7, "new".to_owned())]);
    assert_eq!(fs::read_to_string(&path).unwrap(), "disk\n");
}

#[test]
fn an_open_document_that_changed_since_the_server_was_asked_aborts() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "old\n").unwrap();
    let edit = edits_of(&path, vec![edit(0, 0, 3, "new")]);

    for sent in [HashMap::from([(DocumentId::new(7), 2)]), HashMap::new()] {
        let error = plan(&edit, Check::Rename("old"), dir.path(), &sent, |_| {
            Some(open_document(7, 3, "old\n", false))
        })
        .unwrap_err();
        assert!(matches!(error, EditError::Changed(name) if name == "a.rs"));
    }
}

#[test]
fn a_read_only_open_document_is_refused() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "old\n").unwrap();
    let sent = HashMap::from([(DocumentId::new(7), 3)]);
    let edit = edits_of(&path, vec![edit(0, 0, 3, "new")]);

    let error = plan(&edit, Check::Rename("old"), dir.path(), &sent, |_| {
        Some(open_document(7, 3, "old\n", true))
    })
    .unwrap_err();

    assert!(matches!(error, EditError::ReadOnly(_)));
}

#[test]
fn a_bad_file_stops_the_rename_before_the_good_one_is_written() {
    let dir = project();
    let good = dir.path().join("good.rs");
    let bad = dir.path().join("bad.rs");
    fs::write(&good, "old\n").unwrap();
    fs::write(&bad, "other\n").unwrap();
    let mut both = edits_of(&good, vec![edit(0, 0, 3, "new")]);
    both.files.push(FileEdits {
        uri: file_uri(&bad).unwrap(),
        edits: vec![edit(0, 0, 3, "new")],
    });

    assert!(plan_in(dir.path(), &both).is_err());

    assert_eq!(fs::read_to_string(&good).unwrap(), "old\n");
}

#[test]
fn a_file_that_cannot_be_staged_leaves_every_file_as_it_was() {
    let dir = project();
    let first = dir.path().join("a.rs");
    fs::write(&first, "old\n").unwrap();
    let mut plan = plan_in(dir.path(), &edits_of(&first, vec![edit(0, 0, 3, "new")])).unwrap();
    // The folder of the second file is gone by the time it is written.
    plan.closed.push(Closed {
        path: dir.path().join("gone/b.rs"),
        name: "gone/b.rs".to_owned(),
        text: "new\n".to_owned(),
    });

    let error = plan.write_closed().unwrap_err();

    assert!(matches!(error, EditError::Write { name, .. } if name == "gone/b.rs"));
    assert_eq!(fs::read_to_string(&first).unwrap(), "old\n");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no temp file");
}

#[test]
fn a_swap_that_fails_halfway_names_the_files_already_changed() {
    let dir = project();
    let first = dir.path().join("a.rs");
    let third = dir.path().join("c.rs");
    fs::write(&first, "old\n").unwrap();
    fs::write(&third, "old\n").unwrap();
    // A folder where the second file should be: a file cannot replace it.
    let blocked = dir.path().join("b.rs");
    fs::create_dir(&blocked).unwrap();
    fs::write(blocked.join("inner"), "x").unwrap();
    let mut plan = Plan {
        closed: ["a.rs", "b.rs", "c.rs"]
            .iter()
            .map(|name| Closed {
                path: dir.path().join(name),
                name: (*name).to_owned(),
                text: "new\n".to_owned(),
            })
            .collect(),
        open: Vec::new(),
        moves: Vec::new(),
        places: 3,
    };

    let error = plan.write_closed().unwrap_err();

    let EditError::Torn { done, failed, .. } = error else {
        panic!("expected a torn rename");
    };
    assert_eq!((done.as_str(), failed.as_str()), ("a.rs", "b.rs"));
    assert_eq!(fs::read_to_string(&first).unwrap(), "new\n");
    assert_eq!(
        fs::read_to_string(&third).unwrap(),
        "old\n",
        "what came after was not written"
    );
    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn a_place_follows_the_replacements_before_it() {
    let replacements = [(2..5, "x".to_owned()), (10..13, "longer".to_owned())];

    assert_eq!(remap(0, &replacements), 0);
    assert_eq!(remap(7, &replacements), 5, "after a shorter replacement");
    assert_eq!(remap(20, &replacements), 20 - 2 + 3);
    assert_eq!(
        remap(3, &replacements),
        3,
        "inside: the end of the new text"
    );
    assert_eq!(remap(11, &replacements), 8 + 6, "inside the second");
}

#[test]
fn an_action_edit_has_no_old_text_to_compare() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "fn main() {}\n").unwrap();
    let sent = HashMap::from([(DocumentId::new(1), 0)]);
    // An insertion at the top: the range is empty, nothing is replaced.
    let edit = edits_of(&path, vec![edit(0, 0, 0, "use std::fmt;\n")]);

    let plan = plan(&edit, Check::Action, dir.path(), &sent, |_| {
        Some(open_document(1, 0, "fn main() {}\n", false))
    })
    .unwrap();

    assert_eq!(
        plan.open()[0].replacements,
        [(0..0, "use std::fmt;\n".to_owned())]
    );
}

#[test]
fn an_action_edit_in_a_file_that_is_not_open_is_refused_and_nothing_is_read_or_written() {
    let dir = project();
    let path = dir.path().join("a.rs");
    fs::write(&path, "old\n").unwrap();
    let edit = edits_of(&path, vec![edit(0, 0, 0, "x")]);

    let error = plan(
        &edit,
        Check::Action,
        dir.path(),
        &HashMap::new(),
        nothing_open,
    )
    .unwrap_err();

    assert!(matches!(error, EditError::NotOpen(name) if name == "a.rs"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "old\n");
}
