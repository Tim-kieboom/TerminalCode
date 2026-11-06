use std::path::Path;

use super::text_of;

#[test]
fn a_path_inside_the_project_is_copied_from_its_root() {
    let text = text_of(
        Path::new("/work/app/src/main.rs"),
        Path::new("/work/app"),
        true,
    );

    assert_eq!(text, Path::new("src/main.rs").display().to_string());
}

#[test]
fn the_project_folder_itself_is_a_dot() {
    assert_eq!(
        text_of(Path::new("/work/app"), Path::new("/work/app"), true),
        "."
    );
}

#[test]
fn a_path_outside_the_project_is_copied_whole_even_when_relative_is_asked() {
    let text = text_of(Path::new("/etc/hosts"), Path::new("/work/app"), true);

    assert_eq!(text, "/etc/hosts");
}

#[test]
fn a_whole_path_is_copied_as_it_is() {
    let text = text_of(
        Path::new("/work/app/src/main.rs"),
        Path::new("/work/app"),
        false,
    );

    assert_eq!(text, "/work/app/src/main.rs");
}

#[test]
fn a_folder_that_only_starts_the_same_is_not_the_project() {
    let text = text_of(Path::new("/work/app2/a.rs"), Path::new("/work/app"), true);

    assert_eq!(text, "/work/app2/a.rs");
}

#[cfg(windows)]
#[test]
fn a_verbatim_prefix_is_not_copied() {
    let text = text_of(
        Path::new(r"\\?\C:\app\a.rs"),
        Path::new(r"\\?\C:\app"),
        false,
    );

    assert_eq!(text, r"C:\app\a.rs");
}
