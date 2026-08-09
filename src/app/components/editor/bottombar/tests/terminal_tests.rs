use std::path::{Path, PathBuf};

use ratatui::layout::Rect;

use super::{Session, inner_area, normalize, parse_cd_target, resolve_directory, truncate_start};

#[test]
fn dropping_session_does_not_block() {
    let session = Session::spawn(20, 80, Path::new(".")).expect("failed to spawn session");
    let handle = std::thread::spawn(move || drop(session));

    handle.join().expect("dropping session blocked");
}

#[test]
fn inner_area_is_shifted_down_by_one() {
    let area = Rect {
        x: 2,
        y: 5,
        width: 60,
        height: 10,
    };

    let inner = inner_area(area);

    assert_eq!(
        inner,
        Rect {
            x: 2,
            y: 6,
            width: 60,
            height: 9
        }
    );
}

#[test]
fn inner_area_handles_empty_area() {
    let area = Rect {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    let inner = inner_area(area);

    assert_eq!(
        inner,
        Rect {
            x: 0,
            y: 1,
            width: 0,
            height: 0
        }
    );
}

#[test]
fn truncate_start_keeps_short_text_unchanged() {
    assert_eq!(truncate_start("short", 20), "short");
}

#[test]
fn truncate_start_keeps_tail_when_trimming() {
    let text = " Terminal │ C:\\very\\long\\path";

    assert_eq!(truncate_start(text, 10), "…long\\path");
}

#[test]
fn truncate_start_with_zero_limit_drops_everything() {
    assert_eq!(truncate_start("anything", 0), "…");
}

#[test]
fn parse_cd_target_extracts_plain_target() {
    assert_eq!(
        parse_cd_target("cd /home/user"),
        Some("/home/user".to_string())
    );
    assert_eq!(parse_cd_target("  cd   foo "), Some("foo".to_string()));
}

#[test]
fn parse_cd_target_is_case_insensitive() {
    assert_eq!(parse_cd_target("CD target"), Some("target".to_string()));
}

#[test]
fn parse_cd_target_unquotes_surrounding_quotes() {
    assert_eq!(parse_cd_target("cd \"my dir\""), Some("my dir".to_string()));
    assert_eq!(parse_cd_target("cd 'x'"), Some("x".to_string()));
}

#[test]
fn parse_cd_target_rejects_bare_cd() {
    assert_eq!(parse_cd_target("cd"), None);
    assert_eq!(parse_cd_target("cd   "), None);
}

#[test]
fn parse_cd_target_rejects_unresolvable_targets() {
    assert_eq!(parse_cd_target("cd ~/x"), None);
    assert_eq!(parse_cd_target("cd $HOME"), None);
    assert_eq!(parse_cd_target("cd -"), None);
    assert_eq!(parse_cd_target("cd a && ls"), None);
}

#[test]
fn parse_cd_target_rejects_other_commands() {
    assert_eq!(parse_cd_target("ls"), None);
    assert_eq!(parse_cd_target("copy /a /b"), None);
}

#[test]
fn resolve_directory_joins_and_normalizes() {
    let current = PathBuf::from("C:\\foo\\bar");

    assert_eq!(
        resolve_directory(&current, ".."),
        Some(PathBuf::from("C:\\foo"))
    );
    assert_eq!(
        resolve_directory(&current, "baz"),
        Some(PathBuf::from("C:\\foo\\bar\\baz"))
    );
    assert_eq!(
        resolve_directory(&current, "C:\\other"),
        Some(PathBuf::from("C:\\other"))
    );
}

#[test]
fn normalize_collapses_cur_dir_and_parent_dir() {
    let path = PathBuf::from("/a/b/../c/./d");

    assert_eq!(normalize(&path), PathBuf::from("/a/c/d"));
}
