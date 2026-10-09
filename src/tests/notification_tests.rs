use std::time::{Duration, Instant};

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::components::notifications::INFO_LIFETIME;
use crate::components::prepare_and_render;

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_input(key(KeyCode::Char(c), KeyModifiers::NONE));
    }
}

fn click(column: u16, row: u16) -> InputEvent {
    InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

fn buffer_text(app: &App) -> String {
    app.state().editor().buffer().text()
}

#[test]
fn an_info_message_goes_away_after_its_lifetime() {
    let mut app = App::default();
    app.state_mut().notify("saved");
    let now = Instant::now();

    app.expire_notifications(now + INFO_LIFETIME - Duration::from_secs(1));
    assert_eq!(app.state().latest_notification(), Some("saved"));

    app.expire_notifications(now + INFO_LIFETIME + Duration::from_secs(1));
    assert_eq!(app.state().latest_notification(), None);
    assert_eq!(app.notification_deadline(), None);
}

#[test]
fn an_info_message_sets_a_deadline_and_an_error_does_not() {
    let mut app = App::default();
    app.state_mut().notify_error("broken");
    assert_eq!(app.notification_deadline(), None);

    app.state_mut().notify("saved");
    assert!(app.notification_deadline().is_some());
}

#[test]
fn an_error_never_expires_by_itself() {
    let mut app = App::default();
    app.state_mut().notify_error("broken");

    app.expire_notifications(Instant::now() + Duration::from_secs(3600));

    assert_eq!(app.state().latest_notification(), Some("broken"));
}

#[test]
fn expiring_a_message_asks_for_a_redraw_and_expiring_nothing_does_not() {
    let mut app = App::default();
    app.state_mut().notify("saved");
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    app.mark_drawn();

    app.expire_notifications(Instant::now());
    assert!(!app.needs_redraw());

    app.expire_notifications(Instant::now() + INFO_LIFETIME * 2);
    assert!(app.needs_redraw());
}

#[test]
fn the_next_key_dismisses_an_error_and_still_does_its_job() {
    let mut app = App::default();
    app.state_mut().notify_error("broken");

    type_str(&mut app, "a");

    assert_eq!(app.state().latest_notification(), None);
    assert_eq!(buffer_text(&app), "a");
}

#[test]
fn an_error_caused_by_a_key_survives_that_key() {
    let mut app = App::default();
    type_str(&mut app, "x");

    // The buffer has no path, so saving fails.
    app.handle_input(key(KeyCode::Char('s'), KeyModifiers::CONTROL));

    assert!(app.state().notifications().has_errors());
}

#[test]
fn a_key_release_does_not_dismiss_an_error() {
    let mut app = App::default();
    app.state_mut().notify_error("broken");
    let mut release = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;

    app.handle_input(InputEvent::Key(release));

    assert!(app.state().notifications().has_errors());
}

#[test]
fn a_click_dismisses_an_error_but_scrolling_does_not() {
    let mut app = App::default();
    app.state_mut().notify_error("broken");

    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    }));
    assert!(app.state().notifications().has_errors());

    app.handle_input(click(5, 5));
    assert!(!app.state().notifications().has_errors());
}

#[test]
fn a_key_leaves_info_messages_alone() {
    let mut app = App::default();
    app.state_mut().notify("saved");

    type_str(&mut app, "a");

    assert_eq!(app.state().latest_notification(), Some("saved"));
}

#[test]
fn a_dismissed_error_leaves_the_info_message_next_to_it() {
    let mut app = App::default();
    app.state_mut().notify("saved");
    app.state_mut().notify_error("broken");

    type_str(&mut app, "a");

    assert_eq!(app.state().latest_notification(), Some("saved"));
    assert_eq!(app.state().notifications().len(), 1);
}

