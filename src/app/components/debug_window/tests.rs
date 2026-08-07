use std::path::PathBuf;

use super::{DebugMessage, DebugTag, DebugWindow, StartupArgs};
use crate::keybinds::Action;

#[test]
fn format_space_matches_tag() {
    assert_eq!(
        DebugMessage::new("x".into(), DebugTag::Note).format_space(),
        "   "
    );
    assert_eq!(
        DebugMessage::new("x".into(), DebugTag::Error).format_space(),
        "  "
    );
    assert_eq!(
        DebugMessage::new("x".into(), DebugTag::Warning).format_space(),
        ""
    );
}

#[test]
fn message_round_trips_content_and_tag() {
    let message = DebugMessage::new("hello".into(), DebugTag::Error);
    assert_eq!(message.as_str(), "hello");
    assert!(matches!(message.tag(), DebugTag::Error));
}

#[test]
fn date_time_string_is_hh_mm_ss() {
    let message = DebugMessage::new("x".into(), DebugTag::Note);
    let string = message.date_time_string();

    assert_eq!(string.len(), 8);
    for (i, ch) in string.chars().enumerate() {
        if i == 2 || i == 5 {
            assert_eq!(ch, ':');
        } else {
            assert!(ch.is_ascii_digit(), "unexpected char {ch:?} in {string}");
        }
    }
}

#[test]
fn move_cursor_clamps_to_message_count() {
    let args = StartupArgs::new(PathBuf::from("."));
    let mut window = DebugWindow::new(&args);
    window.push_note("one".into());
    window.push_note("two".into());
    window.move_cursor(Action::ScrollDown);
    window.move_cursor(Action::ScrollDown);
    assert_eq!(window.scroller.cursor().vertical, 1);
}
