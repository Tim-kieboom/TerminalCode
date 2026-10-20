use std::path::Path;

use super::*;

fn kinds(list: &[&str]) -> Vec<String> {
    list.iter().map(|kind| (*kind).to_owned()).collect()
}

#[test]
fn a_file_has_a_kind_by_its_extension_in_any_case() {
    let md = kinds(&["md"]);
    assert!(has_kind(Path::new("docs/A.MD"), &md));
    assert!(has_kind(Path::new("a.md"), &md));
    assert!(!has_kind(Path::new("a.rs"), &md));
    assert!(!has_kind(Path::new("md"), &md));
    assert!(!has_kind(Path::new(".env"), &md));
}

#[test]
fn the_setting_only_takes_kinds_away() {
    let manifest = kinds(&["md", "Markdown"]);
    assert_eq!(effective_kinds(&manifest, None), ["md", "markdown"]);
    assert_eq!(effective_kinds(&manifest, Some(&kinds(&["md"]))), ["md"]);
    // A kind the manifest does not name cannot be added.
    assert_eq!(
        effective_kinds(&manifest, Some(&kinds(&["md", "txt"]))),
        ["md"]
    );
    assert!(effective_kinds(&manifest, Some(&Vec::new())).is_empty());
}

fn active(path: &Path) -> ActiveFile {
    ActiveFile {
        path: path.to_path_buf(),
        modified: false,
        saves: 0,
        reloads: 0,
    }
}

#[test]
fn a_file_that_is_not_utf8_or_not_there_is_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let odd = dir.path().join("odd.md");
    fs::write(&odd, [0xff, 0xfe, 0x00]).unwrap();
    assert_eq!(
        file_message(&active(&odd), "odd.md".to_owned()),
        ToPlugin::FileCleared
    );
    assert_eq!(
        file_message(&active(&dir.path().join("gone.md")), "gone.md".to_owned()),
        ToPlugin::FileCleared
    );
}

#[test]
fn text_that_is_longer_as_json_than_a_message_may_be_is_told_as_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("controls.md");
    // Under a megabyte, and six bytes each as JSON.
    fs::write(&file, "\u{1}".repeat(MAX_FILE_TEXT - 1)).unwrap();
    assert!(matches!(
        file_message(&active(&file), "controls.md".to_owned()),
        ToPlugin::FileTooLarge { .. }
    ));
}

#[test]
fn a_file_of_a_megabyte_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("full.md");
    fs::write(&file, "x".repeat(MAX_FILE_TEXT)).unwrap();
    assert!(matches!(
        file_message(&active(&file), "full.md".to_owned()),
        ToPlugin::FileText { .. }
    ));
}
