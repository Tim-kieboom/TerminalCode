use super::*;

fn parse(source: &str) -> Parsed {
    Settings::parse(source).unwrap()
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
}

#[test]
fn settings_left_out_keep_their_defaults() {
    let parsed = parse(r#"{ "vim": true }"#);

    assert!(parsed.settings.vim);
    assert_eq!(parsed.settings.theme, "dark");
}

#[test]
fn settings_are_read() {
    let parsed = parse(r#"{ "theme": "soldark", "vim": true }"#);

    assert_eq!(parsed.settings.theme, "soldark");
    assert!(parsed.settings.vim);
}

#[test]
fn comments_and_trailing_commas_are_allowed() {
    let parsed =
        parse("{\n  // the theme\n  \"theme\": \"light\", /* inline */\n  \"vim\": true,\n}\n");

    assert_eq!(parsed.settings.theme, "light");
    assert!(parsed.settings.vim);
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
fn stripping_keeps_line_and_column_numbers() {
    let source = "{ // héllo\n  \"theme\": 3, /* x\n y */ }\n";

    let cleaned = strip_jsonc(source);

    assert_eq!(cleaned.lines().count(), source.lines().count());
    assert_eq!(cleaned.chars().count(), source.chars().count());
}

#[test]
fn a_syntax_error_names_the_line() {
    let error = Settings::parse("{\n  \"theme\" \"dark\"\n}").unwrap_err();

    assert!(error.to_string().contains("line 2"), "{error}");
}

#[test]
fn a_wrong_type_names_the_setting() {
    let error = Settings::parse(r#"{ "vim": "yes" }"#).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("\"vim\""), "{message}");
}

#[test]
fn something_other_than_an_object_is_refused() {
    for source in ["[]", "3", "\"dark\""] {
        let error = Settings::parse(source).unwrap_err();
        assert!(matches!(error, SettingsError::NotAnObject), "{source}");
    }
}

#[test]
fn unknown_settings_are_listed_and_the_rest_applies() {
    let parsed = parse(r#"{ "colour": "red", "vim": true, "tab": 4 }"#);

    assert!(parsed.settings.vim);
    assert_eq!(parsed.unknown, ["colour", "tab"]);
}
