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
    assert_eq!(parse("{}").settings.theme, None);
}

#[test]
fn settings_are_read() {
    assert_eq!(
        parse(r#"{ "theme": "soldark" }"#).settings.theme.as_deref(),
        Some("soldark")
    );
}

#[test]
fn comments_and_trailing_commas_are_allowed() {
    let parsed = parse("{\n  // the theme\n  \"theme\": \"light\", /* inline */\n}\n");

    assert_eq!(parsed.settings.theme.as_deref(), Some("light"));
}

#[test]
fn comment_marks_inside_a_string_stay() {
    let parsed = parse(r#"{ "theme": "a//b /* c */, ]" }"#);

    assert_eq!(parsed.settings.theme.as_deref(), Some("a//b /* c */, ]"));
}

#[test]
fn an_escaped_quote_does_not_end_a_string() {
    let parsed = parse(r#"{ "theme": "a\" // not a comment" }"#);

    assert_eq!(
        parsed.settings.theme.as_deref(),
        Some("a\" // not a comment")
    );
}

#[test]
fn a_comment_may_end_the_file_without_a_line_break() {
    assert_eq!(parse("{} // done").settings, Settings::default());
}

#[test]
fn a_block_comment_that_is_never_closed_runs_to_the_end() {
    let parsed = parse("{ \"theme\": \"x\" } /* never closed");

    assert_eq!(parsed.settings.theme.as_deref(), Some("x"));
    assert_eq!(
        parse("/* never closed { \"theme\": \"x\" }").settings,
        Settings::default()
    );
}

#[test]
fn slash_star_slash_does_not_close_the_comment_it_opens() {
    let parsed = parse("{ /*/ \"theme\": \"x\" */ \"theme\": \"light\" }");

    assert_eq!(parsed.settings.theme.as_deref(), Some("light"));
}

#[test]
fn windows_line_endings_are_fine() {
    let parsed = parse("{\r\n  // the theme\r\n  \"theme\": \"light\",\r\n}\r\n");

    assert_eq!(parsed.settings.theme.as_deref(), Some("light"));
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

    assert_eq!(parsed.settings.theme.as_deref(), Some("light"));
    assert_eq!(parsed.unknown, ["colour", "tab"]);
}

#[test]
fn a_setting_set_twice_is_listed_once_and_the_last_counts() {
    let parsed = parse(r#"{ "theme": "light", "theme": "soldark", "theme": "dark" }"#);

    assert_eq!(parsed.settings.theme.as_deref(), Some("dark"));
    assert_eq!(parsed.repeated, ["theme"]);
}

#[test]
fn an_unknown_setting_set_twice_is_both_unknown_and_repeated() {
    let parsed = parse(r#"{ "colour": 1, "colour": 2 }"#);

    assert_eq!(parsed.unknown, ["colour"]);
    assert_eq!(parsed.repeated, ["colour"]);
}

#[test]
fn the_shell_and_mouse_defaults_are_the_users_shell_and_on() {
    let settings = parse("{}").settings;

    assert_eq!(settings.shell, None);
    assert!(settings.mouse);
}

#[test]
fn a_shell_is_read_and_trimmed() {
    assert_eq!(
        parse(r#"{ "shell": "fish" }"#).settings.shell.as_deref(),
        Some("fish")
    );
    assert_eq!(
        parse(r#"{ "shell": "  /bin/zsh " }"#)
            .settings
            .shell
            .as_deref(),
        Some("/bin/zsh")
    );
}

#[test]
fn a_null_or_empty_shell_means_the_users_own() {
    for source in [
        r#"{ "shell": null }"#,
        r#"{ "shell": "" }"#,
        r#"{ "shell": "   " }"#,
    ] {
        assert_eq!(parse(source).settings.shell, None, "{source}");
    }
}

#[test]
fn a_shell_that_is_not_text_is_rejected_by_name() {
    let message = error_of(r#"{ "shell": 3 }"#);

    assert!(message.contains("\"shell\""), "{message}");
}

#[test]
fn the_mouse_is_read() {
    assert!(!parse(r#"{ "mouse": false }"#).settings.mouse);
    assert!(parse(r#"{ "mouse": true }"#).settings.mouse);
}

#[test]
fn a_mouse_that_is_not_true_or_false_is_rejected_by_name() {
    let message = error_of(r#"{ "mouse": "off" }"#);

    assert!(message.contains("\"mouse\""), "{message}");
}

#[test]
fn the_tab_size_defaults_to_four_and_is_read() {
    assert_eq!(parse("{}").settings.tab_size.get(), 4);
    assert_eq!(parse(r#"{ "tab_size": 8 }"#).settings.tab_size.get(), 8);
    assert_eq!(parse(r#"{ "tab_size": 1 }"#).settings.tab_size.get(), 1);
    assert_eq!(parse(r#"{ "tab_size": 16 }"#).settings.tab_size.get(), 16);
}

#[test]
fn a_tab_size_out_of_range_is_rejected_by_name_with_the_range() {
    for source in [
        r#"{ "tab_size": 0 }"#,
        r#"{ "tab_size": 17 }"#,
        r#"{ "tab_size": -2 }"#,
    ] {
        let message = error_of(source);

        assert!(
            message.contains("\"tab_size\"") && message.contains("between 1 and 16"),
            "{source}: {message}"
        );
    }
}

#[test]
fn a_tab_size_that_is_not_a_whole_number_is_rejected_by_name() {
    for source in [
        r#"{ "tab_size": "wide" }"#,
        r#"{ "tab_size": 2.5 }"#,
        r#"{ "tab_size": null }"#,
    ] {
        let message = error_of(source);

        assert!(message.contains("\"tab_size\""), "{source}: {message}");
    }
}

#[test]
fn the_indent_defaults_are_spaces_with_detection_on() {
    let settings = parse("{}").settings;

    assert_eq!(settings.indent_style, IndentKind::Spaces);
    assert!(settings.detect_indent);
    assert_eq!(settings.indent_prefs(), IndentPrefs::default());
}

#[test]
fn the_indent_settings_are_read() {
    let settings =
        parse(r#"{ "tab_size": 2, "indent_style": "tabs", "detect_indent": false }"#).settings;

    assert_eq!(settings.indent_style, IndentKind::Tabs);
    assert!(!settings.detect_indent);
    assert_eq!(settings.indent_prefs().size.get(), 2);
}

#[test]
fn an_indent_style_that_is_neither_spaces_nor_tabs_is_rejected_by_name() {
    for source in [
        r#"{ "indent_style": "space" }"#,
        r#"{ "indent_style": 4 }"#,
        r#"{ "indent_style": null }"#,
    ] {
        let message = error_of(source);

        assert!(message.contains("\"indent_style\""), "{source}: {message}");
    }
}

#[test]
fn a_detect_indent_that_is_not_true_or_false_is_rejected_by_name() {
    let message = error_of(r#"{ "detect_indent": "yes" }"#);

    assert!(message.contains("\"detect_indent\""), "{message}");
}

// ---- language_servers

#[test]
fn language_servers_default_to_those_of_the_default_file() {
    let settings = parse("{}").settings;

    assert_eq!(
        settings.language_servers,
        Settings::default().language_servers
    );
    assert_eq!(
        settings.language_servers.names().collect::<Vec<_>>(),
        ["javascript", "json", "nix", "rust"]
    );
    assert_eq!(
        settings.language_servers.get("rust").unwrap().command,
        "rust-analyzer"
    );
}

#[test]
fn javascript_has_a_default_server_for_its_files_that_is_not_for_json_or_typescript() {
    let settings = parse("{}").settings;
    let server = settings.language_servers.get("javascript").unwrap();

    assert_eq!(server.command, "typescript-language-server");
    assert_eq!(server.args, ["--stdio"]);
    for extension in ["js", "mjs", "cjs", "jsx"] {
        assert!(
            server.extensions.iter().any(|e| e == extension),
            "{extension}"
        );
    }
    assert!(!server.extensions.iter().any(|e| e == "json" || e == "ts"));
}

#[test]
fn a_language_server_entry_is_read_and_the_default_ones_stay() {
    let settings =
        parse(r#"{ "language_servers": { "zig": { "command": "zls", "extensions": ["zig"] } } }"#)
            .settings;

    assert_eq!(
        settings.language_servers.names().collect::<Vec<_>>(),
        ["javascript", "json", "nix", "rust", "zig"]
    );
}

#[test]
fn a_server_can_be_replaced_or_turned_off_in_the_file() {
    let replaced = parse(
        r#"{ "language_servers": { "rust": { "command": "ra-multiplex", "extensions": ["rs"],
            "initialization_options": { "cargo": { "features": "all" } } } } }"#,
    )
    .settings;
    let off = parse(r#"{ "language_servers": { "rust": null } }"#).settings;

    let rust = replaced.language_servers.get("rust").unwrap();
    assert_eq!(rust.command, "ra-multiplex");
    assert_eq!(
        rust.initialization_options["cargo"]["features"],
        serde_json::json!("all")
    );
    assert_eq!(
        off.language_servers.names().collect::<Vec<_>>(),
        ["javascript", "json", "nix"]
    );
}

#[test]
fn a_bad_language_server_entry_rejects_the_file_and_names_the_setting_and_the_entry() {
    for source in [
        r#"{ "language_servers": [] }"#,
        r#"{ "language_servers": { "rust": 3 } }"#,
        r#"{ "language_servers": { "x": { "command": "x" } } }"#,
        r#"{ "language_servers": { "x": { "command": "x", "extensions": ["x"], "typo": 1 } } }"#,
    ] {
        let message = error_of(source);

        assert!(
            message.starts_with("setting \"language_servers\""),
            "{source}: {message}"
        );
    }
    let message = error_of(r#"{ "language_servers": { "x": { "command": "x" } } }"#);
    assert!(
        message.contains("\"x\"") && message.contains("extensions"),
        "{message}"
    );
}

#[test]
fn the_default_file_is_the_base_layer_of_the_servers() {
    assert!(DEFAULT_SETTINGS_JSONC.contains("\"rust-analyzer\""));
    // A user file that has `{}` for the servers (every file from before) keeps them all.
    let kept = parse(r#"{ "language_servers": {} }"#).settings;
    assert_eq!(kept.language_servers, Settings::default().language_servers);
    assert_eq!(
        parse(DEFAULT_SETTINGS_JSONC).settings.language_servers,
        Settings::default().language_servers
    );
}

#[test]
fn an_entry_for_a_default_server_changes_only_the_fields_it_names() {
    let settings =
        parse(r#"{ "language_servers": { "rust": { "format_on_save": true } } }"#).settings;

    let rust = settings.language_servers.get("rust").unwrap();
    assert!(rust.format_on_save);
    assert_eq!(rust.command, "rust-analyzer");
    assert_eq!(rust.extensions, ["rs"]);
    assert_eq!(rust.root_files, ["Cargo.toml"]);
}

#[test]
fn the_sequence_timeout_defaults_to_one_second_and_is_read_in_milliseconds() {
    use std::time::Duration;

    assert_eq!(
        parse("{}").settings.sequence_timeout,
        Some(Duration::from_millis(1000))
    );
    for (text, wanted) in [("200", 200), ("750", 750), ("5000", 5000)] {
        let settings = parse(&format!(r#"{{ "sequence_timeout": {text} }}"#)).settings;
        assert_eq!(
            settings.sequence_timeout,
            Some(Duration::from_millis(wanted))
        );
    }
}

#[test]
fn a_sequence_timeout_of_zero_waits_for_ever() {
    assert_eq!(
        parse(r#"{ "sequence_timeout": 0 }"#)
            .settings
            .sequence_timeout,
        None
    );
}

#[test]
fn a_sequence_timeout_out_of_range_or_of_the_wrong_type_is_rejected_by_name() {
    for source in [
        r#"{ "sequence_timeout": 199 }"#,
        r#"{ "sequence_timeout": 5001 }"#,
        r#"{ "sequence_timeout": -5 }"#,
        r#"{ "sequence_timeout": "fast" }"#,
        r#"{ "sequence_timeout": 1.5 }"#,
    ] {
        assert!(error_of(source).contains("sequence_timeout"), "{source}");
    }
    assert!(error_of(r#"{ "sequence_timeout": 5001 }"#).contains("200 and 5000"));
}

#[test]
fn the_notification_seconds_default_to_four_and_are_read() {
    use std::time::Duration;

    assert_eq!(
        parse("{}").settings.notification_lifetime,
        Duration::from_secs(4)
    );
    for seconds in [1, 10, 60] {
        let settings = parse(&format!(r#"{{ "notification_seconds": {seconds} }}"#)).settings;
        assert_eq!(settings.notification_lifetime, Duration::from_secs(seconds));
    }
}

#[test]
fn notification_seconds_out_of_range_or_of_the_wrong_type_are_rejected_by_name() {
    for source in [
        r#"{ "notification_seconds": 0 }"#,
        r#"{ "notification_seconds": 61 }"#,
        r#"{ "notification_seconds": -1 }"#,
        r#"{ "notification_seconds": "long" }"#,
    ] {
        assert!(
            error_of(source).contains("notification_seconds"),
            "{source}"
        );
    }
}

#[test]
fn hide_ignored_defaults_to_false_and_is_read() {
    assert!(!parse("{}").settings.hide_ignored);
    assert!(parse(r#"{ "hide_ignored": true }"#).settings.hide_ignored);
    assert!(error_of(r#"{ "hide_ignored": "yes" }"#).contains("hide_ignored"));
}

#[test]
fn format_on_save_belongs_to_a_server_and_is_on_by_default_only_for_rust() {
    let settings = parse("{}").settings;
    for name in settings.language_servers.names() {
        assert_eq!(
            settings.language_servers.get(name).unwrap().format_on_save,
            name == "rust",
            "{name}"
        );
    }
    let off = parse(r#"{ "language_servers": { "rust": { "format_on_save": false } } }"#).settings;
    assert!(!off.language_servers.get("rust").unwrap().format_on_save);

    let on = parse(r#"{ "language_servers": { "nix": { "format_on_save": true } } }"#).settings;
    assert!(on.language_servers.get("nix").unwrap().format_on_save);
    assert!(on.language_servers.get("rust").unwrap().format_on_save);
    assert!(
        error_of(r#"{ "language_servers": { "rust": { "format_on_save": 1 } } }"#).contains("rust")
    );
}

#[test]
fn the_global_format_on_save_is_gone() {
    let parsed = parse(r#"{ "format_on_save": true }"#);

    assert_eq!(parsed.unknown, ["format_on_save"]);
}

#[test]
fn a_null_or_empty_theme_is_the_fallback() {
    for source in [
        r#"{ "theme": null }"#,
        r#"{ "theme": "" }"#,
        r#"{ "theme": "  " }"#,
    ] {
        assert_eq!(parse(source).settings.theme, None, "{source}");
    }
}

#[test]
fn plugin_sources_default_to_none_and_are_cleaned() {
    assert!(parse("{}").settings.plugin_sources.is_empty());
    let settings =
        parse(r#"{ "plugin_sources": [" https://github.com/a/b/raw/main/index.json ", "", "https://github.com/a/b/raw/main/index.json"] }"#)
            .settings;
    assert_eq!(
        settings.plugin_sources,
        ["https://github.com/a/b/raw/main/index.json"]
    );
    assert!(error_of(r#"{ "plugin_sources": "https://x" }"#).contains("plugin_sources"));
}
