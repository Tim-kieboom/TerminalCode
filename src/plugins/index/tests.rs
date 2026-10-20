use super::*;

const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn entry_json(extra: &str) -> String {
    format!(
        r#"{{"id":"git","name":"Git","version":"0.1.0","manifest":"https://github.com/a/b/m","module":"https://github.com/a/b/w","sha256":"{HASH}","abi":"1.3"{extra}}}"#
    )
}

fn index_json(format: u32, entries: &[String]) -> String {
    format!(r#"{{"format":{format},"plugins":[{}]}}"#, entries.join(","))
}

const HOST: Host<'static> = Host {
    abi: Abi::CURRENT,
    version: "0.6.2",
};

#[test]
fn the_snapshot_of_the_release_reads() {
    // The file in the repository is what ships; it must always be good.
    let index = Index::parse(SNAPSHOT).unwrap();
    // Every plugin it lists can be installed by the editor it ships with.
    for entry in &index.entries {
        assert_eq!(
            entry.availability(HOST),
            Availability::Ready,
            "{}",
            entry.id
        );
    }
}

#[test]
fn an_entry_is_read_with_its_fields() {
    let index = Index::parse(&index_json(1, &[entry_json(r#","description":"d""#)])).unwrap();
    let entry = index.entry("git").unwrap();
    assert_eq!(entry.name, "Git");
    assert_eq!(entry.description, "d");
    assert_eq!(entry.sha256, HASH);
    assert_eq!(entry.requires, None);
}

#[test]
fn unknown_fields_in_an_entry_are_ignored() {
    let text = index_json(1, &[entry_json(r#","homepage":"x""#)]);
    assert_eq!(Index::parse(&text).unwrap().entries.len(), 1);
}

#[test]
fn a_newer_format_says_to_update_even_when_its_shape_is_different() {
    let error = Index::parse(r#"{"format":2,"plugins":"not a list"}"#).unwrap_err();
    assert_eq!(error, IndexError::NewerFormat(2));
    assert!(error.to_string().contains("newer TerminalCode"));
}

#[test]
fn text_that_is_not_an_index_is_invalid() {
    assert!(matches!(
        Index::parse("not json"),
        Err(IndexError::Invalid(_))
    ));
    assert!(matches!(Index::parse("{}"), Err(IndexError::Invalid(_))));
}

#[test]
fn a_bad_entry_names_itself() {
    let bad_hash = entry_json("").replace(HASH, "ABC");
    let bad_id = entry_json("").replace(r#""id":"git""#, r#""id":"Git!""#);
    let bad_abi = entry_json("").replace("1.3", "one");
    let bad_requires = entry_json(r#","requires":"soon""#);
    for text in [bad_hash, bad_id, bad_abi, bad_requires] {
        let error = Index::parse(&index_json(1, &[text])).unwrap_err();
        assert!(matches!(error, IndexError::BadEntry { .. }), "{error}");
    }
}

#[test]
fn an_entry_that_needs_a_newer_editor_or_api_says_what() {
    let newer_editor = Index::parse(&index_json(1, &[entry_json(r#","requires":"9.0.0""#)]))
        .unwrap()
        .entries
        .remove(0);
    let availability = newer_editor.availability(HOST);
    assert_eq!(availability, Availability::NeedsEditor("9.0.0".into()));
    assert_eq!(
        availability.missing().as_deref(),
        Some("needs TerminalCode 9.0.0")
    );

    let newer_api = Index::parse(&index_json(1, &[entry_json("").replace("1.3", "9.0")]))
        .unwrap()
        .entries
        .remove(0);
    assert!(matches!(
        newer_api.availability(HOST),
        Availability::NeedsApi(AbiError::TooNew { .. })
    ));
}

#[test]
fn an_entry_this_editor_can_run_is_ready() {
    let entry = Index::parse(&index_json(1, &[entry_json(r#","requires":"0.6.0""#)]))
        .unwrap()
        .entries
        .remove(0);
    assert_eq!(entry.availability(HOST), Availability::Ready);
    assert_eq!(Availability::Ready.missing(), None);
}

#[test]
fn the_cache_wins_over_the_snapshot_and_a_broken_one_loses() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        Index::cached_or_snapshot(Some(dir.path()))
            .entries
            .is_empty()
    );

    store_cache(dir.path(), &index_json(1, &[entry_json("")])).unwrap();
    assert_eq!(Index::cached_or_snapshot(Some(dir.path())).entries.len(), 1);

    fs::write(dir.path().join(CACHE_FILE), "garbage").unwrap();
    assert!(
        Index::cached_or_snapshot(Some(dir.path()))
            .entries
            .is_empty()
    );
    assert_eq!(Index::cached_or_snapshot(None), Index::snapshot());
}

// --- more than one version of a plugin ---

fn versions(list: &[(&str, &str, Option<&str>)]) -> Index {
    let entries: Vec<String> = list
        .iter()
        .map(|(version, abi, requires)| {
            let requires = requires
                .map(|text| format!(r#","requires":"{text}""#))
                .unwrap_or_default();
            entry_json(&requires)
                .replace(r#""version":"0.1.0""#, &format!(r#""version":"{version}""#))
                .replace(r#""abi":"1.3""#, &format!(r#""abi":"{abi}""#))
        })
        .collect();
    Index::parse(&index_json(1, &entries)).unwrap()
}

#[test]
fn the_newest_version_this_editor_can_run_is_the_one_offered() {
    // Listed oldest first, as the file has them.
    let index = versions(&[("0.1.0", "1.3", None), ("0.2.0", "1.4", None)]);
    assert_eq!(index.entry_for("git", HOST).unwrap().version, "0.2.0");

    // An editor with the older API cannot run 0.2.0: it is offered 0.1.0.
    let older = Host {
        abi: Abi::new(1, 3),
        version: "0.6.2",
    };
    assert_eq!(index.entry_for("git", older).unwrap().version, "0.1.0");
}

#[test]
fn the_order_in_the_file_does_not_decide() {
    let index = versions(&[
        ("0.2.0", "1.4", None),
        ("0.1.0", "1.3", None),
        ("0.10.0", "1.4", None),
    ]);
    assert_eq!(
        index.entry_for("git", HOST).unwrap().version,
        "0.10.0",
        "numbers, not text"
    );
}

#[test]
fn an_editor_that_can_run_none_is_told_about_the_newest() {
    let index = versions(&[("0.2.0", "1.4", None), ("0.3.0", "1.4", Some("9.0.0"))]);
    let older = Host {
        abi: Abi::new(1, 3),
        version: "0.6.2",
    };
    let entry = index.entry_for("git", older).unwrap();
    assert_eq!(entry.version, "0.3.0");
    assert!(entry.availability(older).missing().is_some());
}

#[test]
fn a_plugin_listed_twice_is_one_id() {
    let index = versions(&[("0.1.0", "1.3", None), ("0.2.0", "1.4", None)]);
    assert_eq!(index.ids(), ["git"]);
    assert!(index.entry("nothing").is_none());
}

fn other_json(id: &str) -> String {
    entry_json("").replace(r#""id":"git""#, &format!(r#""id":"{id}""#))
}

#[test]
fn a_source_is_labelled_by_its_repository_or_its_host() {
    assert_eq!(
        source_label("https://raw.githubusercontent.com/you/plugins/main/index.json"),
        "you/plugins"
    );
    assert_eq!(
        source_label("https://github.com/you/plugins/raw/main/index.json"),
        "you/plugins"
    );
    assert_eq!(source_label("https://github.com"), "github.com");
    assert_eq!(source_label("https://example.org/a/b"), "example.org");
}

#[test]
fn an_index_cannot_say_it_is_the_official_one_or_another_source() {
    let json = entry_json(r#","source":"https://github.com/someone/else""#);
    let index = Index::parse(&index_json(1, &[json])).unwrap();
    assert_eq!(index.entry("git").unwrap().source, None);
    assert_eq!(index.entry("git").unwrap().source_label(), None);
}

#[test]
fn entries_of_a_source_say_where_they_come_from() {
    let url = "https://raw.githubusercontent.com/you/plugins/main/index.json";
    let index = Index::parse(&index_json(1, &[entry_json("")]))
        .unwrap()
        .listed_by(url);
    let entry = index.entry("git").unwrap();
    assert_eq!(entry.source.as_deref(), Some(url));
    assert_eq!(entry.source_label().as_deref(), Some("you/plugins"));
}

#[test]
fn the_source_is_kept_in_the_marker_text_and_the_official_leaves_it_out() {
    let official = Index::parse(&index_json(1, &[entry_json("")])).unwrap();
    let text = serde_json::to_string(official.entry("git").unwrap()).unwrap();
    assert!(!text.contains("source"), "{text}");
    let sourced = official.listed_by("https://github.com/a/b/raw/main/index.json");
    let text = serde_json::to_string(sourced.entry("git").unwrap()).unwrap();
    let back: Entry = serde_json::from_str(&text).unwrap();
    assert_eq!(back.source, sourced.entry("git").unwrap().source);
}

#[test]
fn a_source_cannot_take_the_name_of_an_official_plugin_or_an_earlier_source() {
    let official = Index::parse(&index_json(1, &[entry_json("")])).unwrap();
    let first = Index::parse(&index_json(1, &[other_json("git"), other_json("one")]))
        .unwrap()
        .listed_by("https://github.com/a/a/raw/main/index.json");
    let second = Index::parse(&index_json(1, &[other_json("one"), other_json("two")]))
        .unwrap()
        .listed_by("https://github.com/b/b/raw/main/index.json");

    let (merged, left_out) = Index::merged(
        &official,
        &[
            (
                "https://github.com/a/a/raw/main/index.json".to_owned(),
                first,
            ),
            (
                "https://github.com/b/b/raw/main/index.json".to_owned(),
                second,
            ),
        ],
    );

    assert_eq!(merged.ids(), ["git", "one", "two"]);
    assert_eq!(merged.entry("git").unwrap().source, None);
    assert!(
        merged
            .entry("one")
            .unwrap()
            .source
            .as_deref()
            .unwrap()
            .contains("/a/a/")
    );
    assert_eq!(left_out, ["git", "one"]);
}

#[test]
fn a_source_may_list_several_versions_of_its_own_plugin() {
    let url = "https://github.com/a/a/raw/main/index.json";
    let newer = entry_json("").replace("0.1.0", "0.2.0");
    let source = Index::parse(&index_json(1, &[entry_json(""), newer]))
        .unwrap()
        .listed_by(url);
    let (merged, left_out) = Index::merged(&Index::default(), &[(url.to_owned(), source)]);
    assert!(left_out.is_empty());
    assert_eq!(merged.entries.len(), 2);
}
