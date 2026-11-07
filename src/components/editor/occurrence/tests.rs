use super::next_match;

#[test]
fn the_next_match_is_the_first_after_the_end_of_the_selection() {
    let text = "foo bar foo baz foo";

    assert_eq!(next_match(text, "foo", 3, &[0..3]), Some(8..11));
    assert_eq!(next_match(text, "foo", 11, &[0..3, 8..11]), Some(16..19));
}

#[test]
fn it_goes_round_to_the_start_and_skips_what_is_selected() {
    let text = "foo bar foo";

    assert_eq!(next_match(text, "foo", 11, &[8..11]), Some(0..3));
    assert_eq!(
        next_match(text, "foo", 11, &[0..3, 8..11]),
        None,
        "all of them are selected"
    );
}

#[test]
fn a_needle_that_is_a_word_only_matches_whole_words() {
    let text = "foo foobar barfoo foo";

    assert_eq!(next_match(text, "foo", 3, &[0..3]), Some(18..21));
}

#[test]
fn text_that_is_not_a_word_matches_inside_words_too() {
    let text = "a.b a.bc xa.b";

    assert_eq!(next_match(text, "a.b", 3, &[0..3]), Some(4..7));
    assert_eq!(next_match(text, "a.b", 7, &[0..3, 4..7]), Some(10..13));
}

#[test]
fn nothing_matches_an_empty_needle() {
    assert_eq!(next_match("abc", "", 0, &[]), None);
}

#[test]
fn a_match_that_runs_into_a_selection_is_not_a_new_one() {
    let text = "aaaa";

    // "aa" selected at 1..3: the match at 2 would overlap it, and the next non-overlapping
    // one in the scan is at 0 or 2; 0..2 overlaps too.
    assert_eq!(next_match(text, "aa", 3, &[1..3]), None);
}
