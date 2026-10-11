use crate::components::terminal::TerminalPane;
use crate::pty::{Session, Shell, SpawnConfig};

fn session(script: &str) -> Session {
    Session::spawn(
        &SpawnConfig {
            shell: Shell::script(script),
            directory: std::env::temp_dir(),
            rows: 10,
            columns: 40,
        },
        || {},
    )
    .unwrap()
}

/// A pane with `count` tabs that keep running.
fn pane_with(count: usize) -> TerminalPane {
    let mut pane = TerminalPane::default();
    for number in 1..=count {
        pane.start(session("sleep 30"), format!("sh{number}"));
    }
    pane
}

fn wait_until_ended(pane: &TerminalPane, index: usize) {
    let session = pane.session_of(index).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while session.finished().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "the shell never ended"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn a_new_tab_becomes_the_active_one() {
    let pane = pane_with(3);

    assert_eq!(pane.tab_count(), 3);
    assert_eq!(pane.active_tab(), 2);
}

#[test]
fn selecting_goes_to_a_tab_and_ignores_one_that_is_not_there() {
    let mut pane = pane_with(3);

    pane.select(0);
    assert_eq!(pane.active_tab(), 0);
    pane.select(7);
    assert_eq!(pane.active_tab(), 0);
}

#[test]
fn next_and_previous_wrap_around() {
    let mut pane = pane_with(3);

    pane.select_next();
    assert_eq!(pane.active_tab(), 0);
    pane.select_previous();
    assert_eq!(pane.active_tab(), 2);
    pane.select_previous();
    assert_eq!(pane.active_tab(), 1);
}

#[test]
fn closing_the_active_tab_moves_to_the_one_before_it() {
    let mut pane = pane_with(3);
    pane.select(1);

    pane.close(1);

    assert_eq!(pane.tab_count(), 2);
    assert_eq!(pane.active_tab(), 0);
}

#[test]
fn closing_the_first_tab_keeps_the_keyboard_on_the_new_first() {
    let mut pane = pane_with(3);
    pane.select(0);

    pane.close(0);

    assert_eq!(pane.active_tab(), 0);
    assert_eq!(pane.tab_count(), 2);
}

#[test]
fn closing_a_tab_before_the_active_one_keeps_the_same_tab_active() {
    let mut pane = pane_with(3);
    let before = pane.session().unwrap().process_id();

    pane.close(0);

    assert_eq!(pane.active_tab(), 1);
    assert_eq!(pane.session().unwrap().process_id(), before);
}

#[test]
fn closing_the_last_tab_leaves_none() {
    let mut pane = pane_with(1);

    pane.close(0);

    assert_eq!(pane.tab_count(), 0);
    assert!(pane.session().is_none());
    assert!(!pane.is_running());
}

#[test]
fn closing_a_tab_that_is_not_there_does_nothing() {
    let mut pane = pane_with(2);

    pane.close(5);

    assert_eq!(pane.tab_count(), 2);
}

#[test]
fn closing_a_tab_stops_its_shell() {
    let mut pane = pane_with(2);
    let pid = pane.session_of(0).unwrap().process_id().unwrap();
    let process = std::path::PathBuf::from(format!("/proc/{pid}"));
    if !process.exists() {
        return; // no /proc on this platform
    }

    pane.close(0);

    assert!(!process.exists());
}

#[test]
fn tabs_whose_shell_ended_can_be_dropped_and_the_active_one_follows() {
    let mut pane = TerminalPane::default();
    pane.start(session("exit 0"), "a".to_owned());
    pane.start(session("sleep 30"), "b".to_owned());
    pane.start(session("exit 1"), "c".to_owned());
    pane.start(session("sleep 30"), "d".to_owned());
    wait_until_ended(&pane, 0);
    wait_until_ended(&pane, 2);
    let running = pane.session().unwrap().process_id();

    pane.remove_ended();

    assert_eq!(pane.tab_count(), 2);
    assert_eq!(pane.active_tab(), 1);
    assert_eq!(pane.session().unwrap().process_id(), running);
    assert!(pane.any_running());
}

#[test]
fn each_ended_shell_is_reported_once_with_its_tab_number() {
    let mut pane = TerminalPane::default();
    pane.start(session("sleep 30"), "a".to_owned());
    pane.start(session("exit 4"), "b".to_owned());
    wait_until_ended(&pane, 1);

    assert_eq!(pane.take_exits(), [(2, 4)]);
    assert_eq!(pane.take_exits(), []);
}

#[test]
fn the_labels_number_the_tabs_and_mark_the_ended_ones() {
    let mut pane = TerminalPane::default();
    pane.start(session("sleep 30"), "bash".to_owned());
    pane.start(session("exit 0"), "zsh".to_owned());
    wait_until_ended(&pane, 1);

    assert_eq!(pane.labels(), [" 1: bash ", " 2: zsh (ended) "]);
}

#[test]
fn the_plus_button_goes_right_after_the_last_tab() {
    use crate::components::terminal::add_button_after;
    use ratatui::layout::Rect;
    let bar = Rect::new(10, 20, 60, 1);
    let tabs = [Rect::new(10, 20, 7, 1), Rect::new(17, 20, 9, 1)];

    assert_eq!(add_button_after(&tabs, bar), Rect::new(26, 20, 3, 1));
}

#[test]
fn with_no_tabs_the_plus_button_is_at_the_start_of_the_bar() {
    use crate::components::terminal::add_button_after;
    use ratatui::layout::Rect;

    assert_eq!(
        add_button_after(&[], Rect::new(10, 20, 60, 1)),
        Rect::new(10, 20, 3, 1)
    );
}

#[test]
fn when_the_tabs_fill_the_bar_the_plus_button_stays_at_its_end() {
    use crate::components::terminal::add_button_after;
    use ratatui::layout::Rect;
    let bar = Rect::new(0, 0, 30, 1);
    let tabs = [Rect::new(0, 0, 27, 1)];

    assert_eq!(add_button_after(&tabs, bar), Rect::new(27, 0, 3, 1));
    let overflowing = [Rect::new(0, 0, 30, 1)];
    assert_eq!(add_button_after(&overflowing, bar), Rect::new(27, 0, 3, 1));
}

#[test]
fn without_a_bar_or_room_there_is_no_plus_button() {
    use crate::components::terminal::add_button_after;
    use ratatui::layout::Rect;

    assert_eq!(
        add_button_after(&[], Rect::new(0, 0, 40, 0)),
        Rect::default()
    );
    assert_eq!(
        add_button_after(&[], Rect::new(0, 0, 2, 1)),
        Rect::default()
    );
}