#[test]
fn showing_the_same_message_again_does_not_stack_copies() {
    let mut app = App::default();

    app.state_mut().notify_error("broken");
    app.state_mut().notify_error("broken");
    app.state_mut().notify("saved");
    app.state_mut().notify("saved");

    assert_eq!(app.state().notifications().len(), 2);
}

#[test]
fn only_the_newest_messages_are_kept() {
    let mut app = App::default();

    for number in 0..50 {
        app.state_mut().notify_error(format!("error {number}"));
    }

    assert_eq!(app.state().notifications().len(), 20);
    assert_eq!(app.state().latest_notification(), Some("error 49"));
}

#[test]
fn messages_are_drawn_stacked_with_the_newest_at_the_bottom_right() {
    let mut app = App::default();
    app.state_mut().notify("first message");
    app.state_mut().notify_error("second message");

    let screen = screen(&mut app, 100, 30);

    let rows: Vec<&str> = screen.lines().collect();
    let row_of = |needle: &str| rows.iter().position(|row| row.contains(needle)).unwrap();
    assert!(
        row_of("first message") < row_of("second message"),
        "{screen}"
    );
    assert!(screen.contains(" Error "), "{screen}");
    let column = rows[row_of("second message")]
        .find("second message")
        .unwrap();
    assert!(column > 100 / 2, "drawn on the right: {screen}");
}

#[test]
fn messages_sit_above_the_status_bar() {
    let mut app = App::default();
    app.state_mut().notify("a message");

    let screen = screen(&mut app, 100, 30);

    let rows: Vec<&str> = screen.lines().collect();
    let message = rows
        .iter()
        .position(|row| row.contains("a message"))
        .unwrap();
    let status = rows
        .iter()
        .position(|row| row.contains("Ln 1, Col 1"))
        .unwrap();
    assert!(message < status, "{screen}");
}

#[test]
fn a_long_message_wraps_instead_of_running_off_screen() {
    let mut app = App::default();
    let long = "the file could not be written because the disk is full and nothing else helps";
    app.state_mut().notify_error(long);

    let screen = screen(&mut app, 100, 30);

    assert!(screen.contains("the file could not be written"), "{screen}");
    assert!(screen.contains("nothing else helps"), "{screen}");
    assert!(
        !screen.contains(long),
        "wrapped over several rows: {screen}"
    );
}

#[test]
fn a_tiny_terminal_does_not_panic() {
    let mut app = App::default();
    app.state_mut().notify_error("broken");

    for (width, height) in [(1, 1), (3, 2), (6, 4), (20, 3)] {
        screen(&mut app, width, height);
    }
}

#[test]
fn the_status_bar_no_longer_carries_messages() {
    let mut app = App::default();
    app.state_mut().notify("a message");

    let screen = screen(&mut app, 100, 30);

    let rows: Vec<&str> = screen.lines().collect();
    let status = rows.iter().find(|row| row.contains("Ln 1, Col 1")).unwrap();
    assert!(!status.contains("a message"), "{status}");
}

