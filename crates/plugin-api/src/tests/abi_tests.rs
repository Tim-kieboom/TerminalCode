use crate::{Abi, AbiError};

#[test]
fn the_version_of_these_messages_is_1_0() {
    assert_eq!(Abi::CURRENT, Abi::new(1, 0));
    assert_eq!(Abi::CURRENT.to_string(), "1.0");
}

#[test]
fn the_same_version_and_an_earlier_minor_are_accepted() {
    let host = Abi::new(1, 3);

    assert_eq!(host.check(Abi::new(1, 3)), Ok(()));
    assert_eq!(host.check(Abi::new(1, 0)), Ok(()));
}

#[test]
fn a_later_minor_is_refused_because_it_may_rely_on_what_this_editor_lacks() {
    let host = Abi::new(1, 3);

    assert_eq!(
        host.check(Abi::new(1, 4)),
        Err(AbiError::TooNew {
            plugin: Abi::new(1, 4),
            host
        })
    );
}

#[test]
fn a_newer_major_is_refused_with_a_message_to_update_the_editor() {
    let error = Abi::new(1, 3).check(Abi::new(2, 0)).unwrap_err();

    assert!(matches!(error, AbiError::TooNew { .. }));
    assert_eq!(
        error.to_string(),
        "it needs plugin API 2.0, and this version of TerminalCode has 1.3: update TerminalCode"
    );
}

#[test]
fn an_older_major_is_refused_with_a_message_to_ask_the_author() {
    let error = Abi::new(2, 0).check(Abi::new(1, 9)).unwrap_err();

    assert!(matches!(error, AbiError::TooOld { .. }));
    assert!(error.to_string().contains("ask its author for an update"));
}

#[test]
fn a_version_is_read_from_text() {
    assert_eq!("1.0".parse(), Ok(Abi::new(1, 0)));
    assert_eq!("2.15".parse(), Ok(Abi::new(2, 15)));
    assert_eq!("3".parse(), Ok(Abi::new(3, 0)), "a bare major is its .0");
}

#[test]
fn text_that_is_not_a_version_is_an_error_naming_it() {
    for text in ["", "x", "1.", ".1", "1.2.3", "-1.0", "1.x", "v1"] {
        assert_eq!(
            text.parse::<Abi>(),
            Err(AbiError::Unreadable(text.to_owned())),
            "{text:?}"
        );
    }
}

#[test]
fn versions_are_ordered_by_major_then_minor() {
    assert!(Abi::new(1, 9) < Abi::new(2, 0));
    assert!(Abi::new(1, 1) < Abi::new(1, 2));
}
