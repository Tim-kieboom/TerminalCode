use std::fs;
use std::sync::Arc;

use super::Runs;
use crate::plugin_api::{MAX_OPEN_RUNS, Refusal, RunPattern};
use crate::plugins::exec::Control;

fn patterns(texts: &[&str]) -> Vec<RunPattern> {
    texts
        .iter()
        .map(|text| RunPattern::parse(text).unwrap())
        .collect()
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// A project folder with `crates/a` in it, which is trusted, and a plugin that may run
/// `cargo nextest *`.
fn ready() -> (tempfile::TempDir, Runs) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("crates/a")).unwrap();
    let mut runs = Runs::default();
    runs.set_workspace(root.path().to_path_buf(), true);
    runs.allow("rust", patterns(&["cargo nextest *"]));
    (root, runs)
}

fn check(runs: &Runs, request: u64, words: &[&str], cwd: &str) -> Result<super::Spec, Refusal> {
    runs.check("rust", request, "cargo".into(), args(words), cwd)
}

#[test]
fn an_allowed_command_in_a_trusted_folder_is_to_run_there() {
    let (root, runs) = ready();

    let spec = check(&runs, 1, &["nextest", "list"], "").unwrap();

    assert_eq!(spec.program, "cargo");
    assert_eq!(spec.args, args(&["nextest", "list"]));
    assert_eq!(spec.cwd, root.path().canonicalize().unwrap());
}

#[test]
fn a_command_no_pattern_allows_is_not_allowed_and_the_refusal_says_which() {
    let (_root, runs) = ready();

    let refusal = check(&runs, 1, &["install", "ripgrep"], "").unwrap_err();

    match refusal {
        Refusal::NotAllowed { detail } => {
            assert!(detail.contains("cargo install ripgrep"), "{detail}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_plugin_without_an_allowlist_may_run_nothing() {
    let (_root, runs) = ready();

    let refusal = runs.check("other", 1, "cargo".into(), args(&["nextest"]), "");

    assert!(matches!(refusal, Err(Refusal::NotAllowed { .. })));
}

#[test]
fn an_untrusted_folder_runs_nothing_even_what_is_allowed() {
    let (root, mut runs) = ready();
    runs.set_workspace(root.path().to_path_buf(), false);

    assert_eq!(
        check(&runs, 1, &["nextest"], "").unwrap_err(),
        Refusal::Untrusted
    );
}

#[test]
fn what_is_not_allowed_is_told_so_before_the_folder_is_asked_about() {
    let (root, mut runs) = ready();
    runs.set_workspace(root.path().to_path_buf(), false);

    let refusal = check(&runs, 1, &["install", "x"], "").unwrap_err();

    assert!(matches!(refusal, Refusal::NotAllowed { .. }), "{refusal:?}");
}

#[test]
fn without_a_project_folder_nothing_runs() {
    let mut runs = Runs::default();
    runs.allow("rust", patterns(&["cargo *"]));
    runs.trusted = true;

    assert!(matches!(
        check(&runs, 1, &["x"], ""),
        Err(Refusal::Failed { .. })
    ));
}

#[test]
fn a_folder_of_the_project_is_where_it_runs() {
    let (root, runs) = ready();

    let spec = check(&runs, 1, &["nextest"], "crates/a").unwrap();

    assert_eq!(
        spec.cwd,
        root.path().join("crates/a").canonicalize().unwrap()
    );
}

#[test]
fn a_folder_that_leaves_the_project_or_is_not_there_is_a_bad_argument() {
    let (_root, runs) = ready();

    for cwd in [
        "..",
        "../x",
        "/etc",
        "crates/../..",
        "crates/nothing",
        "crates/a/../../..",
    ] {
        let refusal = check(&runs, 1, &["nextest"], cwd).unwrap_err();
        assert!(
            matches!(refusal, Refusal::BadArgument { .. }),
            "{cwd}: {refusal:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_link_that_leads_out_of_the_project_is_not_a_folder_of_it() {
    let (root, runs) = ready();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();

    let refusal = check(&runs, 1, &["nextest"], "escape").unwrap_err();

    assert!(
        matches!(refusal, Refusal::BadArgument { .. }),
        "{refusal:?}"
    );
}

#[test]
fn a_file_is_not_a_folder_to_run_in() {
    let (root, runs) = ready();
    fs::write(root.path().join("file.txt"), "x").unwrap();

    let refusal = check(&runs, 1, &["nextest"], "file.txt").unwrap_err();

    assert!(matches!(refusal, Refusal::BadArgument { .. }));
}

#[test]
fn a_request_number_that_is_running_is_refused_and_a_plugin_has_room_for_so_many() {
    let (_root, mut runs) = ready();
    for request in 0..MAX_OPEN_RUNS as u64 {
        assert!(check(&runs, request, &["nextest"], "").is_ok());
        assert!(runs.begin("rust", request, Arc::new(Control::default())));
    }

    assert!(matches!(
        check(&runs, 0, &["nextest"], ""),
        Err(Refusal::BadArgument { .. })
    ));
    assert_eq!(
        check(&runs, 100, &["nextest"], "").unwrap_err(),
        Refusal::Busy
    );
    runs.finish("rust", 1);
    assert!(check(&runs, 100, &["nextest"], "").is_ok());
}

#[test]
fn a_command_started_for_a_plugin_that_is_gone_is_not_kept_and_is_stopped() {
    let (_root, mut runs) = ready();
    runs.forget("rust");
    let control = Arc::new(Control::default());

    assert!(!runs.begin("rust", 1, Arc::clone(&control)));
    assert_eq!(control.stopped_for_test(), 2);
}

#[test]
fn losing_a_plugin_stops_what_it_runs_and_keeps_its_allowlist_but_forgetting_drops_it() {
    let (_root, mut runs) = ready();
    let control = Arc::new(Control::default());
    runs.begin("rust", 1, Arc::clone(&control));

    runs.lose("rust");

    assert_eq!(control.stopped_for_test(), 2, "lost: nobody is told");
    assert!(check(&runs, 1, &["nextest"], "").is_ok());
    runs.forget("rust");
    assert!(matches!(
        check(&runs, 1, &["nextest"], ""),
        Err(Refusal::NotAllowed { .. })
    ));
}

#[test]
fn a_cancel_marks_only_the_command_it_names() {
    let (_root, mut runs) = ready();
    let (one, two) = (Arc::new(Control::default()), Arc::new(Control::default()));
    runs.begin("rust", 1, Arc::clone(&one));
    runs.begin("rust", 2, Arc::clone(&two));

    runs.cancel("rust", 2);
    runs.cancel("rust", 9);

    assert_eq!((one.stopped_for_test(), two.stopped_for_test()), (0, 1));
}

#[test]
fn trust_changing_names_the_plugins_to_tell_and_trust_staying_does_not() {
    let (root, mut runs) = ready();

    assert!(
        runs.set_workspace(root.path().to_path_buf(), true)
            .is_empty()
    );
    let told = runs.set_workspace(root.path().to_path_buf(), false);
    assert_eq!(told, vec![Box::<str>::from("rust")]);
    assert!(!runs.trusted());
}
