use std::path::Path;

use super::{cut_from_left, title_of};

#[test]
fn a_file_in_the_project_is_shown_from_its_folder() {
    let title = title_of(
        Some(Path::new("/work/app/src/main.rs")),
        Some(Path::new("/work/app")),
        true,
    );

    assert_eq!(title, Path::new("src/main.rs").display().to_string());
}

#[test]
fn a_file_outside_the_project_is_shown_whole() {
    let title = title_of(
        Some(Path::new("/etc/hosts")),
        Some(Path::new("/work/app")),
        true,
    );

    assert_eq!(title, "/etc/hosts");
}

#[test]
fn a_folder_that_only_starts_like_the_project_is_outside_it() {
    let title = title_of(
        Some(Path::new("/work/app2/a.rs")),
        Some(Path::new("/work/app")),
        true,
    );

    assert_eq!(title, "/work/app2/a.rs");
}

#[test]
fn a_tab_with_no_file_is_untitled_and_no_tab_says_nothing() {
    assert_eq!(title_of(None, Some(Path::new("/work")), true), "[untitled]");
    assert_eq!(title_of(None, Some(Path::new("/work")), false), "");
}

#[test]
fn without_a_project_the_path_is_whole() {
    assert_eq!(title_of(Some(Path::new("/a/b.rs")), None, true), "/a/b.rs");
}

#[test]
fn text_that_fits_is_left_alone() {
    assert_eq!(cut_from_left("src/main.rs", 11), "src/main.rs");
    assert_eq!(cut_from_left("src/main.rs", 40), "src/main.rs");
}

#[test]
fn text_that_does_not_fit_loses_its_start_and_keeps_the_file_name() {
    let cut = cut_from_left("crates/ui/src/components/menu_bar.rs", 16);

    assert!(cut.starts_with('…'), "{cut}");
    assert!(cut.ends_with("menu_bar.rs"), "{cut}");
    assert_eq!(unicode_width::UnicodeWidthStr::width(cut.as_str()), 16);
}

#[test]
fn wide_characters_are_counted_by_their_columns() {
    let cut = cut_from_left("dir/日本語日本語.rs", 9);

    assert!(
        unicode_width::UnicodeWidthStr::width(cut.as_str()) <= 9,
        "{cut}"
    );
    assert!(cut.starts_with('…') && cut.ends_with(".rs"));
}

#[test]
fn no_room_gives_nothing() {
    assert_eq!(cut_from_left("src/main.rs", 0), "");
    assert_eq!(cut_from_left("src/main.rs", 1), "");
}
