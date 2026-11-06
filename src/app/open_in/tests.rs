use std::cell::RefCell;
use std::io::{Error, ErrorKind};
use std::path::Path;

use super::{Launch, LaunchFailure, Platform, launch_first, terminal_candidates};

fn programs(candidates: &[Launch]) -> Vec<&str> {
    candidates.iter().map(|c| c.program.as_str()).collect()
}

#[test]
fn the_setting_is_the_only_terminal_tried() {
    let setting = [
        "kitty".to_owned(),
        "--directory".to_owned(),
        "${folder}".to_owned(),
    ];

    let found = terminal_candidates(Some(&setting), Some("foot"), Platform::Linux);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].program, "kitty");
    assert_eq!(found[0].args, ["--directory", "${folder}"]);
}

#[test]
fn on_linux_the_environments_terminal_comes_first() {
    let found = terminal_candidates(None, Some("foot"), Platform::Linux);

    assert_eq!(programs(&found)[..2], ["foot", "x-terminal-emulator"]);
}

#[test]
fn an_empty_environment_terminal_is_not_tried() {
    let found = terminal_candidates(None, Some("  "), Platform::Linux);

    assert_eq!(programs(&found)[0], "x-terminal-emulator");
}

#[test]
fn windows_and_mac_have_their_own() {
    let windows = terminal_candidates(None, Some("foot"), Platform::Windows);
    let mac = terminal_candidates(None, Some("foot"), Platform::Mac);

    assert_eq!(programs(&windows), ["wt", "cmd"]);
    assert_eq!(programs(&mac), ["open"]);
}

fn starts_only(installed: &'static str) -> impl Fn(&str, &[String], &Path) -> std::io::Result<()> {
    move |program, _, _| match program == installed {
        true => Ok(()),
        false => Err(Error::from(ErrorKind::NotFound)),
    }
}

#[test]
fn a_terminal_that_is_not_installed_is_passed_over() {
    let candidates = [
        Launch::new("a", &[]),
        Launch::new("b", &[]),
        Launch::new("c", &[]),
    ];

    let started = launch_first(&candidates, Path::new("/p"), starts_only("b"));

    assert_eq!(started, Ok("b".to_owned()));
}

#[test]
fn with_none_installed_the_answer_says_so() {
    let candidates = [Launch::new("a", &[])];

    let started = launch_first(&candidates, Path::new("/p"), starts_only("z"));

    assert_eq!(started, Err(LaunchFailure::NoneFound));
}

#[test]
fn a_terminal_that_is_there_but_fails_stops_the_search() {
    let candidates = [Launch::new("a", &[]), Launch::new("b", &[])];
    let tried = RefCell::new(Vec::new());

    let started = launch_first(&candidates, Path::new("/p"), |program, _, _| {
        tried.borrow_mut().push(program.to_owned());
        Err(Error::new(ErrorKind::PermissionDenied, "no"))
    });

    assert_eq!(tried.into_inner(), ["a"], "b is not tried after a failed");
    assert!(matches!(started, Err(LaunchFailure::Failed { program, .. }) if program == "a"));
}

#[test]
fn the_folder_goes_into_the_arguments_and_is_the_working_folder() {
    let candidates = [Launch::new("t", &["--in", "${folder}", "x${folder}y"])];
    let seen = RefCell::new(None);

    launch_first(&candidates, Path::new("/my proj"), |_, args, folder| {
        *seen.borrow_mut() = Some((args.to_vec(), folder.to_path_buf()));
        Ok(())
    })
    .unwrap();

    let (args, folder) = seen.into_inner().unwrap();
    assert_eq!(args, ["--in", "/my proj", "x/my projy"]);
    assert_eq!(folder, Path::new("/my proj"));
}
