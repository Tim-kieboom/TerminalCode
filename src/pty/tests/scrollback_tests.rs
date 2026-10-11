use super::support::{Rig, start};

/// The number printed on the first visible row of a screen of `seq` output.
fn first_row(rig: &Rig) -> u32 {
    rig.text()
        .lines()
        .next()
        .and_then(|line| line.trim().parse().ok())
        .expect("the first row holds a number")
}

/// A screen of 24 rows that has printed 100 numbers and then waits.
fn hundred_lines() -> Rig {
    let rig = start("seq 1 100; sleep 30");
    rig.wait_for_text("100");
    rig
}

#[test]
fn the_view_starts_at_the_present() {
    let rig = hundred_lines();

    assert_eq!(rig.session.scrollback_offset(), 0);
    assert_eq!(first_row(&rig), 78);
}

#[test]
fn scrolling_up_shows_older_lines() {
    let rig = hundred_lines();

    rig.session.scroll_by(10);

    assert_eq!(rig.session.scrollback_offset(), 10);
    assert_eq!(first_row(&rig), 68);
}

#[test]
fn scrolling_stops_at_the_oldest_line_and_at_the_present() {
    let rig = hundred_lines();

    rig.session.scroll_by(1_000_000);
    assert_eq!(first_row(&rig), 1);
    assert_eq!(rig.session.scrollback_offset(), 77);

    rig.session.scroll_by(-1_000_000);
    assert_eq!(rig.session.scrollback_offset(), 0);
    assert_eq!(first_row(&rig), 78);
}

#[test]
fn scrolling_down_after_scrolling_up_comes_back_step_by_step() {
    let rig = hundred_lines();
    rig.session.scroll_by(30);

    rig.session.scroll_by(-10);

    assert_eq!(rig.session.scrollback_offset(), 20);
}

#[test]
fn the_top_and_the_bottom_can_be_jumped_to() {
    let rig = hundred_lines();

    rig.session.scroll_to_top();
    assert_eq!(first_row(&rig), 1);

    rig.session.scroll_to_bottom();
    assert_eq!(rig.session.scrollback_offset(), 0);
    assert_eq!(first_row(&rig), 78);
}

#[test]
fn output_that_arrives_while_scrolled_back_does_not_move_the_view() {
    let rig = start("seq 1 50; sleep 0.5; seq 51 60; sleep 30");
    rig.wait_for_text("50");
    rig.session.scroll_by(5);
    let before = first_row(&rig);
    let received = rig.session.bytes_received();

    rig.wait_until("more output", |rig| rig.session.bytes_received() > received);

    assert_eq!(first_row(&rig), before, "the view stayed on the same lines");
    assert_eq!(
        rig.session.scrollback_offset(),
        15,
        "and the present moved away"
    );
}

#[test]
fn a_full_screen_program_has_its_own_screen_and_no_history() {
    let rig = start(
        "printf main; sleep 0.3; printf '\\033[?1049h\\033[2J\\033[Halt'; sleep 0.8; \
         printf '\\033[?1049l'; sleep 30",
    );
    rig.wait_for_text("alt");

    assert!(rig.session.with_screen(|screen| screen.alternate_screen()));
    assert!(!rig.text().contains("main"), "{}", rig.text());
    rig.session.scroll_by(5);
    assert_eq!(rig.session.scrollback_offset(), 0, "nothing to scroll into");

    rig.wait_until("the normal screen to come back", |rig| {
        !rig.session.with_screen(|screen| screen.alternate_screen())
    });
    assert!(rig.text().contains("main"), "{}", rig.text());
    assert!(!rig.text().contains("alt"));
}

#[test]
fn history_survives_a_full_screen_program() {
    let rig = start(
        "seq 1 100; printf '\\033[?1049h\\033[2Jvim'; sleep 0.5; printf '\\033[?1049l'; sleep 30",
    );
    rig.wait_for_text("vim");
    rig.wait_until("the normal screen", |rig| {
        !rig.session.with_screen(|screen| screen.alternate_screen())
    });

    rig.session.scroll_to_top();

    assert_eq!(first_row(&rig), 1);
}

#[test]
fn a_shorter_screen_keeps_what_was_scrolled_off() {
    let rig = hundred_lines();

    rig.session.resize(12, 80);
    rig.session.scroll_to_top();

    assert_eq!(first_row(&rig), 1);
}
