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
