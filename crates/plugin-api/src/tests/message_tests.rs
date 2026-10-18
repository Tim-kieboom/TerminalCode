//! The shapes of the messages are a promise to plugins already built, so the
//! messages of ABI 1.0 are written out here, byte for byte. A change that makes
//! one of these fail breaks plugins in the field: it needs a new major version,
//! not a change to the test.

use crate::{
    ActionDecl, Capability, FromPlugin, GitEntry, GitKind, GitSection, GitSummary, MAX_MESSAGE,
    MessageError, Refusal, ToPlugin, WireLine, WireSpan, WireView,
};

// --- ABI 1.0, as written ---

const START: &str = r#"{"type":"start"}"#;
const REFRESH: &str = r#"{"type":"refresh"}"#;
const SET_LINES: &str = r#"{"type":"set_view","view":{"type":"lines","lines":[{"text":"hello","slot":"explorer.file"}]}}"#;
const SET_LIST: &str =
    r#"{"type":"set_view","view":{"type":"list","items":["a","b"],"selected":1}}"#;
const SET_LIST_NOTHING_SELECTED: &str =
    r#"{"type":"set_view","view":{"type":"list","items":["a"],"selected":null}}"#;

#[test]
fn the_messages_of_1_0_are_written_as_they_always_were() {
    assert_eq!(ToPlugin::Start.to_bytes().unwrap(), START.as_bytes());
    assert_eq!(ToPlugin::Refresh.to_bytes().unwrap(), REFRESH.as_bytes());
    let lines = FromPlugin::set_view(WireView::lines([WireLine::new("hello", "explorer.file")]));
    assert_eq!(lines.to_bytes().unwrap(), SET_LINES.as_bytes());
    let list = FromPlugin::set_view(WireView::list(["a".to_owned(), "b".to_owned()], Some(1)));
    assert_eq!(list.to_bytes().unwrap(), SET_LIST.as_bytes());
    let none = FromPlugin::set_view(WireView::list(["a".to_owned()], None));
    assert_eq!(
        none.to_bytes().unwrap(),
        SET_LIST_NOTHING_SELECTED.as_bytes()
    );
}

#[test]
fn the_messages_of_1_0_are_still_read() {
    assert_eq!(ToPlugin::from_bytes(START.as_bytes()), Ok(ToPlugin::Start));
    assert_eq!(
        ToPlugin::from_bytes(REFRESH.as_bytes()),
        Ok(ToPlugin::Refresh)
    );
    assert_eq!(
        FromPlugin::from_bytes(SET_LINES.as_bytes()),
        Ok(FromPlugin::set_view(WireView::lines([WireLine::new(
            "hello",
            "explorer.file"
        )])))
    );
    assert_eq!(
        FromPlugin::from_bytes(SET_LIST.as_bytes()),
        Ok(FromPlugin::set_view(WireView::list(
            ["a".to_owned(), "b".to_owned()],
            Some(1)
        )))
    );
    assert_eq!(
        FromPlugin::from_bytes(SET_LIST_NOTHING_SELECTED.as_bytes()),
        Ok(FromPlugin::set_view(WireView::list(["a".to_owned()], None)))
    );
}

// --- a plugin from before: it leaves out what was added later ---

#[test]
fn a_field_that_is_missing_reads_as_its_default() {
    let from_an_older_plugin = [
        (
            r#"{"type":"set_view","view":{"type":"lines"}}"#,
            WireView::lines([]),
        ),
        (
            r#"{"type":"set_view","view":{"type":"lines","lines":[{}]}}"#,
            WireView::lines([WireLine::new("", "")]),
        ),
        (
            r#"{"type":"set_view","view":{"type":"lines","lines":[{"text":"x"}]}}"#,
            WireView::lines([WireLine::new("x", "")]),
        ),
        (
            r#"{"type":"set_view","view":{"type":"list"}}"#,
            WireView::list([], None),
        ),
    ];

    for (json, view) in from_an_older_plugin {
        assert_eq!(
            FromPlugin::from_bytes(json.as_bytes()),
            Ok(FromPlugin::set_view(view)),
            "{json}"
        );
    }
}

// --- a plugin from later: it says what this version does not know ---

