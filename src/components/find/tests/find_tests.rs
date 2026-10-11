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
    assert_eq!(key(KeyCode::Enter, none), Some(Command::Confirm));
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

// ---- whole words

fn word_options() -> Options {
    Options {
        whole_word: true,
        ..Options::default()
    }
}

fn find_in(text: &str, query: &str, options: Options) -> Find {
    Find::open(
        &Buffer::from_text(text),
        Position::default(),
        query.to_owned(),
        options,
    )
}

#[test]
fn whole_word_leaves_out_matches_inside_longer_words() {
    let find = find_in("cat concat cat_x cat. (cat)", "cat", word_options());

    assert_eq!(find.matches(), [at(0, 0, 3), at(0, 17, 20), at(0, 23, 26)]);
}

#[test]
fn without_whole_word_every_occurrence_counts() {
    let find = find_in("cat concat cat_x", "cat", Options::default());

    assert_eq!(find.matches().len(), 3);
}

#[test]
fn whole_word_works_for_a_query_that_starts_or_ends_in_punctuation() {
    // Like ripgrep's -w: the character before the match must not be a word
    // character, even when the match itself starts with a paren.
    let find = find_in("call(foo) (foo) recall(foo)", "(foo)", word_options());
    assert_eq!(find.matches(), [at(0, 10, 15)]);

    let find = find_in("x foo() foox()", "foo(", word_options());
    assert_eq!(find.matches(), [at(0, 2, 6)]);
}

#[test]
fn whole_word_with_a_pattern_wraps_the_whole_pattern() {
    let options = Options {
        regex: true,
        whole_word: true,
        ..Options::default()
    };

    let find = find_in("ab abc a abcd", "ab|abc", options);

    assert_eq!(find.matches(), [at(0, 0, 2), at(0, 3, 6)]);
}

#[test]
fn whole_word_is_toggled_and_matches_are_found_again() {
    let buffer = Buffer::from_text("cat concat");
    let mut find = Find::open(
        &buffer,
        Position::default(),
        "cat".into(),
        Options::default(),
    );
    assert_eq!(find.matches().len(), 2);

    find.toggle_word(&buffer);
    assert_eq!(find.matches().len(), 1);
    assert!(find.options().whole_word);

    find.toggle_word(&buffer);
    assert_eq!(find.matches().len(), 2);
}

#[test]
fn whole_word_counts_words_with_non_ascii_letters() {
    let find = find_in("été étés", "été", word_options());

    assert_eq!(find.matches(), [at(0, 0, 3)]);
}

#[test]
fn alt_w_toggles_whole_word() {
    assert_eq!(
        key(KeyCode::Char('w'), KeyModifiers::ALT),
        Some(Command::ToggleWord)
    );
}

// ---- replacing

fn with_replacement(
    text: &str,
    query: &str,
    replacement: &str,
    options: Options,
) -> (Buffer, Find) {
    let buffer = Buffer::from_text(text);
    let mut find = Find::open(&buffer, Position::default(), query.to_owned(), options);
    find.set_replacement(replacement.to_owned());
    (buffer, find)
}

fn texts(replacements: &[crate::components::editor::Replacement]) -> Vec<&str> {
    replacements.iter().map(|r| r.text.as_str()).collect()
}

#[test]
fn a_literal_replacement_is_taken_as_it_is() {
    let (buffer, find) = with_replacement("a.c a.c", "a.c", "$1\\n", Options::default());

    assert_eq!(texts(&find.all_replacements(&buffer)), ["$1\\n", "$1\\n"]);
}

#[test]
fn replacements_know_where_they_go() {
    let (buffer, find) = with_replacement("one two\ntwo", "two", "2", Options::default());

    let all = find.all_replacements(&buffer);

    assert_eq!(all.len(), 2);
    assert_eq!(
        (all[0].start, all[0].end),
        (Position::new(0, 4), Position::new(0, 7))
    );
    assert_eq!(
        (all[1].start, all[1].end),
        (Position::new(1, 0), Position::new(1, 3))
    );
}

#[test]
fn the_current_replacement_is_the_one_for_the_current_match() {
    let (buffer, mut find) = with_replacement("one two\ntwo", "two", "2", Options::default());
    find.next();

    let current = find.current_replacement(&buffer).unwrap();

    assert_eq!(current.start, Position::new(1, 0));
    assert_eq!(current.text, "2");
}