fn texts(app: &App) -> Vec<String> {
    app.state()
        .notifications()
        .texts()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_sticky_error_survives_a_flood_of_info_messages() {
    let mut app = App::default();
    app.state_mut().notify_error("the disk is full");

    for number in 0..50 {
        app.state_mut().notify(format!("info {number}"));
    }

    assert!(app.state().notifications().has_errors());
    assert!(texts(&app).contains(&"the disk is full".to_owned()));
    assert_eq!(app.state().notifications().len(), 20);
}

#[test]
fn when_the_store_is_full_the_oldest_info_message_goes_not_an_older_error() {
    let mut app = App::default();
    app.state_mut().notify_error("first error");
    for number in 1..20 {
        app.state_mut().notify(format!("info {number}"));
    }
    assert_eq!(app.state().notifications().len(), 20);

    app.state_mut().notify("one more");

    let kept = texts(&app);
    assert_eq!(kept.len(), 20);
    assert!(kept.contains(&"first error".to_owned()));
    assert!(
        !kept.contains(&"info 1".to_owned()),
        "the oldest info message went"
    );
    assert!(kept.contains(&"info 2".to_owned()));
    assert_eq!(kept.last().map(String::as_str), Some("one more"));
}

#[test]
fn a_new_error_also_makes_room_by_dropping_an_info_message() {
    let mut app = App::default();
    for number in 0..20 {
        app.state_mut().notify(format!("info {number}"));
    }

    app.state_mut().notify_error("new error");

    let kept = texts(&app);
    assert_eq!(kept.len(), 20);
    assert!(!kept.contains(&"info 0".to_owned()));
    assert_eq!(kept.last().map(String::as_str), Some("new error"));
}

#[test]
fn when_everything_stored_is_an_error_the_oldest_error_goes() {
    let mut app = App::default();
    for number in 0..20 {
        app.state_mut().notify_error(format!("error {number}"));
    }

    app.state_mut().notify_error("error 20");

    let kept = texts(&app);
    assert_eq!(kept.len(), 20);
    assert!(!kept.contains(&"error 0".to_owned()));
    assert!(kept.contains(&"error 1".to_owned()));
    assert_eq!(kept.last().map(String::as_str), Some("error 20"));
}

#[test]
fn an_info_message_does_not_displace_errors_in_a_store_full_of_them() {
    let mut app = App::default();
    for number in 0..20 {
        app.state_mut().notify_error(format!("error {number}"));
    }

    app.state_mut().notify("saved a.txt");

    let kept = texts(&app);
    assert_eq!(kept.len(), 20);
    assert!(
        !kept.contains(&"saved a.txt".to_owned()),
        "the new info is the one dropped"
    );
    assert!(kept.contains(&"error 0".to_owned()));
    assert!(app.state().notifications().has_errors());
}

#[test]
fn the_error_is_still_drawn_after_a_flood_of_info_messages() {
    let mut app = App::default();
    app.state_mut().notify_error("the disk is full");
    for number in 0..50 {
        app.state_mut().notify(format!("info {number}"));
    }

    let screen = screen(&mut app, 100, 60);

    assert!(screen.contains("the disk is full"), "{screen}");
}

#[test]
fn the_oldest_infos_going_first_keeps_the_order_of_what_is_left() {
    let mut app = App::default();
    app.state_mut().notify_error("error a");
    app.state_mut().notify("info 1");
    app.state_mut().notify_error("error b");
    for number in 2..=19 {
        app.state_mut().notify(format!("info {number}"));
    }
    app.state_mut().notify("info 20");

    let kept = texts(&app);

    // 21 messages were stored when `info 19` came, so `info 1` went; `info 20` then
    // pushed out `info 2`. The errors stayed in their places.
    assert_eq!(&kept[..3], ["error a", "error b", "info 3"]);
}

#[test]
fn when_not_everything_fits_the_error_and_the_newest_infos_are_drawn_in_order() {
    let mut app = App::default();
    app.state_mut().notify_error("the error");
    for number in 0..10 {
        app.state_mut().notify(format!("info {number}"));
    }

    // 29 rows are free above the status bar: nine messages of three rows.
    let screen = screen(&mut app, 100, 30);

    let rows: Vec<&str> = screen.lines().collect();
    let row_of = |needle: &str| rows.iter().position(|row| row.contains(needle));
    assert!(row_of("the error").is_some(), "{screen}");
    assert!(row_of("info 9").is_some(), "{screen}");
    assert!(row_of("info 2").is_some(), "{screen}");
    assert!(
        row_of("info 0").is_none(),
        "the oldest infos are the ones left out: {screen}"
    );
    assert!(row_of("info 1").is_none(), "{screen}");
    assert!(row_of("the error") < row_of("info 2"), "oldest on top");
    assert!(
        row_of("info 2") < row_of("info 9"),
        "newest nearest the bottom"
    );
}
