//! The shapes of the messages are a promise to plugins already built, so the
//! messages of ABI 1.0 are written out here, byte for byte. A change that makes
//! one of these fail breaks plugins in the field: it needs a new major version,
//! not a change to the test.

use crate::{FromPlugin, MAX_MESSAGE, MessageError, ToPlugin, WireLine, WireView};

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
