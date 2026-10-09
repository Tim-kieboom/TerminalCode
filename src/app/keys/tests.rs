use std::time::Duration;

use super::Keyboard;
use crate::event::action::Action;
use crate::keymap::{Context, Expiry, KeyChord, Keymap, Outcome};

const CONTEXTS: [Context; 2] = [Context::Editor, Context::Global];

fn chord(text: &str) -> KeyChord {
    text.parse().unwrap()
}

fn keyboard() -> Keyboard {
    let keymap = Keymap::from_toml(
        r#"
        [[binding]]
        keys = "ctrl+k"
        action = "save"
        [[binding]]
        keys = "ctrl+k ctrl+s"
        action = "undo"
        [[binding]]
        keys = "ctrl+q"
        action = "quit"
        "#,
    )
    .unwrap();
    Keyboard::new(keymap)
}

#[test]
fn nothing_is_pending_at_first() {
    assert_eq!(keyboard().pending_deadline(), None);
}

#[test]
fn a_half_typed_sequence_waits_until_its_deadline() {
    let mut keyboard = keyboard().with_sequence_timeout(Duration::from_secs(5));
    let before = std::time::Instant::now();

    let resolution = keyboard.feed(&CONTEXTS, chord("ctrl+k"));

    assert!(matches!(resolution.outcome, Outcome::Pending));
    let deadline = keyboard
        .pending_deadline()
        .expect("a deadline while pending");
    assert!(deadline >= before + Duration::from_secs(5));
}

#[test]
fn finishing_the_sequence_stops_the_wait() {
    let mut keyboard = keyboard();
    keyboard.feed(&CONTEXTS, chord("ctrl+k"));

    let resolution = keyboard.feed(&CONTEXTS, chord("ctrl+s"));
    assert!(matches!(resolution.outcome, Outcome::Action(Action::Undo)));
    assert_eq!(keyboard.pending_deadline(), None);
}

#[test]
fn a_chord_that_is_a_binding_by_itself_does_not_wait() {
    let mut keyboard = keyboard();

    let resolution = keyboard.feed(&CONTEXTS, chord("ctrl+q"));

    assert!(matches!(resolution.outcome, Outcome::Action(Action::Quit)));
    assert_eq!(keyboard.pending_deadline(), None);
}

#[test]
fn a_chord_that_breaks_the_sequence_stops_the_wait() {
    let mut keyboard = keyboard();
    keyboard.feed(&CONTEXTS, chord("ctrl+k"));

    let resolution = keyboard.feed(&CONTEXTS, chord("ctrl+q"));

    assert!(matches!(resolution.outcome, Outcome::Action(Action::Quit)));
    assert_eq!(keyboard.pending_deadline(), None);
}

#[test]
fn giving_up_runs_a_sequence_that_is_a_binding_by_itself_and_stops_the_wait() {
    let mut keyboard = keyboard();
    keyboard.feed(&CONTEXTS, chord("ctrl+k"));
    assert!(keyboard.pending_deadline().is_some());

    let expiry = keyboard.expire(&CONTEXTS);

    assert!(matches!(expiry, Expiry::Fire(Action::Save)));
    assert_eq!(keyboard.pending_deadline(), None);
}

#[test]
fn giving_up_with_nothing_pending_does_nothing() {
    let mut keyboard = keyboard();

    assert!(matches!(keyboard.expire(&CONTEXTS), Expiry::Nothing));
}

#[test]
fn the_keys_for_an_action_are_looked_up_in_the_contexts_given() {
    let keyboard = keyboard();

    assert_eq!(
        keyboard.keys_for(&Action::Quit, &CONTEXTS).as_deref(),
        Some("ctrl+q")
    );
    assert_eq!(keyboard.keys_for(&Action::Redo, &CONTEXTS), None);
}
