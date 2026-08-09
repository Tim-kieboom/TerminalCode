use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use ratatui::layout::Rect;

use super::{
    Parser, Query, Session, cursor_position, find_query, handle_terminal_queries, inner_area,
    normalize, parse_cd_target, resolve_directory, truncate_start,
};

#[test]
fn dropping_session_does_not_block() {
    let session = Session::spawn(20, 80, Path::new(".")).expect("failed to spawn session");
    let handle = std::thread::spawn(move || drop(session));

    handle.join().expect("dropping session blocked");
}

#[test]
fn pty_round_trip_echoes_output() {
    let mut session = Session::spawn(24, 80, Path::new(".")).expect("failed to spawn session");
    session.write(b"echo hello\r\n").expect("failed to write");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let text = loop {
        let text = session.screen.lock().unwrap().screen().contents();
        if text.contains("hello") || std::time::Instant::now() > deadline {
            break text;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };

    assert!(
        text.contains("hello"),
        "pty output was not echoed; screen: {text:?}"
    );
}

#[test]
fn find_query_detects_cursor_position() {
    assert_eq!(find_query(b"\x1b[6n"), Some((0, Query::CursorPosition, 4)));
    assert_eq!(
        find_query(b"abc\x1b[6ndef"),
        Some((3, Query::CursorPosition, 4))
    );
}

#[test]
fn find_query_detects_device_attributes() {
    assert_eq!(find_query(b"\x1b[c"), Some((0, Query::DeviceAttributes, 3)));
    assert_eq!(
        find_query(b"\x1b[0c"),
        Some((0, Query::DeviceAttributes, 4))
    );
}

#[test]
fn find_query_ignores_non_queries() {
    assert_eq!(find_query(b"hello"), None);
    assert_eq!(find_query(b"\x1b[6"), None);
    assert_eq!(find_query(b"\x1b[?9001h"), None);
}

#[test]
fn find_query_skips_dead_sequences() {
    assert_eq!(
        find_query(b"\x1b[?9001h\x1b[6n"),
        Some((8, Query::CursorPosition, 4))
    );
}

#[test]
fn queries_are_answered_immediately() {
    let screen = Arc::new(Mutex::new(Parser::new(24, 80, 0)));
    let writer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut pending = Vec::new();

    handle_terminal_queries(b"\x1b[6n", &mut pending, &screen, &writer);
    assert_eq!(writer.lock().unwrap().as_slice(), b"\x1b[1;1R");

    handle_terminal_queries(b"\x1b[0c", &mut pending, &screen, &writer);
    assert_eq!(writer.lock().unwrap().as_slice(), b"\x1b[1;1R\x1b[?6c");
}

#[test]
fn queries_are_answered_across_chunks() {
    let screen = Arc::new(Mutex::new(Parser::new(24, 80, 0)));
    let writer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut pending = Vec::new();

    handle_terminal_queries(b"\x1b[6", &mut pending, &screen, &writer);
    assert!(writer.lock().unwrap().is_empty());

    handle_terminal_queries(b"n", &mut pending, &screen, &writer);
    assert_eq!(writer.lock().unwrap().as_slice(), b"\x1b[1;1R");
}

#[test]
fn cursor_position_is_reported_one_based() {
    let screen = Arc::new(Mutex::new(Parser::new(24, 80, 0)));
    assert_eq!(cursor_position(&screen), (1, 1));
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
