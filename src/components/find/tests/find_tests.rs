use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::buffer::{Buffer, Position};
use crate::components::find::{Command, Find, MAX_MATCHES, Match};
use crate::components::search::Options;

const TEXT: &str = "one two\nTwo three two\nnothing\nfour TWO";

fn buffer() -> Buffer {
    Buffer::from_text(TEXT)
}

fn find(query: &str) -> Find {
    Find::open(
        &buffer(),
        Position::default(),
        query.to_owned(),
        Options::default(),
    )
}

fn at(line: usize, start: usize, end: usize) -> Match {
    Match { line, start, end }
}

#[test]
fn matches_are_found_line_by_line_ignoring_case_by_default() {
    let find = find("two");

    assert_eq!(
        find.matches(),
        [at(0, 4, 7), at(1, 0, 3), at(1, 10, 13), at(3, 5, 8)]
    );
}

#[test]
fn match_case_narrows_the_matches() {
    let mut find = find("two");
    find.toggle_case(&buffer());

    assert_eq!(find.matches(), [at(0, 4, 7), at(1, 10, 13)]);
}

#[test]
fn a_literal_query_takes_regex_characters_literally() {
    let buffer = Buffer::from_text("a.c abc\nf(x)");

    let dot = Find::open(
        &buffer,
        Position::default(),
        "a.c".into(),
        Options::default(),
    );
    let paren = Find::open(
        &buffer,
        Position::default(),
        "f(".into(),
        Options::default(),
    );

    assert_eq!(dot.matches(), [at(0, 0, 3)]);
    assert_eq!(paren.matches(), [at(1, 0, 2)]);
}

#[test]
fn with_regex_on_the_query_is_a_pattern() {
    let buffer = Buffer::from_text("a.c abc\nxyz");
    let options = Options {
        regex: true,
        ..Options::default()
    };

    let find = Find::open(&buffer, Position::default(), "a.c".into(), options);

    assert_eq!(find.matches(), [at(0, 0, 3), at(0, 4, 7)]);
}

#[test]
fn an_invalid_pattern_is_a_problem_with_no_matches() {
    let buffer = buffer();
    let options = Options {
        regex: true,
        ..Options::default()
    };

    let find = Find::open(&buffer, Position::default(), "(two".into(), options);

    assert!(find.matches().is_empty());
    assert!(find.current().is_none());
    let reason = find.problem().unwrap();
    assert!(!reason.is_empty() && !reason.contains('\n'), "{reason:?}");
}

#[test]
fn an_empty_query_has_no_matches_and_no_problem() {
    let find = find("");

    assert!(find.matches().is_empty());
    assert_eq!(find.problem(), None);
}

#[test]
fn columns_count_graphemes_not_bytes() {
    let buffer = Buffer::from_text("🇳🇱 é needle");

    let find = Find::open(
        &buffer,
        Position::default(),
        "needle".into(),
        Options::default(),
    );

    assert_eq!(find.matches(), [at(0, 4, 10)]);
}

#[test]
fn matches_that_would_be_empty_are_skipped() {
    let buffer = buffer();
    let options = Options {
        regex: true,
        ..Options::default()
    };

    let find = Find::open(&buffer, Position::default(), "x*".into(), options);

    assert!(find.matches().is_empty());
}

#[test]
fn the_current_match_is_the_first_at_or_after_the_cursor() {
    let buffer = buffer();

    let from_start = Find::open(
        &buffer,
        Position::new(0, 0),
        "two".into(),
        Options::default(),
    );
    let on_a_match = Find::open(
        &buffer,
        Position::new(1, 0),
        "two".into(),
        Options::default(),
    );
    let after_one = Find::open(
        &buffer,
        Position::new(1, 1),
        "two".into(),
        Options::default(),
    );

    assert_eq!(from_start.current(), Some(at(0, 4, 7)));
    assert_eq!(on_a_match.current(), Some(at(1, 0, 3)));
    assert_eq!(after_one.current(), Some(at(1, 10, 13)));
}

#[test]
fn past_the_last_match_the_search_wraps_to_the_first() {
    let buffer = buffer();

    let find = Find::open(
        &buffer,
        Position::new(3, 7),
        "two".into(),
        Options::default(),
    );

    assert_eq!(find.current(), Some(at(0, 4, 7)));
}

#[test]
fn next_and_previous_step_and_wrap() {
    let mut find = find("two");
    assert_eq!(find.current_index(), Some(0));

    find.next();
    find.next();
    find.next();
    assert_eq!(find.current_index(), Some(3));
    find.next();
    assert_eq!(find.current_index(), Some(0), "wraps forward");
    find.previous();
    assert_eq!(find.current_index(), Some(3), "wraps backward");
}

#[test]
fn stepping_with_no_matches_does_nothing() {
    let mut find = find("zzz");

    find.next();
    find.previous();

    assert_eq!(find.current(), None);
}

