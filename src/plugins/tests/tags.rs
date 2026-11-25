use crate::plugins::index::Index;
use crate::plugins::manifest::{Host, MANIFEST_FILE, read};
use crate::plugins::tags::{Kind, Tag, known};

use trcode_plugin_api::Abi;

const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn names(tags: &[Tag]) -> Vec<&'static str> {
    tags.iter().map(|tag| tag.name()).collect()
}

#[test]
fn a_tag_is_found_by_its_name_whatever_the_case_and_the_space() {
    assert_eq!(Tag::parse(" Rust ").map(Tag::name), Some("rust"));
    assert_eq!(Tag::parse("C++").map(Tag::name), Some("c++"));
    assert_eq!(Tag::parse("git").map(Tag::kind), Some(Kind::Purpose));
    assert_eq!(Tag::parse("go").map(Tag::kind), Some(Kind::Language));
}

#[test]
fn a_name_outside_the_set_is_not_a_tag() {
    for name in ["vcs", "official", "", "rust lang", "git-viewer"] {
        assert_eq!(Tag::parse(name), None, "{name:?}");
    }
}

#[test]
fn the_names_of_the_set_are_each_there_once() {
    let all = names(Tag::all());
    for (at, name) in all.iter().enumerate() {
        assert!(!all[..at].contains(name), "{name} is in the set twice");
        assert_eq!(Tag::parse(name).map(Tag::name), Some(*name));
    }
}

#[test]
fn what_a_plugin_is_comes_before_its_languages_in_the_set() {
    let kinds: Vec<Kind> = Tag::all().iter().map(|tag| tag.kind()).collect();
    let first_language = kinds
        .iter()
        .position(|kind| *kind == Kind::Language)
        .unwrap();
    assert!(
        kinds[first_language..]
            .iter()
            .all(|kind| *kind == Kind::Language)
    );
}

#[test]
fn known_tags_come_once_in_the_order_of_the_set_and_the_unknown_are_dropped() {
    let found = known(&["rust", "Syntax", "vcs", "rust", "official"]);
    assert_eq!(names(&found), ["syntax", "rust"]);
}

#[test]
fn no_tags_is_fine() {
    assert!(known::<&str>(&[]).is_empty());
}

fn index_with(extra: &str) -> Index {
    let text = format!(
        r#"{{"format":1,"plugins":[{{"id":"sol","name":"Sol","version":"0.1.0","manifest":"https://github.com/a/b/m","module":"https://github.com/a/b/w","sha256":"{HASH}","abi":"1.3"{extra}}}]}}"#
    );
    Index::parse(&text).unwrap()
}

#[test]
fn an_index_entry_copies_the_tags_and_leaves_the_unknown_out_when_shown() {
    let index = index_with(r#","tags":["syntax","Rust","vcs"]"#);
    let entry = index.entry("sol").unwrap();
    assert_eq!(names(&entry.known_tags()), ["syntax", "rust"]);
}

#[test]
fn an_index_entry_without_tags_has_none() {
    let index = index_with("");
    assert!(index.entry("sol").unwrap().known_tags().is_empty());
}

#[test]
fn tags_survive_being_written_and_read_again_as_installed_json_does() {
    let index = index_with(r#","tags":["git"]"#);
    let entry = index.entry("sol").unwrap();
    let again: crate::plugins::index::Entry =
        serde_json::from_str(&serde_json::to_string(entry).unwrap()).unwrap();
    assert_eq!(&again, entry);
    assert_eq!(names(&again.known_tags()), ["git"]);
}

#[test]
fn an_entry_without_tags_writes_no_field() {
    let index = index_with("");
    let text = serde_json::to_string(index.entry("sol").unwrap()).unwrap();
    assert!(!text.contains("tags"), "{text}");
}

fn read_manifest(extra: &str) -> crate::plugins::manifest::Installed {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("viewer");
    std::fs::create_dir_all(&dir).unwrap();
    let text = format!(
        "id = \"viewer\"\nversion = \"0.1.0\"\nabi = \"1.2\"\nwasm = \"viewer.wasm\"\n{extra}"
    );
    std::fs::write(dir.join(MANIFEST_FILE), text).unwrap();
    std::fs::write(dir.join("viewer.wasm"), b"\0asm").unwrap();
    let host = Host {
        abi: Abi::new(1, 2),
        version: "0.7.1",
    };
    read(&dir, host).unwrap()
}

#[test]
fn a_manifest_lists_its_tags() {
    let installed = read_manifest("tags = [\"syntax\", \"c++\"]\n");
    assert_eq!(names(&installed.known_tags()), ["syntax", "c++"]);
}

#[test]
fn an_unknown_tag_in_a_manifest_does_not_stop_the_plugin() {
    let installed = read_manifest("tags = [\"git\", \"my own thing\"]\n");
    assert_eq!(names(&installed.known_tags()), ["git"]);
    assert_eq!(
        installed.manifest.tags.len(),
        2,
        "what was written is kept as it is"
    );
}

#[test]
fn a_manifest_without_tags_has_none() {
    assert!(read_manifest("").known_tags().is_empty());
}
