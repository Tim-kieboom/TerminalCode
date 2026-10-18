use std::fs;

use super::example::TABLE;
use super::*;
use crate::persist::saved_state::SavedState;

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

/// A file with every setting but `format_on_save`, as a 0.6.1 user has it.
const OLD_FILE: &str = "// my settings\n{\n    // my theme\n    \"theme\": null,\n    \"mouse\": false, // no mouse for me\n    \"tab_size\": 2\n}\n";

fn inserted(source: &str, known: &str) -> String {
    match plan(TABLE, source, DEFAULT_SETTINGS_JSONC, v(known)).unwrap() {
        Plan::Insert { text, .. } => text,
        Plan::Current => panic!("nothing was added to {source:?}"),
    }
}

// ---- versions

#[test]
fn versions_are_read_and_compared_by_their_numbers() {
    assert_eq!(Version::parse("0.6.1"), Some(Version(0, 6, 1)));
    assert_eq!(Version::parse("0.10.0"), Some(Version(0, 10, 0)));
    assert_eq!(Version::parse("0.7.0-dev"), Some(Version(0, 7, 0)));
    assert!(v("0.10.0") > v("0.9.9"));
    assert_eq!(Version::parse("0.6"), None);
    assert_eq!(Version::parse("a.b.c"), None);
    assert_eq!(Version::parse("1.2.3.4"), None);
    assert_eq!(v("0.7.0").to_string(), "0.7.0");
}

// ---- what is added

#[test]
fn a_missing_setting_is_added_with_its_comments_and_default_value() {
    let text = inserted(OLD_FILE, "0.6.1");

    assert!(text.contains("\"example_setting\": 1,"), "{text}");
    assert!(
        text.contains("// an example of a setting that a new version added"),
        "{text}"
    );
    assert!(Settings::parse(&text).is_ok());
    assert!(
        keys_of(&text)
            .unwrap()
            .contains(&"example_setting".to_owned())
    );
}

#[test]
fn the_users_own_text_is_untouched() {
    let text = inserted(OLD_FILE, "0.6.1");

    // Everything of the user's file is still there, in the same order, with its comments.
    let mut at = 0;
    for part in [
        "// my settings\n{\n    // my theme\n    \"theme\": null,\n",
        "    \"mouse\": false, // no mouse for me\n",
        "    \"tab_size\": 2",
    ] {
        let found = text[at..]
            .find(part)
            .unwrap_or_else(|| panic!("{part:?} in {text}"));
        at += found + part.len();
    }
    assert!(text.ends_with("}\n"));
}

#[test]
fn a_comma_after_the_last_entry_is_added_when_it_has_none_and_not_when_it_has_one() {
    let without = inserted("{\n    \"tab_size\": 2\n}\n", "0.6.1");
    assert!(without.contains("\"tab_size\": 2,\n"), "{without}");

    let with = inserted("{\n    \"tab_size\": 2,\n}\n", "0.6.1");
    assert!(
        with.contains("\"tab_size\": 2,\n") && !with.contains(",,"),
        "{with}"
    );
    assert!(Settings::parse(&with).is_ok() && Settings::parse(&without).is_ok());
}

#[test]
fn a_file_on_one_line_gets_its_closing_brace_on_a_line_of_its_own() {
    let text = inserted("{ \"tab_size\": 2 }", "0.6.1");

    assert!(text.starts_with("{ \"tab_size\": 2,\n"), "{text}");
    assert!(
        text.trim_end().ends_with("\"example_setting\": 1,\n}"),
        "{text}"
    );
    assert!(Settings::parse(&text).is_ok());
}

#[test]
fn an_empty_object_gets_the_entries_inside() {
    for source in ["{}", "{ }", "{\n}\n", "// nothing yet\n{}\n"] {
        let text = inserted(source, "0.6.1");
        assert!(text.contains("\"example_setting\": 1,"), "{text}");
        assert!(
            keys_of(&text)
                .unwrap()
                .contains(&"example_setting".to_owned())
        );
    }
}

#[test]
fn text_with_accents_in_a_comment_does_not_shift_the_insertion() {
    let source = "{\n    // thé théme, zoals ik het wil: 漢字\n    \"tab_size\": 2\n}\n";

    let text = inserted(source, "0.6.1");

    assert!(
        text.contains("// thé théme, zoals ik het wil: 漢字\n    \"tab_size\": 2,\n"),
        "{text}"
    );
}

// ---- which settings are new