#[test]
fn typing_more_stays_on_the_match_that_was_stepped_to() {
    let buffer = buffer();
    let mut find = Find::open(&buffer, Position::default(), "t".into(), Options::default());
    find.next();
    let stepped_to = find.current().unwrap();
    assert_eq!(stepped_to.start_position(), Position::new(1, 0));

    find.insert(&buffer, 'w');

    assert_eq!(
        find.current().unwrap().start_position(),
        stepped_to.start_position()
    );
}

#[test]
fn backspace_and_clear_find_again() {
    let buffer = buffer();
    let mut find = Find::open(
        &buffer,
        Position::default(),
        "twox".into(),
        Options::default(),
    );
    assert!(find.matches().is_empty());

    find.backspace(&buffer);
    assert_eq!(find.matches().len(), 4);

    find.clear_query(&buffer);
    assert!(find.matches().is_empty());
    assert_eq!(find.query(), "");
}

#[test]
fn matches_on_a_line_are_found_for_drawing() {
    let find = find("two");

    assert_eq!(find.matches_on_line(0), [at(0, 4, 7)]);
    assert_eq!(find.matches_on_line(1), [at(1, 0, 3), at(1, 10, 13)]);
    assert!(find.matches_on_line(2).is_empty());
    assert_eq!(find.matches_on_line(3), [at(3, 5, 8)]);
    assert!(find.matches_on_line(99).is_empty());
}

#[test]
fn a_changed_buffer_is_searched_again_on_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "two two\n").unwrap();
    let mut buffer = Buffer::open(&path).unwrap();
    let mut find = Find::open(
        &buffer,
        Position::default(),
        "two".into(),
        Options::default(),
    );
    assert_eq!(find.matches().len(), 2);

    std::fs::write(&path, "one two\n").unwrap();
    buffer.reload().unwrap();
    find.refresh(&buffer);

    assert_eq!(find.matches(), [at(0, 4, 7)]);
}

#[test]
fn a_huge_number_of_matches_is_capped_and_says_so() {
    let buffer = Buffer::from_text(&"e\n".repeat(MAX_MATCHES + 500));

    let find = Find::open(&buffer, Position::default(), "e".into(), Options::default());

    assert_eq!(find.matches().len(), MAX_MATCHES);
}

#[test]
fn a_selected_word_seeds_the_query_but_not_a_blank_or_multiline_selection() {
    let buffer = buffer();

    let word = Find::seed(&buffer, Position::new(0, 4), Position::new(0, 7));
    let multi = Find::seed(&buffer, Position::new(0, 0), Position::new(1, 3));
    let none = Find::seed(&buffer, Position::new(0, 2), Position::new(0, 2));
    let blank = Find::seed(&buffer, Position::new(0, 3), Position::new(0, 4));

    assert_eq!(word.as_deref(), Some("two"));
    assert_eq!(multi, None);
    assert_eq!(none, None);
    assert_eq!(blank, None, "a space is not worth searching for");
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> Option<Command> {
    Command::from_key(KeyEvent::new(code, modifiers))
}

#[test]
fn keys_map_to_commands() {
    let none = KeyModifiers::NONE;
    assert_eq!(key(KeyCode::Char('x'), none), Some(Command::Insert('x')));
    assert_eq!(key(KeyCode::Enter, none), Some(Command::Next));
    assert_eq!(
        key(KeyCode::Enter, KeyModifiers::SHIFT),
        Some(Command::Previous)
    );
    assert_eq!(key(KeyCode::Down, none), Some(Command::Next));
    assert_eq!(key(KeyCode::Up, none), Some(Command::Previous));
    assert_eq!(key(KeyCode::F(3), none), Some(Command::Next));
    assert_eq!(
        key(KeyCode::F(3), KeyModifiers::SHIFT),
        Some(Command::Previous)
    );
    assert_eq!(key(KeyCode::Esc, none), Some(Command::Close));
    assert_eq!(key(KeyCode::Backspace, none), Some(Command::Backspace));
    assert_eq!(
        key(KeyCode::Char('u'), KeyModifiers::CONTROL),
        Some(Command::ClearQuery)
    );
    assert_eq!(
        key(KeyCode::Char('c'), KeyModifiers::ALT),
        Some(Command::ToggleCase)
    );
    assert_eq!(
        key(KeyCode::Char('r'), KeyModifiers::ALT),
        Some(Command::ToggleRegex)
    );
}

#[test]
fn other_modified_keys_and_releases_are_not_commands() {
    assert_eq!(key(KeyCode::Char('c'), KeyModifiers::CONTROL), None);
    assert_eq!(key(KeyCode::Char('x'), KeyModifiers::ALT), None);
    let mut release = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    assert_eq!(Command::from_key(release), None);
}
