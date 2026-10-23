//! What the design is for: a program that prints without end, or a paste the
//! shell does not read, must not make the editor stop answering. The thread
//! that handles keys and draws is stood in for by `probe`.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::support::{Rig, start};

/// How much the full flood prints. It takes about 12 s in a debug build, so it
/// is ignored there: run it with `cargo test --release -- --ignored`. The
/// ordinary tests print a tenth as much (a fifth of that on Windows), which exercises the same paths.
const FULL_FLOOD: u64 = 100 * 1000 * 1000;
/// ConPTY carries about a third of a megabyte a second, so on Windows the short flood
/// is a fifth of that (a few seconds instead of half a minute); it is still thousands
/// of reads, which is what the paths are exercised by.
#[cfg(not(windows))]
const SHORT_FLOOD: u64 = 10 * 1000 * 1000;
#[cfg(windows)]
const SHORT_FLOOD: u64 = 2 * 1000 * 1000;
/// The size of one read of the pty.
const CHUNK: u64 = 8192;

/// What the probe saw while it ran.
#[derive(Debug, Default)]
struct Probe {
    keys: usize,
    frames: usize,
}

/// Does what the app does between two events for as long as `going` holds:
/// handles a key (hands a byte to the shell) and, when the screen changed,
/// draws (looks at every cell of the screen). A call that blocks for good shows as a
/// test that never ends, which nextest kills; how long a call takes is not asserted,
/// because a shared runner stretches it.
fn probe(rig: &Rig, going: impl Fn() -> bool) -> Probe {
    let mut seen = Probe::default();
    while going() {
        rig.session.write(b"k");
        seen.keys += 1;

        if rig.session.take_dirty() {
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

/// Checks that the flood really ran and the probe really ran beside it: keys and
/// frames went on while the shell printed.
fn assert_responsive(bytes: u64) {
    let (rig, seen) = flood(bytes);
    let received = rig.session.bytes_received();
    assert!(received >= bytes, "only {received} bytes came through");
    assert!(
        seen.keys > 20 && seen.frames > 5,
        "{seen:?}: the probe barely ran"
    );
}

#[test]
fn a_short_flood_of_output_does_not_hold_up_keys_or_drawing() {
    assert_responsive(SHORT_FLOOD);
}

#[test]
#[ignore = "100 MB takes about 12 s in a debug build; cargo test --release -- --ignored"]
fn a_hundred_megabytes_of_output_do_not_hold_up_keys_or_drawing() {
    assert_responsive(FULL_FLOOD);
}

#[test]
fn the_flood_wakes_the_app_once_per_drawn_frame_not_once_per_chunk() {
    let (rig, seen) = flood(SHORT_FLOOD);

    let wakes = rig.wakes.load(Ordering::SeqCst);
    let chunks = rig.session.bytes_received() / CHUNK;
    assert!(
        wakes <= seen.frames + 2,
        "{wakes} wake-ups for {} drawn frames ({chunks} chunks were read)",
        seen.frames
    );
    // Most of the flood must have come in reads of the usual size.
    assert!(
        chunks > SHORT_FLOOD / CHUNK * 4 / 5,
        "the flood was not a flood"
    );
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

// A Unix terminal in raw mode stops reading, which fills its input; ConPTY takes input into a console buffer that does not fill the same way, so this cannot be set up there.
#[cfg(unix)]
#[test]
fn a_paste_of_several_megabytes_into_a_shell_that_is_not_reading_does_not_freeze_anything() {
    let rig = start(NOT_READING);
    std::thread::sleep(Duration::from_millis(300));
    let paste = vec![b'p'; 8 * 1024 * 1024];

    let written = rig.session.write(&paste);

    assert!(!written.complete);
    assert!(
        written.sent > 0 && written.sent < 1024 * 1024,
        "{written:?}"
    );
}

#[test]
fn drawing_and_further_keys_stay_responsive_after_such_a_paste() {
    let rig = start(NOT_READING);
    std::thread::sleep(Duration::from_millis(300));
    rig.session.write(&vec![b'p'; 8 * 1024 * 1024]);

    let until = Instant::now() + Duration::from_millis(500);
    let seen = probe(&rig, || Instant::now() < until);

    // The queue is full, so these keys are dropped, but they are handled: the probe
    // only gets here because no write blocked for good.
    assert!(seen.keys > 0, "{seen:?}");
}

// A Unix terminal in raw mode stops reading, which fills its input; ConPTY takes input into a console buffer that does not fill the same way, so this cannot be set up there.
#[cfg(unix)]
#[test]
fn output_keeps_coming_while_a_paste_is_stuck() {
    let rig = start("stty raw -echo; (while true; do printf tick; sleep 0.05; done) & sleep 30");
    std::thread::sleep(Duration::from_millis(300));
    rig.session.write(&vec![b'p'; 8 * 1024 * 1024]);

    let before = rig.session.bytes_received();
    std::thread::sleep(Duration::from_millis(400));

    assert!(rig.session.bytes_received() > before, "the screen froze");
}