#[test]
fn a_setting_older_than_the_stored_version_stays_deleted() {
    // `hide_ignored` is not in the file, and was introduced before what the file has seen.
    let source = "{\n    \"example_setting\": 5\n}\n";

    assert_eq!(
        plan(TABLE, source, DEFAULT_SETTINGS_JSONC, v("0.6.1")).unwrap(),
        Plan::Current
    );
}

#[test]
fn a_stored_version_at_or_above_the_newest_adds_nothing() {
    for known in ["0.7.0", "0.9.0", "9.9.9"] {
        assert_eq!(
            plan(TABLE, OLD_FILE, DEFAULT_SETTINGS_JSONC, v(known)).unwrap(),
            Plan::Current,
            "{known}"
        );
    }
}

// ---- the file on disk and the remembered version

fn file_with(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, text).unwrap();
    (dir, path)
}

#[test]
fn a_file_with_no_stored_version_counts_as_0_6_1_and_gets_only_what_came_after() {
    // It has neither `hide_ignored` (older) nor `format_on_save` (newer).
    let (_dir, path) = file_with("{\n    \"tab_size\": 2\n}\n");
    let mut saved = SavedState::default();

    let report = update_file_in(TABLE, &path, &mut saved, false);

    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"example_setting\": 1"), "{text}");
    assert!(!text.contains("hide_ignored"), "{text}");
    assert_eq!(saved.settings_version(), Some("0.7.0"));
    assert!(report.remember && report.warning.is_none());
    assert_eq!(
        report.info.as_deref(),
        Some("added new settings to settings.json: example_setting")
    );
}

#[test]
fn the_second_pass_adds_nothing_and_changes_nothing() {
    let (_dir, path) = file_with(OLD_FILE);
    let mut saved = SavedState::default();
    update_file_in(TABLE, &path, &mut saved, false);
    let after_first = fs::read_to_string(&path).unwrap();

    let report = update_file_in(TABLE, &path, &mut saved, false);

    assert_eq!(fs::read_to_string(&path).unwrap(), after_first);
    assert!(!report.remember && report.info.is_none() && report.warning.is_none());
}

#[test]
fn a_setting_the_user_deletes_afterwards_stays_deleted() {
    let (_dir, path) = file_with(OLD_FILE);
    let mut saved = SavedState::default();
    update_file_in(TABLE, &path, &mut saved, false);
    fs::write(&path, OLD_FILE).unwrap();

    let report = update_file_in(TABLE, &path, &mut saved, false);

    assert_eq!(fs::read_to_string(&path).unwrap(), OLD_FILE);
    assert!(report.info.is_none());
}

#[test]
fn a_file_that_does_not_exist_is_left_for_the_default_to_make() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let mut saved = SavedState::default();

    let report = update_file_in(TABLE, &path, &mut saved, false);

    assert!(!path.exists());
    assert!(!report.remember && report.info.is_none() && report.warning.is_none());
    assert_eq!(saved.settings_version(), None);
}

#[test]
fn a_file_that_has_everything_only_remembers_the_version() {
    let (_dir, path) = file_with("{\n    \"example_setting\": 5\n}\n");
    let mut saved = SavedState::default();

    let report = update_file_in(TABLE, &path, &mut saved, false);

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "{\n    \"example_setting\": 5\n}\n"
    );
    assert_eq!(saved.settings_version(), Some("0.7.0"));
    assert!(report.remember && report.info.is_none());
}

#[test]
fn a_newer_stored_version_is_not_lowered() {
    let (_dir, path) = file_with(OLD_FILE);
    let mut saved = SavedState::default();
    saved.set_settings_version("9.0.0");

    let report = update_file_in(TABLE, &path, &mut saved, false);

    assert_eq!(fs::read_to_string(&path).unwrap(), OLD_FILE);
    assert_eq!(saved.settings_version(), Some("9.0.0"));
    assert!(!report.remember);
}

// ---- when it refuses: nothing is written and the version is not advanced

fn refused(text: &str, unsaved: bool) -> (String, Report, SavedState) {
    let (_dir, path) = file_with(text);
    let mut saved = SavedState::default();
    let report = update_file_in(TABLE, &path, &mut saved, unsaved);
    (fs::read_to_string(&path).unwrap(), report, saved)
}

