//! What the design is for: a program that prints without end, or a paste the
//! shell does not read, must not make the editor stop answering. The thread
//! that handles keys and draws is stood in for by `probe`.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::support::{Rig, start};

/// How much the full flood prints. It takes about 12 s in a debug build, so it
/// is ignored there: run it with `cargo test --release -- --ignored`. The
/// ordinary tests print a tenth as much, which exercises the same paths.
const FULL_FLOOD: u64 = 100 * 1000 * 1000;
const SHORT_FLOOD: u64 = 10 * 1000 * 1000;
/// The longest a key may take to be handled, or a frame to be drawn.
const LIMIT: Duration = Duration::from_millis(50);

/// What the probe saw while it ran.
#[derive(Debug, Default)]
struct Probe {
    keys: usize,
    frames: usize,
    slowest_key: Duration,
    slowest_frame: Duration,
}

/// Does what the app does between two events for as long as `going` holds:
/// handles a key (hands a byte to the shell) and, when the screen changed,
/// draws (looks at every cell of the screen). Times both.
fn probe(rig: &Rig, going: impl Fn() -> bool) -> Probe {
    let mut seen = Probe::default();
    while going() {
        let start = Instant::now();
        rig.session.write(b"k");
        seen.slowest_key = seen.slowest_key.max(start.elapsed());
        seen.keys += 1;

        if rig.session.take_dirty() {
            let start = Instant::now();
            rig.session.with_screen(|screen| {
                let (rows, columns) = screen.size();
                let mut printed = 0usize;
                for row in 0..rows {
                    for column in 0..columns {
                        if screen
                            .cell(row, column)
                            .is_some_and(|cell| cell.has_contents())
                        {
                            printed += 1;
                        }
                    }
                }
                printed
            });
            seen.slowest_frame = seen.slowest_frame.max(start.elapsed());
            seen.frames += 1;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    seen
}

/// Prints `bytes` of scrolling lines (as `cat` of a big file would) while the
/// probe runs; returns what the probe saw.
fn flood(bytes: u64) -> (Rig, Probe) {
    let rig = start(&format!(
        "yes 'some line of output, long enough to be worth parsing' | head -c {bytes}"
    ));
    let seen = probe(&rig, || rig.session.exit_code().is_none());
    rig.wait_for_exit();
    (rig, seen)
}

fn assert_responsive(rig: &Rig, seen: &Probe, bytes: u64) {
    let received = rig.session.bytes_received();
    assert!(received >= bytes, "only {received} bytes came through");
    assert!(
        seen.keys > 20 && seen.frames > 5,
        "{seen:?}: the probe barely ran"
    );
    assert!(
        seen.slowest_key < LIMIT,
        "a key took {:?}: {seen:?}",
        seen.slowest_key
    );
    assert!(
        seen.slowest_frame < LIMIT,
        "a frame took {:?}: {seen:?}",
        seen.slowest_frame
    );
}

#[test]
fn ten_megabytes_of_output_do_not_hold_up_keys_or_drawing() {
    let (rig, seen) = flood(SHORT_FLOOD);

    assert_responsive(&rig, &seen, SHORT_FLOOD);
}

#[test]
#[ignore = "100 MB takes about 12 s in a debug build; cargo test --release -- --ignored"]
fn a_hundred_megabytes_of_output_do_not_hold_up_keys_or_drawing() {
    let (rig, seen) = flood(FULL_FLOOD);

    assert_responsive(&rig, &seen, FULL_FLOOD);
}

#[test]
fn the_flood_wakes_the_app_once_per_drawn_frame_not_once_per_chunk() {
    let (rig, seen) = flood(SHORT_FLOOD);

    let wakes = rig.wakes.load(Ordering::SeqCst);
    let chunks = rig.session.bytes_received() / 8192;
    assert!(
        wakes <= seen.frames + 2,
        "{wakes} wake-ups for {} drawn frames ({chunks} chunks were read)",
        seen.frames
    );
    assert!(chunks > 1000, "the flood was not a flood");
}

#[test]
fn keys_still_reach_a_shell_that_is_busy_printing() {
    // The shell prints in the background and reads one line meanwhile.
    let rig = start(
        "stty -echo; yes 'output' | head -c 50000000 & \
         read line; echo \"$line\" > received.txt; wait",
    );
    rig.wait_until("the flood to start", |rig| {
        rig.session.bytes_received() > 100_000
    });

    rig.session.write(b"ping from the editor\n");

    let file = rig.directory.join("received.txt");
    rig.wait_until("the line to arrive", |_| {
        std::fs::read_to_string(&file).is_ok_and(|text| text.contains("ping from the editor"))
    });
}

/// A shell that has set raw mode, as a line editor does, and then does not
/// read: the only kind of shell a write can block on.
const NOT_READING: &str = "stty raw -echo; sleep 30";

#[test]
fn a_paste_of_several_megabytes_into_a_shell_that_is_not_reading_does_not_freeze_anything() {
    let rig = start(NOT_READING);
    std::thread::sleep(Duration::from_millis(300));
    let paste = vec![b'p'; 8 * 1024 * 1024];

    let start = Instant::now();
    let written = rig.session.write(&paste);
    let took = start.elapsed();

    assert!(!written.complete);
    assert!(
        written.sent > 0 && written.sent < 1024 * 1024,
        "{written:?}"
    );
    assert!(took < LIMIT, "the paste held the caller for {took:?}");
}

#[test]
fn drawing_and_further_keys_stay_responsive_after_such_a_paste() {
    let rig = start(NOT_READING);
    std::thread::sleep(Duration::from_millis(300));
    rig.session.write(&vec![b'p'; 8 * 1024 * 1024]);

    let until = Instant::now() + Duration::from_millis(500);
    let seen = probe(&rig, || Instant::now() < until);

    // The queue is full, so these keys are dropped, but none of them waits long.
    assert!(seen.keys > 5, "{seen:?}");
    assert!(seen.slowest_key < LIMIT, "{seen:?}");
}

#[test]
fn output_keeps_coming_while_a_paste_is_stuck() {
    let rig = start("stty raw -echo; (while true; do printf tick; sleep 0.05; done) & sleep 30");
    std::thread::sleep(Duration::from_millis(300));
    rig.session.write(&vec![b'p'; 8 * 1024 * 1024]);

    let before = rig.session.bytes_received();
    std::thread::sleep(Duration::from_millis(400));

    assert!(rig.session.bytes_received() > before, "the screen froze");
}
