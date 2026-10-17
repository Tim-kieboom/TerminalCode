use std::path::Path;

use crate::paths::without_verbatim_prefix;

fn stripped(path: &str) -> String {
    without_verbatim_prefix(Path::new(path))
        .to_string_lossy()
        .into_owned()
}

#[test]
fn a_drive_path_loses_the_verbatim_prefix() {
    assert_eq!(stripped(r"\\?\C:\Users\tim\code"), r"C:\Users\tim\code");
    assert_eq!(stripped(r"\\?\c:\"), r"c:\");
    assert_eq!(stripped(r"\\?\D:"), "D:");
}

#[test]
fn a_network_path_becomes_a_normal_unc_path() {
    assert_eq!(stripped(r"\\?\UNC\server\share\dir"), r"\\server\share\dir");
}

#[test]
fn paths_without_the_prefix_are_left_alone() {
    for path in [
        r"C:\Users\tim",
        r"\\server\share",
        "/home/tim",
        "src/main.rs",
        "",
    ] {
        assert_eq!(stripped(path), path);
    }
}

#[test]
fn a_verbatim_path_that_is_no_drive_path_stays_as_it_is() {
    for path in [
        r"\\?\Volume{1234}\dir",
        r"\\?\GLOBALROOT\x",
        r"\\?\1:\x",
        r"\\?\C:x",
    ] {
        assert_eq!(stripped(path), path);
    }
}
