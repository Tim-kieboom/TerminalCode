use crate::event::action::Action;
use crate::keymap::{Context, Expiry, KeyChord, Keymap, Outcome, Resolver};

const CONTEXTS: [Context; 1] = [Context::Global];

fn chord(source: &str) -> KeyChord {
    source.parse().unwrap()
}

fn keymap() -> Keymap {
    Keymap::from_toml(
        r#"
        [[binding]]
        keys = "ctrl+q"
        action = "quit"

        [[binding]]
        keys = "ctrl+k ctrl+s"
        action = "save"

        [[binding]]
        keys = "ctrl+g"
        action = "undo"

        [[binding]]
        keys = "ctrl+g ctrl+g"
        action = "redo"
        "#,
    )
    .unwrap()
}

fn feed(resolver: &mut Resolver, keymap: &Keymap, key: &str) -> (Vec<KeyChord>, Outcome) {
    let resolution = resolver.feed(keymap, &CONTEXTS, chord(key));
    (resolution.discarded, resolution.outcome)
}

#[test]
fn single_chord_binding_resolves_immediately() {
    let (keymap, mut resolver) = (keymap(), Resolver::default());

    let (discarded, outcome) = feed(&mut resolver, &keymap, "ctrl+q");

    assert!(discarded.is_empty());
    assert_eq!(outcome, Outcome::Action(Action::Quit));
    assert!(!resolver.is_pending());
}

#[test]
fn unbound_chord_is_reported_back() {
    let (keymap, mut resolver) = (keymap(), Resolver::default());

    let (discarded, outcome) = feed(&mut resolver, &keymap, "a");

    assert!(discarded.is_empty());
    assert_eq!(outcome, Outcome::Unbound(chord("a")));
    assert!(!resolver.is_pending());
}

#[test]
fn a_sequence_is_pending_until_its_last_chord() {
    let (keymap, mut resolver) = (keymap(), Resolver::default());

    let (_, first) = feed(&mut resolver, &keymap, "ctrl+k");
    assert_eq!(first, Outcome::Pending);
    assert!(resolver.is_pending());

    let (_, second) = feed(&mut resolver, &keymap, "ctrl+s");
    assert_eq!(second, Outcome::Action(Action::Save));
    assert!(!resolver.is_pending());
}

#[test]
fn a_broken_sequence_discards_its_prefix_and_retries_the_new_chord() {
    let (keymap, mut resolver) = (keymap(), Resolver::default());
    feed(&mut resolver, &keymap, "ctrl+k");

    let (discarded, outcome) = feed(&mut resolver, &keymap, "ctrl+q");

    assert_eq!(discarded, [chord("ctrl+k")]);
    assert_eq!(outcome, Outcome::Action(Action::Quit));
    assert!(!resolver.is_pending());
}

#[test]
fn a_broken_sequence_can_start_another_sequence() {
    let (keymap, mut resolver) = (keymap(), Resolver::default());
    feed(&mut resolver, &keymap, "ctrl+k");

    let (discarded, outcome) = feed(&mut resolver, &keymap, "ctrl+g");

    assert_eq!(discarded, [chord("ctrl+k")]);
    assert_eq!(outcome, Outcome::Pending);
}

#[test]
fn a_broken_sequence_followed_by_an_unbound_chord_reports_both() {
    let (keymap, mut resolver) = (keymap(), Resolver::default());
    feed(&mut resolver, &keymap, "ctrl+k");

    let (discarded, outcome) = feed(&mut resolver, &keymap, "x");

    assert_eq!(discarded, [chord("ctrl+k")]);
    assert_eq!(outcome, Outcome::Unbound(chord("x")));
}

#[test]
fn a_prefix_that_is_also_a_binding_waits_then_completes() {
    let (keymap, mut resolver) = (keymap(), Resolver::default());

    let (_, first) = feed(&mut resolver, &keymap, "ctrl+g");
    let (_, second) = feed(&mut resolver, &keymap, "ctrl+g");

    assert_eq!(first, Outcome::Pending);
    assert_eq!(second, Outcome::Action(Action::Redo));
}

#[test]
fn expiring_an_ambiguous_prefix_fires_its_own_binding() {
    let (keymap, mut resolver) = (keymap(), Resolver::default());
    feed(&mut resolver, &keymap, "ctrl+g");

    let expiry = resolver.expire(&keymap, &CONTEXTS);

    assert_eq!(expiry, Expiry::Fire(Action::Undo));
    assert!(!resolver.is_pending());
}

#[test]
fn expiring_a_pure_prefix_discards_it() {
    let (keymap, mut resolver) = (keymap(), Resolver::default());
    feed(&mut resolver, &keymap, "ctrl+k");

    let expiry = resolver.expire(&keymap, &CONTEXTS);

    assert_eq!(expiry, Expiry::Discard(vec![chord("ctrl+k")]));
    assert!(!resolver.is_pending());
}

#[test]
fn expiring_with_nothing_pending_does_nothing() {
    let (keymap, mut resolver) = (keymap(), Resolver::default());

    assert_eq!(resolver.expire(&keymap, &CONTEXTS), Expiry::Nothing);
}

#[test]
fn three_chord_sequences_work() {
    let keymap = Keymap::from_toml(
        r#"
        [[binding]]
        keys = "ctrl+a ctrl+b ctrl+c"
        action = "quit"
        "#,
    )
    .unwrap();
    let mut resolver = Resolver::default();

    let outcomes: Vec<_> = ["ctrl+a", "ctrl+b", "ctrl+c"]
        .into_iter()
        .map(|key| feed(&mut resolver, &keymap, key).1)
        .collect();

    assert_eq!(
        outcomes,
        [
            Outcome::Pending,
            Outcome::Pending,
            Outcome::Action(Action::Quit)
        ]
    );
}
