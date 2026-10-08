use crate::ui::fuzzy::score;

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