#[test]
fn with_regex_the_groups_of_a_match_can_be_used() {
    let options = Options {
        regex: true,
        ..Options::default()
    };
    let (buffer, find) = with_replacement(
        "john smith\nada lovelace",
        r"(\w+) (\w+)",
        "$2, $1",
        options,
    );

    assert_eq!(
        texts(&find.all_replacements(&buffer)),
        ["smith, john", "lovelace, ada"]
    );
}

#[test]
fn named_groups_and_a_missing_group_work() {
    let options = Options {
        regex: true,
        ..Options::default()
    };
    let (buffer, find) = with_replacement("k=v", r"(?P<key>\w)=(\w)", "${key}:$2:[$3]", options);

    assert_eq!(texts(&find.all_replacements(&buffer)), ["k:v:[]"]);
}

#[test]
fn matches_in_a_line_with_several_are_each_replaced_in_place() {
    let (buffer, find) = with_replacement("aXa aXa", "X", "--", Options::default());

    let all = find.all_replacements(&buffer);

    assert_eq!(all.len(), 2);
    assert_eq!(all[1].start, Position::new(0, 5));
}

#[test]
fn graphemes_are_counted_not_bytes() {
    let (buffer, find) = with_replacement("é é", "é", "e", Options::default());

    let all = find.all_replacements(&buffer);

    assert_eq!(all[1].start, Position::new(0, 2));
    assert_eq!(all[1].end, Position::new(0, 3));
}

#[test]
fn with_no_match_there_is_nothing_to_replace() {
    let (buffer, find) = with_replacement("abc", "zzz", "x", Options::default());

    assert!(find.all_replacements(&buffer).is_empty());
    assert!(find.current_replacement(&buffer).is_none());
}

#[test]
fn an_invalid_pattern_has_nothing_to_replace() {
    let options = Options {
        regex: true,
        ..Options::default()
    };
    let (buffer, find) = with_replacement("abc", "(", "x", options);

    assert!(find.all_replacements(&buffer).is_empty());
}

#[test]
fn the_replacement_row_is_shown_hidden_and_switched_to() {
    let mut find = find("two");
    assert!(!find.replace_open());
    assert_eq!(find.bar_rows(), 1);

    find.toggle_replace();
    assert!(find.replace_open());
    assert!(find.replacing(), "the row gets the typing");
    assert_eq!(find.bar_rows(), 2);

    find.switch_field();
    assert!(!find.replacing());
    find.switch_field();
    assert!(find.replacing());

    find.toggle_replace();
    assert!(!find.replace_open());
    assert!(!find.replacing());
}

#[test]
fn switching_fields_does_nothing_with_the_row_hidden() {
    let mut find = find("two");

    find.switch_field();

    assert!(!find.replacing());
}

#[test]
fn typing_goes_to_the_field_that_has_it() {
    let buffer = buffer();
    let mut find = find("two");
    find.toggle_replace();

    find.insert(&buffer, 'X');
    find.insert(&buffer, 'Y');

    assert_eq!(find.replacement(), "XY");
    assert_eq!(find.query(), "two");
    find.backspace(&buffer);
    assert_eq!(find.replacement(), "X");
    find.clear_query(&buffer);
    assert_eq!(find.replacement(), "");
    assert_eq!(find.query(), "two");
    assert_eq!(find.matches().len(), 4, "the matches did not change");

    find.switch_field();
    find.insert(&buffer, 's');
    assert_eq!(find.query(), "twos");
}

#[test]
fn the_keys_for_replacing() {
    assert_eq!(
        key(KeyCode::Char('r'), KeyModifiers::CONTROL),
        Some(Command::ToggleReplace)
    );
    assert_eq!(
        key(KeyCode::Tab, KeyModifiers::NONE),
        Some(Command::SwitchField)
    );
    assert_eq!(
        key(KeyCode::BackTab, KeyModifiers::SHIFT),
        Some(Command::SwitchField)
    );
    assert_eq!(
        key(KeyCode::Char('a'), KeyModifiers::ALT),
        Some(Command::ReplaceAll)
    );
}
