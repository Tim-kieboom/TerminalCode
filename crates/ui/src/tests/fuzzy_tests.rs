use crate::fuzzy::score;

#[test]
fn an_empty_query_matches_everything() {
    assert_eq!(score("", "File: Save"), Some(0));
    assert_eq!(score("  ", "File: Save"), Some(0));
}

#[test]
fn every_query_character_must_appear_in_order() {
    assert!(score("sav", "File: Save").is_some());
    assert!(score("fsv", "File: Save").is_some());
    assert_eq!(score("vas", "File: Save"), None);
    assert_eq!(score("saz", "File: Save"), None);
}

#[test]
fn case_and_whitespace_in_the_query_are_ignored() {
    assert_eq!(score("SAVE", "File: Save"), score("save", "File: Save"));
    assert_eq!(
        score("file save", "File: Save"),
        score("filesave", "File: Save")
    );
}

#[test]
fn a_consecutive_match_beats_a_scattered_one() {
    let tight = score("sav", "File: Save").unwrap();
    let loose = score("sav", "Select: Paste Move").unwrap();
    assert!(tight > loose, "{tight} <= {loose}");
}

#[test]
fn word_starts_beat_the_middle_of_a_word() {
    let initials = score("fn", "File: New").unwrap();
    let middle = score("ie", "File: New").unwrap();
    assert!(initials > middle, "{initials} <= {middle}");
}

#[test]
fn an_earlier_match_beats_a_later_one() {
    let early = score("tab", "Tab: Close").unwrap();
    let late = score("tab", "Application: Toggle Table").unwrap();
    assert!(early > late, "{early} <= {late}");
}

// ---- positions

use crate::fuzzy::Query;

#[test]
fn positions_are_where_the_query_matched() {
    let query = Query::new("fsv");
    assert_eq!(query.positions("File: Save"), Some(vec![0, 6, 8]));
}

#[test]
fn positions_are_the_ones_score_counts_greedily() {
    // `a` takes the first `a`, `b` the first `b` after it.
    assert_eq!(Query::new("ab").positions("a_a_b_b"), Some(vec![0, 4]));
}

#[test]
fn positions_ignore_case_and_whitespace_in_the_query() {
    assert_eq!(Query::new("S a").positions("sAve"), Some(vec![0, 1]));
}

#[test]
fn positions_are_counted_in_characters_not_bytes() {
    assert_eq!(Query::new("b").positions("é-b"), Some(vec![2]));
}

#[test]
fn positions_of_a_query_that_does_not_match_are_none() {
    assert_eq!(Query::new("xyz").positions("File: Save"), None);
    assert_eq!(Query::new("vas").positions("File: Save"), None);
}

#[test]
fn an_empty_query_has_no_positions_but_matches() {
    assert_eq!(Query::new("").positions("anything"), Some(vec![]));
}

#[test]
fn positions_agree_with_score_on_whether_there_is_a_match() {
    for (query, text) in [
        ("sav", "File: Save"),
        ("zzz", "File: Save"),
        ("", "x"),
        ("x", ""),
    ] {
        assert_eq!(
            Query::new(query).positions(text).is_some(),
            Query::new(query).score(text).is_some(),
            "{query:?} in {text:?}"
        );
    }
}
