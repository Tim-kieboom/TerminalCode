use super::*;

fn parse(source: &str) -> Parsed {
    match Settings::parse(source) {
        Ok(parsed) => parsed,
        Err(error) => panic!("{source:?} should parse: {error}"),
    }
}

fn error_of(source: &str) -> String {
    match Settings::parse(source) {
        Ok(parsed) => panic!("{source:?} should not parse, got {parsed:?}"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn an_empty_file_is_all_defaults() {
    for source in ["", "  \n", "// only a comment\n", "/* and another */"] {
        assert_eq!(parse(source).settings, Settings::default(), "{source:?}");
    }
}

#[test]
fn the_default_file_says_what_the_defaults_are() {
    let parsed = parse(DEFAULT_SETTINGS_JSONC);

    assert_eq!(parsed.settings, Settings::default());
    assert!(parsed.unknown.is_empty());
    assert!(parsed.repeated.is_empty());
}

#[test]
fn a_setting_left_out_keeps_its_default() {
    assert_eq!(parse("{}").settings.theme, "dark");
}

#[test]
fn settings_are_read() {
    assert_eq!(parse(r#"{ "theme": "soldark" }"#).settings.theme, "soldark");
}

#[test]
fn comments_and_trailing_commas_are_allowed() {
    let parsed = parse("{\n  // the theme\n  \"theme\": \"light\", /* inline */\n}\n");

    assert_eq!(parsed.settings.theme, "light");
}

#[test]
fn comment_marks_inside_a_string_stay() {
    let parsed = parse(r#"{ "theme": "a//b /* c */, ]" }"#);

    assert_eq!(parsed.settings.theme, "a//b /* c */, ]");
}

#[test]
fn an_escaped_quote_does_not_end_a_string() {
    let parsed = parse(r#"{ "theme": "a\" // not a comment" }"#);

    assert_eq!(parsed.settings.theme, "a\" // not a comment");
}

#[test]
fn a_comment_may_end_the_file_without_a_line_break() {
    assert_eq!(parse("{} // done").settings, Settings::default());
}

#[test]
fn a_block_comment_that_is_never_closed_runs_to_the_end() {
    let parsed = parse("{ \"theme\": \"x\" } /* never closed");

    assert_eq!(parsed.settings.theme, "x");
    assert_eq!(
        parse("/* never closed { \"theme\": \"x\" }").settings,
        Settings::default()
    );
}

#[test]
fn slash_star_slash_does_not_close_the_comment_it_opens() {
    let parsed = parse("{ /*/ \"theme\": \"x\" */ \"theme\": \"light\" }");

    assert_eq!(parsed.settings.theme, "light");
}

#[test]
fn windows_line_endings_are_fine() {
    let parsed = parse("{\r\n  // the theme\r\n  \"theme\": \"light\",\r\n}\r\n");

    assert_eq!(parsed.settings.theme, "light");
}

#[test]
fn stripping_keeps_line_and_column_numbers() {
    let source = "{ // héllo\n  \"theme\": 3, /* x\n y */ }\n";

    let cleaned = strip_jsonc(source);

    assert_eq!(cleaned.lines().count(), source.lines().count());
    assert_eq!(cleaned.chars().count(), source.chars().count());
}

#[test]
fn a_syntax_error_names_the_line() {
    let message = error_of("{\n  \"theme\" \"dark\"\n}");

    assert!(message.contains("line 2"), "{message}");
}

#[test]
fn a_wrong_type_names_the_setting() {
    let message = error_of(r#"{ "theme": 3 }"#);

    assert!(message.contains("\"theme\""), "{message}");
}

#[test]
fn something_other_than_an_object_is_refused() {
    for source in ["[]", "3", "\"dark\"", "null"] {
        let message = error_of(source);
        assert!(message.contains("object"), "{source}: {message}");
    }
}

#[test]
fn unknown_settings_are_listed_and_the_rest_applies() {
    let parsed = parse(r#"{ "colour": "red", "theme": "light", "tab": 4 }"#);

    assert_eq!(parsed.settings.theme, "light");
    assert_eq!(parsed.unknown, ["colour", "tab"]);
}

#[test]
fn a_setting_set_twice_is_listed_once_and_the_last_counts() {
    let parsed = parse(r#"{ "theme": "light", "theme": "soldark", "theme": "dark" }"#);

    assert_eq!(parsed.settings.theme, "dark");
    assert_eq!(parsed.repeated, ["theme"]);
}

#[test]
fn an_unknown_setting_set_twice_is_both_unknown_and_repeated() {
    let parsed = parse(r#"{ "colour": 1, "colour": 2 }"#);

    assert_eq!(parsed.unknown, ["colour"]);
    assert_eq!(parsed.repeated, ["colour"]);
}
