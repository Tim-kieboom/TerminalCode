use crate::{PatternError, RunPattern, allows, parse_all};

fn pattern(text: &str) -> RunPattern {
    RunPattern::parse(text).unwrap()
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn words_that_are_not_stars_must_be_equal_and_in_number() {
    let pattern = pattern("cargo nextest list");

    assert!(pattern.matches("cargo", &args(&["nextest", "list"])));
    assert!(!pattern.matches("cargo", &args(&["nextest"])));
    assert!(!pattern.matches("cargo", &args(&["nextest", "run"])));
    assert!(!pattern.matches("cargo", &args(&["nextest", "list", "extra"])));
    assert!(!pattern.matches("rustc", &args(&["nextest", "list"])));
}

#[test]
fn a_last_star_is_any_arguments_none_included() {
    let pattern = pattern("cargo nextest *");

    assert!(pattern.matches("cargo", &args(&["nextest"])));
    assert!(pattern.matches("cargo", &args(&["nextest", "run", "-E", "test(=a)"])));
    assert!(!pattern.matches("cargo", &args(&["install", "nextest"])));
    assert!(!pattern.matches("cargo", &[]));
}

#[test]
fn a_star_among_words_is_exactly_one_argument() {
    let pattern = pattern("cargo llvm-cov * --junit-path *");

    assert!(pattern.matches("cargo", &args(&["llvm-cov", "nextest", "--junit-path"])));
    assert!(pattern.matches(
        "cargo",
        &args(&["llvm-cov", "nextest", "--junit-path", "a.xml"])
    ));
    assert!(!pattern.matches("cargo", &args(&["llvm-cov", "--junit-path"])));
}

#[test]
fn a_program_alone_allows_it_with_no_arguments_and_a_star_allows_all() {
    assert!(pattern("pytest").matches("pytest", &[]));
    assert!(!pattern("pytest").matches("pytest", &args(&["-x"])));
    assert!(pattern("pytest *").matches("pytest", &args(&["-x", "tests/"])));
}

#[test]
fn an_argument_with_a_shell_character_is_only_ever_itself() {
    let pattern = pattern("cargo nextest list");

    assert!(!pattern.matches("cargo", &args(&["nextest", "list; rm -rf /"])));
    assert!(!pattern.matches("cargo", &args(&["nextest", "list", "&&", "sh"])));
}

#[test]
fn a_program_that_is_a_path_or_has_a_star_or_is_an_option_is_not_a_pattern() {
    for text in [
        "./cargo build",
        "/usr/bin/cargo",
        "..\\cargo",
        "ca*go",
        "-x run",
    ] {
        assert!(
            matches!(RunPattern::parse(text), Err(PatternError::BadProgram(_))),
            "{text}"
        );
    }
}

#[test]
fn nothing_and_a_star_that_is_part_of_a_word_are_not_patterns() {
    assert_eq!(RunPattern::parse("   "), Err(PatternError::Empty));
    assert_eq!(
        RunPattern::parse("pytest --junitxml=*"),
        Err(PatternError::PartialWildcard("--junitxml=*".to_owned()))
    );
}

#[test]
fn a_pattern_is_shown_as_it_is_written_with_the_spaces_made_even() {
    assert_eq!(pattern("cargo   nextest  *").as_str(), "cargo nextest *");
    assert_eq!(pattern("cargo * x *").as_str(), "cargo * x *");
    assert_eq!(pattern("pytest").as_str(), "pytest");
}

#[test]
fn a_list_allows_what_any_of_its_patterns_allows_and_an_empty_list_nothing() {
    let patterns = parse_all(&["cargo nextest *".to_owned(), "git status".to_owned()]).unwrap();

    assert!(allows(&patterns, "git", &args(&["status"])));
    assert!(allows(&patterns, "cargo", &args(&["nextest", "run"])));
    assert!(!allows(&patterns, "git", &args(&["push"])));
    assert!(!allows(&[], "git", &args(&["status"])));
}

#[test]
fn a_list_with_a_bad_pattern_is_not_read() {
    assert!(parse_all(&["cargo *".to_owned(), "./x".to_owned()]).is_err());
}
