//! The shapes of the messages are a promise to plugins already built, so the
//! messages of ABI 1.0 are written out here, byte for byte. A change that makes
//! one of these fail breaks plugins in the field: it needs a new major version,
//! not a change to the test.

use crate::{
    ActionDecl, Capability, FromPlugin, GitEntry, GitKind, GitSection, GitSummary, MAX_MESSAGE,
    MessageError, Refusal, ToPlugin, WireButton, WireLine, WireSpan, WireView,
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

// --- ABI 1.4, as written: buttons, columns, an input that can be idle ---

const SET_COLUMN: &str = r#"{"type":"set_view","view":{"type":"column","parts":[{"type":"buttons","buttons":[{"id":"commit","label":"Commit"},{"id":"push","label":"Push","enabled":false}]},{"type":"rows","lines":[{"text":"a.rs","slot":"explorer.file"}],"selected":0},{"type":"input","prompt":"> ","value":"msg","cursor":3,"active":false}]}}"#;
const PRESS: &str = r#"{"type":"press","button":"commit"}"#;
const INPUT_CLICK: &str = r#"{"type":"input_click","column":2}"#;

fn column() -> FromPlugin {
    FromPlugin::set_view(WireView::column([
        WireView::buttons([
            WireButton::new("commit", "Commit"),
            WireButton::new("push", "Push").enabled(false),
        ]),
        WireView::rows([WireLine::new("a.rs", "explorer.file")], Some(0)),
        WireView::idle_input("> ", "msg", 3),
    ]))
}

#[test]
fn the_messages_of_1_4_are_written_as_pinned() {
    assert_eq!(column().to_bytes().unwrap(), SET_COLUMN.as_bytes());
    assert_eq!(
        ToPlugin::Press {
            button: "commit".into()
        }
        .to_bytes()
        .unwrap(),
        PRESS.as_bytes()
    );
    assert_eq!(
        ToPlugin::InputClick { column: 2 }.to_bytes().unwrap(),
        INPUT_CLICK.as_bytes()
    );
}

#[test]
fn the_messages_of_1_4_are_read() {
    assert_eq!(FromPlugin::from_bytes(SET_COLUMN.as_bytes()), Ok(column()));
    assert_eq!(
        ToPlugin::from_bytes(PRESS.as_bytes()),
        Ok(ToPlugin::Press {
            button: "commit".into()
        })
    );
    assert_eq!(
        ToPlugin::from_bytes(INPUT_CLICK.as_bytes()),
        Ok(ToPlugin::InputClick { column: 2 })
    );
}

#[test]
fn an_input_is_active_unless_it_says_it_is_not_and_is_written_as_it_was() {
    let input = FromPlugin::set_view(WireView::input("> ", "x", 1));
    // The 1.3 shape: no `active` written for a line that is typed into.
    assert_eq!(
        input.to_bytes().unwrap(),
        br#"{"type":"set_view","view":{"type":"input","prompt":"> ","value":"x","cursor":1}}"#
    );
    assert_eq!(
        FromPlugin::from_bytes(
            br#"{"type":"set_view","view":{"type":"input","prompt":"> ","value":"x","cursor":1}}"#
        ),
        Ok(input)
    );
}

#[test]
fn a_button_that_leaves_out_what_it_may_is_enabled_and_empty() {
    assert_eq!(
        FromPlugin::from_bytes(
            br#"{"type":"set_view","view":{"type":"buttons","buttons":[{"id":"a"}]}}"#
        ),
        Ok(FromPlugin::set_view(WireView::buttons([WireButton::new(
            "a", ""
        )])))
    );
}

#[test]
fn an_older_editor_reads_a_column_as_an_unknown_view_not_an_error() {
    // What 1.3 does with a view of a later version: `Unknown` (the `other` arm).
    assert_eq!(
        FromPlugin::from_bytes(br#"{"type":"set_view","view":{"type":"hologram"}}"#),
        Ok(FromPlugin::set_view(WireView::Unknown))
    );
}

const STAGE_ALL: &str = r#"{"type":"git_stage_all","request":1}"#;
const UNSTAGE_ALL: &str = r#"{"type":"git_unstage_all","request":2}"#;
const COMMIT: &str = r#"{"type":"git_commit","request":3,"message":"Fix it"}"#;
const NOTHING_STAGED: &str = r#"{"type":"refused","request":3,"reason":{"type":"nothing_staged"}}"#;

#[test]
fn the_requests_to_stage_all_unstage_all_and_commit_are_written_as_pinned() {
    for (message, text) in [
        (FromPlugin::GitStageAll { request: 1 }, STAGE_ALL),
        (FromPlugin::GitUnstageAll { request: 2 }, UNSTAGE_ALL),
        (
            FromPlugin::GitCommit {
                request: 3,
                message: "Fix it".into(),
            },
            COMMIT,
        ),
    ] {
        assert_eq!(message.to_bytes().unwrap(), text.as_bytes());
        assert_eq!(FromPlugin::from_bytes(text.as_bytes()), Ok(message));
    }
}

#[test]
fn nothing_staged_is_a_refusal_of_its_own() {
    let refused = ToPlugin::Refused {
        request: 3,
        reason: Refusal::NothingStaged,
    };
    assert_eq!(refused.to_bytes().unwrap(), NOTHING_STAGED.as_bytes());
    assert_eq!(ToPlugin::from_bytes(NOTHING_STAGED.as_bytes()), Ok(refused));
}

const FETCH: &str = r#"{"type":"git_fetch","request":4}"#;
const PULL: &str = r#"{"type":"git_pull","request":5}"#;
const PUSH: &str = r#"{"type":"git_push","request":6}"#;
const CANCEL: &str = r#"{"type":"git_cancel","request":7}"#;

#[test]
fn the_remote_requests_are_written_as_pinned() {
    for (message, text) in [
        (FromPlugin::GitFetch { request: 4 }, FETCH),
        (FromPlugin::GitPull { request: 5 }, PULL),
        (FromPlugin::GitPush { request: 6 }, PUSH),
        (FromPlugin::GitCancel { request: 7 }, CANCEL),
    ] {
        assert_eq!(message.to_bytes().unwrap(), text.as_bytes());
        assert_eq!(FromPlugin::from_bytes(text.as_bytes()), Ok(message));
    }
}

#[test]
fn the_new_refusals_are_written_as_pinned() {
    for (reason, text) in [
        (Refusal::Busy, r#"{"type":"busy"}"#),
        (Refusal::Cancelled, r#"{"type":"cancelled"}"#),
        (
            Refusal::AuthRequired {
                detail: "terminal prompts disabled".into(),
            },
            r#"{"type":"auth_required","detail":"terminal prompts disabled"}"#,
        ),
    ] {
        assert_eq!(serde_json::to_string(&reason).unwrap(), text);
        assert_eq!(serde_json::from_str::<Refusal>(text).unwrap(), reason);
    }
}

#[test]
fn the_capability_for_the_remote_has_its_name_and_is_listed() {
    assert_eq!(Capability::parse("git.remote"), Some(Capability::GitRemote));
    assert_eq!(
        serde_json::to_string(&Capability::GitRemote).unwrap(),
        r#""git.remote""#
    );
    assert!(Capability::ALL.contains(&Capability::GitRemote));
    // The other three keep their place, so what a manifest lists is shown in the same order.
    assert_eq!(
        &Capability::ALL[..3],
        &[
            Capability::GitRead,
            Capability::GitWrite,
            Capability::EditorOpen
        ]
    );
}

// --- plugin API 1.5: the text of the file in front ---

#[test]
fn the_text_of_a_file_is_a_message_with_its_golden_json() {
    let text = ToPlugin::FileText {
        path: "docs/a.md".to_owned(),
        text: "# Hi\n".to_owned(),
        modified: true,
    };
    let json = r##"{"type":"file_text","path":"docs/a.md","text":"# Hi\n","modified":true}"##;
    assert_eq!(text.to_bytes().unwrap(), json.as_bytes());
    assert_eq!(ToPlugin::from_bytes(json.as_bytes()), Ok(text));

    // `modified` may be left out: the file is as it was saved.
    let plain = ToPlugin::from_bytes(br##"{"type":"file_text","path":"a.md","text":""}"##).unwrap();
    assert_eq!(
        plain,
        ToPlugin::FileText {
            path: "a.md".to_owned(),
            text: String::new(),
            modified: false
        }
    );
}

#[test]
fn a_file_cleared_and_a_file_too_large_have_golden_json() {
    assert_eq!(
        ToPlugin::FileCleared.to_bytes().unwrap(),
        br#"{"type":"file_cleared"}"#
    );
    let large = ToPlugin::FileTooLarge {
        path: "big.md".to_owned(),
        bytes: 2_000_000,
    };
    let json = r#"{"type":"file_too_large","path":"big.md","bytes":2000000}"#;
    assert_eq!(large.to_bytes().unwrap(), json.as_bytes());
    assert_eq!(ToPlugin::from_bytes(json.as_bytes()), Ok(large));
}

#[test]
fn the_capability_of_file_text_and_the_manifest_field() {
    assert_eq!(Capability::parse("files.text"), Some(Capability::FilesText));
    assert_eq!(
        serde_json::to_string(&Capability::FilesText).unwrap(),
        r#""files.text""#
    );
    assert!(Capability::ALL.contains(&Capability::FilesText));

    let manifest: crate::Manifest = toml_like(
        r#"{"id":"markdown","version":"0.1.0","abi":"1.5","wasm":"m.wasm","capabilities":["files.text"],"file_types":["md","markdown"]}"#,
    );
    assert_eq!(manifest.file_types, ["md", "markdown"]);
    let without: crate::Manifest =
        toml_like(r#"{"id":"x","version":"0.1.0","abi":"1.5","wasm":"m.wasm"}"#);
    assert!(without.file_types.is_empty());
}

fn toml_like<T: serde::de::DeserializeOwned>(json: &str) -> T {
    serde_json::from_str(json).unwrap()
}

#[test]
fn a_whole_file_of_text_fits_in_one_message() {
    use crate::MAX_FILE_TEXT;
    // The worst case of a line break written as two bytes.
    let text = "\n".repeat(MAX_FILE_TEXT);
    let message = ToPlugin::FileText {
        path: "a.md".to_owned(),
        text,
        modified: false,
    };
    assert!(message.to_bytes().unwrap().len() <= MAX_MESSAGE);
}