fn assert_refused(text: &str, unsaved: bool, why: &str) {
    let (after, report, saved) = refused(text, unsaved);
    assert_eq!(after, text, "nothing was written");
    assert_eq!(
        saved.settings_version(),
        None,
        "the version was not advanced"
    );
    assert!(!report.remember && report.info.is_none());
    let warning = report.warning.expect("a warning").to_string();
    assert_eq!(
        warning,
        format!(
            "settings.json was not updated with the new settings ({why}); the defaults are used"
        )
    );
}

#[test]
fn a_file_that_does_not_parse_is_left_alone() {
    assert_refused("{ \"tab_size\": }", false, "the file does not parse");
    assert_refused("{ \"mouse\": \"yes\" }", false, "the file does not parse");
    assert_refused("[1, 2]", false, "the file does not parse");
}

#[test]
fn a_comment_after_the_last_entry_is_left_alone() {
    assert_refused(
        "{\n    \"tab_size\": 2\n    // \"mouse\": false\n}\n",
        false,
        "a comment follows its last entry",
    );
    assert_refused(
        "{\n    \"tab_size\": 2, /* more later */ }\n",
        false,
        "a comment follows its last entry",
    );
}

#[test]
fn a_file_open_in_a_tab_with_unsaved_changes_is_left_alone() {
    assert_refused(OLD_FILE, true, "it is open in a tab with unsaved changes");
}

#[cfg(unix)]
#[test]
fn a_file_that_cannot_be_written_is_left_alone() {
    let (_dir, path) = file_with(OLD_FILE);
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let mut saved = SavedState::default();

    let report = update_file_in(TABLE, &path, &mut saved, false);

    assert_eq!(fs::read_to_string(&path).unwrap(), OLD_FILE);
    assert_eq!(saved.settings_version(), None);
    let warning = report.warning.expect("a warning").to_string();
    assert!(warning.contains("cannot be written"), "{warning}");
}

#[test]
fn a_refusal_is_tried_again_once_the_problem_is_gone() {
    let (_dir, path) = file_with(OLD_FILE);
    let mut saved = SavedState::default();
    assert!(
        update_file_in(TABLE, &path, &mut saved, true)
            .warning
            .is_some()
    );

    let report = update_file_in(TABLE, &path, &mut saved, false);

    assert!(report.warning.is_none() && report.info.is_some());
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("\"example_setting\": 1")
    );
}

// ---- the table and the default file

#[test]
fn every_setting_of_the_default_file_is_in_the_table_and_the_other_way_round() {
    let in_file = keys_of(DEFAULT_SETTINGS_JSONC).unwrap();
    for key in &in_file {
        assert!(
            INTRODUCED.iter().any(|setting| setting.key == key),
            "\"{key}\" is in defaults/settings.jsonc but not in INTRODUCED: it would never reach the files that exist"
        );
    }
    for setting in INTRODUCED {
        assert!(
            in_file.iter().any(|key| key == setting.key),
            "\"{}\" is in INTRODUCED but not in defaults/settings.jsonc",
            setting.key
        );
    }
}

#[test]
fn every_version_of_the_table_is_one_and_every_new_setting_has_text_to_add() {
    let baseline = v(BASELINE);
    for setting in INTRODUCED {
        let since = Version::parse(setting.since)
            .unwrap_or_else(|| panic!("\"{}\": {} is not a version", setting.key, setting.since));
        if since > baseline {
            let block = match setting.text {
                Some(text) => Some(text.to_owned()),
                None => default_block(DEFAULT_SETTINGS_JSONC, setting.key),
            };
            let block = block.unwrap_or_else(|| {
                panic!(
                    "\"{}\" has no block in the default file (one line ending in a comma, the comments above it); give it `text` in INTRODUCED",
                    setting.key
                )
            });
            assert!(block.contains(&format!("\"{}\":", setting.key)));
        }
    }
}

#[test]
fn the_block_of_a_setting_is_its_comments_and_its_line() {
    let defaults = "{\n    // one\n    // two\n    \"a\": 1,\n\n    // three\n    \"b\": 2,\n    \"c\": {\n        \"a\": 9,\n    }\n}\n";

    assert_eq!(
        default_block(defaults, "a").as_deref(),
        Some("    // one\n    // two\n    \"a\": 1,\n")
    );
    assert_eq!(
        default_block(defaults, "b").as_deref(),
        Some("    // three\n    \"b\": 2,\n")
    );
    assert_eq!(
        default_block(defaults, "c"),
        None,
        "a value over several lines"
    );
    assert_eq!(default_block(defaults, "missing"), None);
}
