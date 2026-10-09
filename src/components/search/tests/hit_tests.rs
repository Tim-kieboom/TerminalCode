use std::path::PathBuf;

use crate::components::search::Hit;

fn hit(line: &str, needle: &str) -> Hit {
    let start = line.find(needle).unwrap();
    Hit::new("a.txt".into(), 3, line, start..start + needle.len())
}

fn shown(hit: &Hit) -> &str {
    &hit.text[hit.matched.clone()]
}

#[test]
fn a_hit_knows_its_file_line_and_column() {
    let hit = hit("let value = 1;", "value");

    assert_eq!(hit.path, PathBuf::from("a.txt"));
    assert_eq!(hit.line, 2, "lines are 0-based, the sink counts from 1");
    assert_eq!(hit.column, 4);
    assert_eq!(shown(&hit), "value");
}

#[test]
fn the_indent_is_not_shown_but_the_column_still_counts_it() {
    let hit = hit("        needle here", "needle");

    assert_eq!(hit.text, "needle here");
    assert_eq!(hit.column, 8);
    assert_eq!(shown(&hit), "needle");
}

#[test]
fn a_match_inside_the_indent_keeps_the_indent_so_it_can_be_seen() {
    let hit = hit("    x", "  ");

    assert_eq!(hit.text, "    x");
    assert_eq!(shown(&hit), "  ");
}

#[test]
fn the_column_counts_graphemes_not_bytes() {
    // A flag is two characters and one grapheme; é is two bytes.
    let hit = hit("🇳🇱 é needle", "needle");

    assert_eq!(hit.column, 4);
    assert_eq!(shown(&hit), "needle");
}

#[test]
fn line_breaks_are_not_part_of_the_text() {
    let crlf = hit("needle\r\n", "needle");
    let lf = hit("needle\n", "needle");

    assert_eq!(crlf.text, "needle");
    assert_eq!(lf.text, "needle");
}

#[test]
fn a_match_far_into_a_line_is_shown_with_the_start_cut_off() {
    let line = format!("{}needle after", "x".repeat(200));

    let hit = hit(&line, "needle");

    assert!(hit.text.starts_with('…'), "{}", hit.text);
    assert_eq!(shown(&hit), "needle");
    assert_eq!(hit.column, 200, "the cursor still goes to the real place");
    assert!(hit.text.chars().count() < 100);
}

#[test]
fn a_very_long_line_is_cut_off_at_the_end() {
    let line = format!("needle{}", "y".repeat(1000));

    let hit = hit(&line, "needle");

    assert!(hit.text.ends_with('…'), "{}", hit.text);
    assert!(hit.text.chars().count() <= 241);
    assert_eq!(shown(&hit), "needle");
}

#[test]
fn a_match_beyond_the_cut_is_clamped_not_a_panic() {
    let line = format!("{}needle", "y".repeat(10));
    let start = 5000;

    // A match range that lies past what is shown.
    let hit = Hit::new("a.txt".into(), 1, &line, start..start + 6);

    assert!(hit.matched.end <= hit.text.len());
    assert!(hit.matched.start <= hit.matched.end);
}

#[test]
fn multibyte_text_around_the_match_is_never_split() {
    let line = format!("{}needle{}", "é".repeat(100), "ü".repeat(300));

    let hit = hit(&line, "needle");

    assert!(hit.text.is_char_boundary(hit.matched.start));
    assert!(hit.text.is_char_boundary(hit.matched.end));
    assert_eq!(shown(&hit), "needle");
}
