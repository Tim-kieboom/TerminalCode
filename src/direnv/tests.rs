use std::ffi::OsStr;

use super::{find_envrc, load_with, parse};

#[test]
fn the_envrc_is_looked_for_in_the_folders_above_the_project_too() {
    let root = tempfile::tempdir().unwrap();
    let inner = root.path().join("crates/app");
    std::fs::create_dir_all(&inner).unwrap();
    assert_eq!(find_envrc(&inner), None);

    std::fs::write(root.path().join(".envrc"), "use flake\n").unwrap();

    assert_eq!(find_envrc(&inner), Some(root.path().join(".envrc")));
}

#[test]
fn the_json_of_direnv_gives_the_variables_to_set_and_to_remove() {
    let changes = parse(r#"{"PATH":"/nix/bin:/bin","OLD":null}"#).unwrap();

    assert_eq!(
        changes,
        [
            ("OLD".to_owned(), None),
            ("PATH".to_owned(), Some("/nix/bin:/bin".to_owned())),
        ]
    );
}

#[test]
fn a_name_the_environment_would_panic_on_is_left_out() {
    let changes = parse(r#"{"":"a","A=B":"c","NUL":"x\u0000y","GOOD":"1"}"#).unwrap();

    assert_eq!(changes, [("GOOD".to_owned(), Some("1".to_owned()))]);
}

#[test]
fn output_that_is_no_json_is_an_error() {
    assert!(parse("direnv: error something").is_err());
}

#[cfg(unix)]
mod with_a_program {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use super::*;

    /// A stand-in for `direnv` that runs `body`.
    fn fake_direnv(folder: &Path, body: &str) -> std::path::PathBuf {
        let program = folder.join("direnv");
        std::fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        program
    }

    fn load(body: &str) -> Result<Vec<super::super::Change>, String> {
        let folder = tempfile::tempdir().unwrap();
        let program = fake_direnv(folder.path(), body);
        load_with(
            program.as_os_str(),
            &folder.path().join(".envrc"),
            folder.path(),
        )
    }

    #[test]
    fn the_variables_direnv_prints_are_the_changes() {
        let changes = load(r#"echo '{"IN_NIX_SHELL":"impure"}'"#).unwrap();

        assert_eq!(
            changes,
            [("IN_NIX_SHELL".to_owned(), Some("impure".to_owned()))]
        );
    }

    #[test]
    fn an_envrc_that_is_already_loaded_changes_nothing() {
        assert_eq!(load("echo 'direnv: loading' >&2"), Ok(Vec::new()));
    }

    #[test]
    fn a_blocked_envrc_says_to_allow_it() {
        let error = load("echo 'direnv: error .envrc is blocked' >&2").unwrap_err();

        assert!(error.contains("direnv allow"), "{error}");
    }

    #[test]
    fn a_failing_direnv_shows_its_last_line() {
        let error = load("echo first >&2; echo boom >&2; exit 1").unwrap_err();

        assert_eq!(error, "direnv failed: boom");
    }

    #[test]
    fn without_direnv_the_user_is_told_it_is_missing() {
        let folder = tempfile::tempdir().unwrap();

        let error = load_with(
            OsStr::new("/nonexistent/direnv"),
            &folder.path().join(".envrc"),
            folder.path(),
        )
        .unwrap_err();

        assert!(error.contains("not installed"), "{error}");
    }
}