#[test]
fn a_field_that_is_not_known_is_skipped() {
    let from_a_later_plugin = r#"{"type":"set_view","priority":3,"view":{"type":"lines","color":"red","lines":[{"text":"x","slot":"y","bold":true,"icon":"!"}]}}"#;

    assert_eq!(
        FromPlugin::from_bytes(from_a_later_plugin.as_bytes()),
        Ok(FromPlugin::set_view(WireView::lines([WireLine::new(
            "x", "y"
        )])))
    );
}

#[test]
fn a_kind_of_message_that_is_not_known_is_unknown_not_an_error() {
    assert_eq!(
        FromPlugin::from_bytes(br#"{"type":"teleport","to":"mars"}"#),
        Ok(FromPlugin::Unknown)
    );
}

#[test]
fn a_kind_of_view_that_is_not_known_is_unknown_not_an_error() {
    assert_eq!(
        FromPlugin::from_bytes(br#"{"type":"set_view","view":{"type":"hologram","depth":3}}"#),
        Ok(FromPlugin::set_view(WireView::Unknown))
    );
}

// --- an editor from later: it sends what this version does not know ---

#[test]
fn a_message_from_a_later_editor_is_unknown_and_its_fields_are_skipped() {
    assert_eq!(
        ToPlugin::from_bytes(br#"{"type":"focus_changed","focused":true}"#),
        Ok(ToPlugin::Unknown)
    );
    assert_eq!(
        ToPlugin::from_bytes(br#"{"type":"start","reason":"restart"}"#),
        Ok(ToPlugin::Start)
    );
}

// --- what is not a message ---

#[test]
fn what_is_not_a_message_is_an_error() {
    for text in [
        "",
        "nope",
        "[]",
        "42",
        "null",
        r#"{"kind":"start"}"#,
        r#"{"type":7}"#,
        r#"{"type":"set_view"}"#,
        r#"{"type":"set_view","view":"lines"}"#,
    ] {
        assert!(
            matches!(
                FromPlugin::from_bytes(text.as_bytes()),
                Err(MessageError::Invalid(_))
            ),
            "{text:?}"
        );
    }
}

#[test]
fn bytes_that_are_not_text_are_an_error() {
    assert!(matches!(
        ToPlugin::from_bytes(&[0xff, 0xfe, 0x00]),
        Err(MessageError::Invalid(_))
    ));
}

#[test]
fn a_message_that_is_too_big_is_refused_both_ways() {
    let big = "x".repeat(MAX_MESSAGE);
    let message = FromPlugin::set_view(WireView::lines([WireLine::new(big.clone(), "")]));

    assert!(matches!(
        message.to_bytes(),
        Err(MessageError::TooBig(size)) if size > MAX_MESSAGE
    ));
    let oversized = vec![b' '; MAX_MESSAGE + 1];
    assert_eq!(
        FromPlugin::from_bytes(&oversized),
        Err(MessageError::TooBig(MAX_MESSAGE + 1))
    );
}

#[test]
fn text_with_quotes_and_unicode_survives_the_trip() {
    let line = WireLine::new("a \"quoted\" line\twith\nbreaks and ünï©ode ✓", "x");
    let message = FromPlugin::set_view(WireView::lines([line]));

    let bytes = message.to_bytes().unwrap();

    assert_eq!(FromPlugin::from_bytes(&bytes), Ok(message));
}

// --- ABI 1.1, as written: actions, keys and the input view ---

const ACTION: &str = r#"{"type":"action","action":"stage"}"#;
const TEXT: &str = r#"{"type":"text","text":"hi \"there\""}"#;
const KEY: &str = r#"{"type":"key","key":"backspace","ctrl":false,"alt":false,"shift":false}"#;
const KEY_WITH_CTRL: &str = r#"{"type":"key","key":"s","ctrl":true,"alt":false,"shift":false}"#;
const INPUT_ENDED: &str = r#"{"type":"input_ended"}"#;
const SET_INPUT: &str =
    r#"{"type":"set_view","view":{"type":"input","prompt":"msg: ","value":"abc","cursor":2}}"#;

#[test]
fn the_messages_of_1_1_are_written_as_they_always_were() {
    assert_eq!(
        ToPlugin::action("stage").to_bytes().unwrap(),
        ACTION.as_bytes()
    );
    assert_eq!(
        ToPlugin::text("hi \"there\"").to_bytes().unwrap(),
        TEXT.as_bytes()
    );
    assert_eq!(
        ToPlugin::key("backspace", false, false, false)
            .to_bytes()
            .unwrap(),
        KEY.as_bytes()
    );
    assert_eq!(
        ToPlugin::key("s", true, false, false).to_bytes().unwrap(),
        KEY_WITH_CTRL.as_bytes()
    );
    assert_eq!(
        ToPlugin::InputEnded.to_bytes().unwrap(),
        INPUT_ENDED.as_bytes()
    );
    assert_eq!(
        FromPlugin::set_view(WireView::input("msg: ", "abc", 2))
            .to_bytes()
            .unwrap(),
        SET_INPUT.as_bytes()
    );
}

#[test]
fn the_messages_of_1_1_are_still_read() {
    assert_eq!(
        ToPlugin::from_bytes(ACTION.as_bytes()),
        Ok(ToPlugin::action("stage"))
    );
    assert_eq!(
        ToPlugin::from_bytes(TEXT.as_bytes()),
        Ok(ToPlugin::text("hi \"there\""))
    );
    assert_eq!(
        ToPlugin::from_bytes(KEY.as_bytes()),
        Ok(ToPlugin::key("backspace", false, false, false))
    );
    assert_eq!(
        ToPlugin::from_bytes(INPUT_ENDED.as_bytes()),
        Ok(ToPlugin::InputEnded)
    );
    assert_eq!(
        FromPlugin::from_bytes(SET_INPUT.as_bytes()),
        Ok(FromPlugin::set_view(WireView::input("msg: ", "abc", 2)))
    );
}

#[test]
fn a_key_without_its_modifiers_has_none_held() {
    assert_eq!(
        ToPlugin::from_bytes(br#"{"type":"key","key":"left"}"#),
        Ok(ToPlugin::key("left", false, false, false))
    );
}

#[test]
fn an_input_view_with_fields_left_out_takes_defaults() {
    assert_eq!(
        FromPlugin::from_bytes(br#"{"type":"set_view","view":{"type":"input"}}"#),
        Ok(FromPlugin::set_view(WireView::input("", "", 0)))
    );
}

#[test]
fn an_action_declaration_reads_with_and_without_keys() {
    let with: ActionDecl =
        serde_json::from_str(r#"{"id":"stage","title":"Stage File","keys":"s"}"#).unwrap();
    let without: ActionDecl = serde_json::from_str(r#"{"id":"commit"}"#).unwrap();
    let later: ActionDecl =
        serde_json::from_str(r#"{"id":"x","title":"X","icon":"star","when":"always"}"#).unwrap();

    assert_eq!(with, ActionDecl::new("stage", "Stage File").with_keys("s"));
    assert_eq!(without, ActionDecl::new("commit", ""));
    assert_eq!(
        later,
        ActionDecl::new("x", "X"),
        "what is not known is skipped"
    );
}

// --- ABI 1.2, as written: git ---

const GIT_CHANGED: &str = r#"{"type":"git_changed","summary":{"generation":3,"branch":"main","commit":"abc1234","upstream":"origin/main","ahead":1,"behind":2,"staged":4,"changes":5,"untracked":6,"conflicts":0}}"#;
const GIT_PAGE: &str = r#"{"type":"git_entries","request":7,"generation":3,"section":"changes","offset":0,"total":2,"entries":[{"path":"a.rs","original":null,"kind":"modified"},{"path":"new name.rs","original":"old.rs","kind":"renamed"}]}"#;
const DONE: &str = r#"{"type":"done","request":7}"#;
const GIT_UNAVAILABLE: &str = r#"{"type":"git_unavailable"}"#;
const DENIED: &str =
    r#"{"type":"refused","request":7,"reason":{"type":"denied","capability":"git.read"}}"#;
const NO_REPOSITORY: &str = r#"{"type":"refused","request":7,"reason":{"type":"no_repository"}}"#;
const BAD_ARGUMENT: &str =
    r#"{"type":"refused","request":7,"reason":{"type":"bad_argument","detail":"outside"}}"#;
const FAILED: &str =
    r#"{"type":"refused","request":7,"reason":{"type":"failed","detail":"fatal: x"}}"#;
const ASK_PAGE: &str =
    r#"{"type":"git_entries","request":7,"section":"staged","offset":10,"limit":30}"#;
const STAGE_FILE: &str = r#"{"type":"git_stage","request":8,"path":"src/a.rs"}"#;
const UNSTAGE_FILE: &str = r#"{"type":"git_unstage","request":9,"path":"a.rs"}"#;

fn summary() -> GitSummary {
    let mut summary = GitSummary::empty(3);
    summary.branch = Some("main".into());
    summary.commit = Some("abc1234".into());
    summary.upstream = Some("origin/main".into());
    summary.ahead = 1;
    summary.behind = 2;
    summary.staged = 4;
    summary.changes = 5;
    summary.untracked = 6;
    summary
}

fn page() -> ToPlugin {
    ToPlugin::GitEntries {
        request: 7,
        generation: 3,
        section: GitSection::Changes,
        offset: 0,
        total: 2,
        entries: vec![
            GitEntry::new("a.rs", GitKind::Modified),
            GitEntry::new("new name.rs", GitKind::Renamed).renamed_from("old.rs"),
        ],
    }
}

fn refused(reason: Refusal) -> ToPlugin {
    ToPlugin::Refused { request: 7, reason }
}

fn to_plugin_1_2() -> Vec<(&'static str, ToPlugin)> {
    vec![
        (GIT_CHANGED, ToPlugin::GitChanged { summary: summary() }),
        (GIT_PAGE, page()),
        (DONE, ToPlugin::Done { request: 7 }),
        (GIT_UNAVAILABLE, ToPlugin::GitUnavailable),
        (
            DENIED,
            refused(Refusal::Denied {
                capability: Capability::GitRead,
            }),
        ),
        (NO_REPOSITORY, refused(Refusal::NoRepository)),
        (
            BAD_ARGUMENT,
            refused(Refusal::BadArgument {
                detail: "outside".into(),
            }),
        ),
        (
            FAILED,
            refused(Refusal::Failed {
                detail: "fatal: x".into(),
            }),
        ),
    ]
}

fn from_plugin_1_2() -> Vec<(&'static str, FromPlugin)> {
    vec![
        (
            ASK_PAGE,
            FromPlugin::GitEntries {
                request: 7,
                section: GitSection::Staged,
                offset: 10,
                limit: 30,
            },
        ),
        (
            STAGE_FILE,
            FromPlugin::GitStage {
                request: 8,
                path: "src/a.rs".into(),
            },
        ),
        (
            UNSTAGE_FILE,
            FromPlugin::GitUnstage {
                request: 9,
                path: "a.rs".into(),
            },
        ),
    ]
}

#[test]
fn the_messages_of_1_2_are_written_as_they_always_were() {
    for (json, message) in to_plugin_1_2() {
        assert_eq!(message.to_bytes().unwrap(), json.as_bytes(), "{json}");
    }
    for (json, message) in from_plugin_1_2() {
        assert_eq!(message.to_bytes().unwrap(), json.as_bytes(), "{json}");
    }
}

#[test]
fn the_messages_of_1_2_are_still_read() {
    for (json, message) in to_plugin_1_2() {
        assert_eq!(ToPlugin::from_bytes(json.as_bytes()), Ok(message), "{json}");
    }
    for (json, message) in from_plugin_1_2() {
        assert_eq!(
            FromPlugin::from_bytes(json.as_bytes()),
            Ok(message),
            "{json}"
        );
    }
}

#[test]
fn a_summary_from_an_older_editor_with_fewer_numbers_takes_defaults() {
    let read = ToPlugin::from_bytes(
        br#"{"type":"git_changed","summary":{"generation":9,"branch":"main"}}"#,
    )
    .unwrap();

    let ToPlugin::GitChanged { summary } = read else {
        panic!("{read:?}");
    };
    assert_eq!(summary.generation, 9);
    assert_eq!(summary.branch.as_deref(), Some("main"));
    assert_eq!((summary.staged, summary.changes, summary.ahead), (0, 0, 0));
}

#[test]
fn what_a_later_editor_adds_to_git_is_unknown_not_an_error() {
    let page = ToPlugin::from_bytes(
        br#"{"type":"git_entries","request":1,"generation":2,"section":"stashes","offset":0,"total":1,"entries":[{"path":"x","kind":"ignored","icon":"!"}],"more":true}"#,
    )
    .unwrap();
    let ToPlugin::GitEntries {
        section, entries, ..
    } = page
    else {
        panic!("{page:?}");
    };
    assert_eq!(section, GitSection::Unknown);
    assert_eq!(entries[0].kind, GitKind::Unknown);
    assert_eq!(entries[0].original, None, "left out reads as nothing");

    assert_eq!(
        ToPlugin::from_bytes(
            br#"{"type":"refused","request":1,"reason":{"type":"rate_limited","wait":5}}"#
        ),
        Ok(refused_with(1, Refusal::Unknown))
    );
    assert_eq!(
        ToPlugin::from_bytes(
            br#"{"type":"refused","request":1,"reason":{"type":"denied","capability":"net.fetch"}}"#
        ),
        Ok(refused_with(
            1,
            Refusal::Denied {
                capability: Capability::Unknown
            }
        ))
    );
}

fn refused_with(request: u64, reason: Refusal) -> ToPlugin {
    ToPlugin::Refused { request, reason }
}

#[test]
fn a_request_from_a_later_plugin_for_a_section_this_version_lacks_reads_as_unknown() {
    assert_eq!(
        FromPlugin::from_bytes(
            br#"{"type":"git_entries","request":1,"section":"stashes","limit":10}"#
        ),
        Ok(FromPlugin::GitEntries {
            request: 1,
            section: GitSection::Unknown,
            offset: 0,
            limit: 10
        })
    );
}

#[test]
fn capabilities_are_written_as_their_names() {
    assert_eq!(
        serde_json::to_string(&[Capability::GitRead, Capability::GitWrite]).unwrap(),
        r#"["git.read","git.write"]"#
    );
    assert_eq!(
        serde_json::from_str::<Vec<Capability>>(r#"["git.write","time.travel"]"#).unwrap(),
        vec![Capability::GitWrite, Capability::Unknown]
    );
}

// --- the manifest ---

use crate::Manifest;

#[test]
fn a_manifest_reads_from_toml_with_its_actions() {
    let manifest: Manifest = toml::from_str(
        r#"
id = "git-viewer"
name = "Git Viewer"
version = "0.1.0"
abi = "1.2"
requires = "0.7.0"
wasm = "git_viewer.wasm"
capabilities = ["git.read", "git.write"]

[[action]]
id = "stage"
title = "Stage File"
keys = "s"

[[action]]
id = "refresh"
"#,
    )
    .unwrap();

    assert_eq!(manifest.id, "git-viewer");
    assert_eq!(manifest.display_name(), "Git Viewer");
    assert_eq!(manifest.requires.as_deref(), Some("0.7.0"));
    assert_eq!(manifest.capabilities, ["git.read", "git.write"]);
    assert_eq!(
        manifest.actions,
        vec![
            ActionDecl::new("stage", "Stage File").with_keys("s"),
            ActionDecl::new("refresh", ""),
        ]
    );
}

#[test]
fn a_manifest_needs_only_an_id_a_version_an_abi_and_a_module() {
    let manifest: Manifest =
        toml::from_str("id = \"tiny\"\nversion = \"1.0.0\"\nabi = \"1.0\"\nwasm = \"tiny.wasm\"\n")
            .unwrap();

    assert_eq!(
        manifest.display_name(),
        "tiny",
        "the id when there is no name"
    );
    assert_eq!(manifest.requires, None);
    assert!(manifest.capabilities.is_empty());
    assert!(manifest.actions.is_empty());
}

#[test]
fn a_manifest_with_a_part_this_version_does_not_know_still_reads() {
    let manifest: Result<Manifest, _> = toml::from_str(
        "id = \"x\"\nversion = \"1.0.0\"\nabi = \"1.0\"\nwasm = \"x.wasm\"\nlicense = \"MIT\"\n[ui]\nicon = \"star\"\n",
    );

    assert!(manifest.is_ok());
}

#[test]
fn a_manifest_without_what_it_needs_does_not_read() {
    for text in [
        "version = \"1\"\nabi = \"1.0\"\nwasm = \"x.wasm\"",
        "id = \"x\"\nabi = \"1.0\"\nwasm = \"x.wasm\"",
        "id = \"x\"\nversion = \"1\"\nwasm = \"x.wasm\"",
        "id = \"x\"\nversion = \"1\"\nabi = \"1.0\"",
    ] {
        assert!(toml::from_str::<Manifest>(text).is_err(), "{text}");
    }
}

#[test]
fn capabilities_are_found_by_the_name_a_manifest_writes() {
    assert_eq!(Capability::parse("git.read"), Some(Capability::GitRead));
    assert_eq!(Capability::parse("git.write"), Some(Capability::GitWrite));
    assert_eq!(Capability::parse("net.fetch"), None);
    assert_eq!(Capability::parse("unknown"), None, "not a name to ask for");
    for capability in Capability::ALL {
        assert_eq!(Capability::parse(capability.name()), Some(capability));
    }
}

// --- ABI 1.3, as written: rows, clicks, opening files ---

const SET_ROWS: &str = r#"{"type":"set_view","view":{"type":"rows","lines":[{"text":"Changes","slot":"explorer.dir"},{"text":"","slot":"","spans":[{"text":"M ","slot":"git.modified"},{"text":"a.rs","slot":"explorer.file"}]}],"selected":1}}"#;
const CLICK: &str = r#"{"type":"click","row":3}"#;
const OPEN_FILE: &str = r#"{"type":"open_file","request":9,"path":"src/a.rs"}"#;

fn rows() -> FromPlugin {
    FromPlugin::set_view(WireView::rows(
        [
            WireLine::new("Changes", "explorer.dir"),
            WireLine::of_spans([
                WireSpan::new("M ", "git.modified"),
                WireSpan::new("a.rs", "explorer.file"),
            ]),
        ],
        Some(1),
    ))
}

#[test]
fn the_messages_of_1_3_are_written_as_they_always_were() {
    assert_eq!(rows().to_bytes().unwrap(), SET_ROWS.as_bytes());
    assert_eq!(
        ToPlugin::Click { row: 3 }.to_bytes().unwrap(),
        CLICK.as_bytes()
    );
    assert_eq!(
        FromPlugin::OpenFile {
            request: 9,
            path: "src/a.rs".into()
        }
        .to_bytes()
        .unwrap(),
        OPEN_FILE.as_bytes()
    );
}

#[test]
fn the_messages_of_1_3_are_still_read() {
    assert_eq!(FromPlugin::from_bytes(SET_ROWS.as_bytes()), Ok(rows()));
    assert_eq!(
        ToPlugin::from_bytes(CLICK.as_bytes()),
        Ok(ToPlugin::Click { row: 3 })
    );
    assert_eq!(
        FromPlugin::from_bytes(OPEN_FILE.as_bytes()),
        Ok(FromPlugin::OpenFile {
            request: 9,
            path: "src/a.rs".into()
        })
    );
}

#[test]
fn a_line_without_spans_is_written_the_way_it_was_before_spans_existed() {
    let line = WireLine::new("x", "y");

    assert_eq!(
        serde_json::to_string(&line).unwrap(),
        r#"{"text":"x","slot":"y"}"#
    );
}

#[test]
fn rows_with_fields_left_out_take_defaults() {
    assert_eq!(
        FromPlugin::from_bytes(br#"{"type":"set_view","view":{"type":"rows"}}"#),
        Ok(FromPlugin::set_view(WireView::rows([], None)))
    );
}

#[test]
fn the_capability_to_open_files_has_its_name() {
    assert_eq!(
        Capability::parse("editor.open"),
        Some(Capability::EditorOpen)
    );
    assert_eq!(
        serde_json::to_string(&Capability::EditorOpen).unwrap(),
        r#""editor.open""#
    );
}
