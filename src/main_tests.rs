use super::*;

fn args(list: &[&str]) -> impl Iterator<Item = OsString> {
    list.iter()
        .map(OsString::from)
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn no_arguments_opens_nothing() {
    assert_eq!(path_from_args(args(&["terminal_code"])), None);
}

#[test]
fn first_argument_is_the_path() {
    assert_eq!(
        path_from_args(args(&["terminal_code", "notes.md"])),
        Some(PathBuf::from("notes.md"))
    );
}

#[test]
fn leading_double_dash_is_skipped() {
    assert_eq!(
        path_from_args(args(&["terminal_code", "--", "notes.md"])),
        Some(PathBuf::from("notes.md"))
    );
}

#[test]
fn double_dash_alone_opens_nothing() {
    assert_eq!(path_from_args(args(&["terminal_code", "--"])), None);
}

#[test]
fn a_file_named_like_an_option_can_follow_the_separator() {
    assert_eq!(
        path_from_args(args(&["terminal_code", "--", "--odd"])),
        Some(PathBuf::from("--odd"))
    );
}
