use std::path::Path;

use crate::lsp::uri::{path_from_uri, windows_file_uri};

#[cfg(unix)]
use crate::lsp::uri::{file_uri, uri_to_path};

#[cfg(unix)]
#[test]
fn a_unix_path_is_a_file_uri() {
    assert_eq!(
        file_uri(Path::new("/home/tim/project")).as_deref(),
        Some("file:///home/tim/project")
    );
}

#[cfg(unix)]
#[test]
fn a_relative_path_has_no_uri() {
    assert_eq!(file_uri(Path::new("src/main.rs")), None);
}

#[cfg(unix)]
#[test]
fn characters_a_uri_does_not_allow_are_percent_encoded() {
    assert_eq!(
        file_uri(Path::new("/a b/c#d/100%/é😀.rs")).as_deref(),
        Some("file:///a%20b/c%23d/100%25/%C3%A9%F0%9F%98%80.rs")
    );
    // These stay as they are: they are allowed in a path.
    assert_eq!(
        file_uri(Path::new("/x-y_z.~/@a+b,c;d=e(f)!$&'*")).as_deref(),
        Some("file:///x-y_z.~/@a+b,c;d=e(f)!$&'*")
    );
}

#[cfg(unix)]
#[test]
fn a_uri_gives_the_path_back() {
    for path in [
        "/home/tim/project",
        "/a b/c#d/100%/é😀.rs",
        "/",
        "/with%20percent literally%2F",
    ] {
        let uri = file_uri(Path::new(path)).unwrap();

        assert_eq!(uri_to_path(&uri).as_deref(), Some(Path::new(path)), "{uri}");
    }
}

#[test]
fn a_uri_with_an_encoded_path_is_decoded() {
    assert_eq!(
        path_from_uri("file:///a%20b/%C3%A9.rs", false).as_deref(),
        Some("/a b/é.rs")
    );
}

#[test]
fn only_local_file_uris_are_paths() {
    for uri in [
        "https://example.com/a",
        "untitled:Untitled-1",
        "file://otherhost/a/b",
        "file:///bad%zz",
        "file:///bad%2",
        "file:///not-utf8%FF",
        "file:no-slashes",
    ] {
        assert_eq!(path_from_uri(uri, false), None, "{uri}");
    }
}

#[test]
fn localhost_is_this_machine() {
    assert_eq!(
        path_from_uri("file://localhost/a/b", false).as_deref(),
        Some("/a/b")
    );
}

#[test]
fn a_windows_drive_path_is_a_uri_with_forward_slashes_and_the_drive_kept() {
    assert_eq!(
        windows_file_uri(r"C:\Users\Tim Kieboom\proj").as_deref(),
        Some("file:///C:/Users/Tim%20Kieboom/proj")
    );
    assert_eq!(
        windows_file_uri(r"d:\a\é.rs").as_deref(),
        Some("file:///d:/a/%C3%A9.rs")
    );
}

#[test]
fn a_windows_share_is_a_uri_with_a_host() {
    assert_eq!(
        windows_file_uri(r"\\server\share\a b").as_deref(),
        Some("file://server/share/a%20b")
    );
}

#[test]
fn a_windows_path_that_is_not_absolute_has_no_uri() {
    for path in [r"src\main.rs", r"\no\drive", "", r"\\"] {
        assert_eq!(windows_file_uri(path), None, "{path:?}");
    }
}

#[test]
fn a_uri_gives_a_windows_path_back_in_every_spelling_an_editor_uses() {
    for uri in [
        "file:///C:/Users/Tim/proj",
        "file:///c%3A/Users/Tim/proj",
        "file://localhost/C:/Users/Tim/proj",
    ] {
        let path = path_from_uri(uri, true).unwrap();

        assert!(
            path.eq_ignore_ascii_case(r"C:\Users\Tim\proj"),
            "{uri} gave {path}"
        );
    }
    assert_eq!(
        path_from_uri("file://server/share/a%20b", true).as_deref(),
        Some(r"\\server\share\a b")
    );
}

#[test]
fn a_uri_with_no_drive_is_no_windows_path() {
    assert_eq!(path_from_uri("file:///usr/bin", true), None);
}

#[test]
fn a_windows_path_survives_the_trip() {
    let path = r"C:\Users\Tim Kieboom\é 😀\a#b.rs";
    let uri = windows_file_uri(path).unwrap();

    assert_eq!(path_from_uri(&uri, true).as_deref(), Some(path));
}

#[cfg(windows)]
#[test]
fn the_verbatim_prefix_windows_adds_is_not_in_the_uri() {
    use crate::lsp::uri::file_uri;
    assert_eq!(
        file_uri(Path::new(r"\\?\C:\proj")).as_deref(),
        Some("file:///C:/proj")
    );
}
