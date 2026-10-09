use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::components::search::worker::SearchHandle;
use crate::components::search::{Command, Hit, Options, Search, Status};

fn search() -> Search {
    Search::new(PathBuf::from("/project"), String::new(), Options::default())
}

fn hit(name: &str, line: u64) -> Hit {
    Hit::new(name.into(), line, "text", 0..4)
}

fn running(id: u64) -> Search {
    let mut search = search();
    search.begin(id, SearchHandle::inert());
    search
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> Option<Command> {
    Command::from_key(KeyEvent::new(code, modifiers))
}

#[test]
fn a_new_search_is_idle_with_what_was_typed_last_time() {
    let search = Search::new(
        PathBuf::from("/p"),
        "needle".to_owned(),
        Options {
            case_sensitive: true,
            regex: false,
        },
    );

    assert_eq!(search.query(), "needle");
    assert!(search.options().case_sensitive);
    assert_eq!(search.status(), &Status::Idle);
}

#[test]
fn hits_are_taken_from_the_current_search_only() {
    let mut search = running(5);

    search.add_hits(5, vec![hit("a", 1)]);
    search.add_hits(4, vec![hit("stale", 1)]);
    search.finish(4, 9, false);

    assert_eq!(search.hits().len(), 1);
    assert_eq!(search.status(), &Status::Running);

    search.finish(5, 1, false);
    assert_eq!(
        search.status(),
        &Status::Done {
            files: 1,
            truncated: false
        }
    );
}

#[test]
fn beginning_again_forgets_the_results_and_the_selection() {
    let mut search = running(1);
    search.add_hits(1, vec![hit("a", 1), hit("b", 1)]);
    search.select_next();

    search.begin(2, SearchHandle::inert());

    assert!(search.hits().is_empty());
    assert_eq!(search.selected(), 0);
    search.add_hits(1, vec![hit("late", 1)]);
    assert!(search.hits().is_empty(), "the first search is over");
}

#[test]
fn idle_and_invalid_forget_the_results_too() {
    let mut search = running(1);
    search.add_hits(1, vec![hit("a", 1)]);

    search.invalid("unclosed group".to_owned());
    assert!(search.hits().is_empty());
    assert_eq!(
        search.status(),
        &Status::Invalid("unclosed group".to_owned())
    );

    search.idle();
    assert_eq!(search.status(), &Status::Idle);
}

#[test]
fn the_selection_moves_stops_at_the_ends_and_pages() {
    let mut search = running(1);
    search.add_hits(1, (1..=30).map(|line| hit("a", line)).collect());

    search.select_previous();
    assert_eq!(search.selected(), 0);
    search.select_next();
    assert_eq!(search.selected(), 1);
    search.page_down();
    assert_eq!(search.selected(), 11);
    search.page_down();
    search.page_down();
    search.page_down();
    assert_eq!(search.selected(), 29);
    search.page_up();
    assert_eq!(search.selected(), 19);
    for _ in 0..5 {
        search.page_up();
    }
    assert_eq!(search.selected(), 0);
}

#[test]
fn the_selection_stays_put_while_more_hits_arrive() {
    let mut search = running(1);
    search.add_hits(1, vec![hit("a", 1), hit("b", 1)]);
    search.select_next();

    search.add_hits(1, vec![hit("c", 1)]);

    assert_eq!(search.selected_hit().unwrap().path, PathBuf::from("b"));
}

#[test]
fn the_options_toggle_and_the_query_edits() {
    let mut search = search();

    search.toggle_case();
    search.toggle_regex();
    assert_eq!(
        search.options(),
        Options {
            case_sensitive: true,
            regex: true
        }
    );
    search.toggle_case();
    assert!(!search.options().case_sensitive);

    "ab".chars().for_each(|c| search.insert(c));
    search.backspace();
    assert_eq!(search.query(), "a");
    search.clear_query();
    assert_eq!(search.query(), "");
}

#[test]
fn keys_map_to_commands() {
    let none = KeyModifiers::NONE;
    assert_eq!(key(KeyCode::Char('x'), none), Some(Command::Insert('x')));
    assert_eq!(
        key(KeyCode::Char('X'), KeyModifiers::SHIFT),
        Some(Command::Insert('X'))
    );
    assert_eq!(key(KeyCode::Enter, none), Some(Command::Open));
    assert_eq!(key(KeyCode::Esc, none), Some(Command::Cancel));
    assert_eq!(key(KeyCode::Up, none), Some(Command::Previous));
    assert_eq!(key(KeyCode::Down, none), Some(Command::Next));
    assert_eq!(key(KeyCode::PageUp, none), Some(Command::PageUp));
    assert_eq!(key(KeyCode::PageDown, none), Some(Command::PageDown));
    assert_eq!(key(KeyCode::Backspace, none), Some(Command::Backspace));
    assert_eq!(
        key(KeyCode::Char('u'), KeyModifiers::CONTROL),
        Some(Command::ClearQuery)
    );
    assert_eq!(
        key(KeyCode::Char('c'), KeyModifiers::ALT),
        Some(Command::ToggleCase)
    );
    assert_eq!(
        key(KeyCode::Char('r'), KeyModifiers::ALT),
        Some(Command::ToggleRegex)
    );
}

#[test]
fn other_modified_keys_and_releases_are_not_commands() {
    assert_eq!(key(KeyCode::Char('c'), KeyModifiers::CONTROL), None);
    assert_eq!(key(KeyCode::Char('x'), KeyModifiers::ALT), None);
    assert_eq!(key(KeyCode::F(3), KeyModifiers::NONE), None);
    let mut release = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    assert_eq!(Command::from_key(release), None);
}
