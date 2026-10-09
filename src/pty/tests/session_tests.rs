use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::pty::{PtyError, Session, Shell, SpawnConfig};

/// A session running `script`, and how often the app was woken.
struct Rig {
    session: Session,
    wakes: Arc<AtomicUsize>,
    directory: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

fn start(script: &str) -> Rig {
    start_shell(Shell::script(script))
}

fn start_shell(shell: Shell) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().canonicalize().unwrap();
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&wakes);
    let session = Session::spawn(
        &SpawnConfig {
            shell,
            directory: directory.clone(),
            rows: 24,
            columns: 80,
        },
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
        },
    )
    .unwrap();
    Rig {
        session,
        wakes,
        directory,
        _dir: dir,
    }
}

impl Rig {
    fn text(&self) -> String {
        self.session.with_screen(|screen| screen.contents())
    }

    /// Waits until `condition` holds, for at most ten seconds.
    fn wait_until(&self, what: &str, condition: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !condition(self) {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}; the screen shows {:?}",
                self.text()
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_for_text(&self, wanted: &str) {
        self.wait_until(wanted, |rig| rig.text().contains(wanted));
    }

    fn wait_for_exit(&self) -> u32 {
        self.wait_until("the shell to exit", |rig| rig.session.exit_code().is_some());
        self.session.exit_code().unwrap()
    }
}

#[test]
fn what_the_shell_prints_shows_on_the_screen() {
    let rig = start("printf hello");

    rig.wait_for_text("hello");
}

#[test]
fn colors_and_cursor_moves_are_understood() {
    let rig = start("printf '\\033[31mred\\033[0m plain\\r\\nnext'");

    rig.wait_for_text("next");

    rig.session.with_screen(|screen| {
        assert_eq!(screen.cell(0, 0).unwrap().fgcolor(), vt100::Color::Idx(1));
        assert_eq!(screen.cell(0, 4).unwrap().fgcolor(), vt100::Color::Default);
        assert!(screen.contents().contains("red plain\nnext"));
    });
}

#[test]
fn text_that_is_not_ascii_arrives_intact() {
    let rig = start("printf 'héllo wörld ✓'");

    rig.wait_for_text("héllo wörld ✓");
}

#[test]
fn the_shell_starts_in_the_directory_asked_for() {
    let rig = start("pwd");

    rig.wait_for_text(rig.directory.to_str().unwrap());
}

#[test]
fn the_shell_is_told_what_kind_of_terminal_it_has() {
    let rig = start("echo $TERM");

    rig.wait_for_text("xterm-256color");
}

#[test]
fn what_is_written_reaches_the_shell() {
    let rig = start("cat");

    let written = rig.session.write(b"typed text\n");

    assert!(written.complete);
    assert_eq!(written.sent, 11);
    rig.wait_for_text("typed text");
}

#[test]
fn a_write_of_several_chunks_arrives_in_order() {
    let rig = start("cat > /dev/null; echo done");
    let line = "0123456789abcdef".repeat(8);
    let mut all = Vec::new();
    for n in 0..200 {
        all.extend_from_slice(format!("{n:03} {line}\n").as_bytes());
    }

    let written = rig.session.write(&all);

    assert!(written.complete, "{written:?}");
    assert_eq!(written.sent, all.len());
}

#[test]
fn the_exit_code_is_known_once_the_shell_ends() {
    let rig = start("exit 3");

    assert_eq!(rig.wait_for_exit(), 3);
}

#[test]
fn a_shell_that_is_still_running_has_no_exit_code() {
    let rig = start("sleep 30");

    assert_eq!(rig.session.exit_code(), None);
    assert!(rig.session.process_id().is_some());
}

#[test]
fn a_program_that_does_not_exist_is_an_error() {
    let dir = tempfile::tempdir().unwrap();

    let result = Session::spawn(
        &SpawnConfig {
            shell: Shell::program("/nonexistent/shell"),
            directory: dir.path().to_path_buf(),
            rows: 24,
            columns: 80,
        },
        || {},
    );

    assert!(matches!(result, Err(PtyError::Spawn { .. })), "{result:?}");
}

#[test]
fn resizing_changes_the_screen_at_once_and_the_shell_after() {
    let rig = start("read x; stty size");

    rig.session.resize(10, 40);
    assert_eq!(rig.session.with_screen(|screen| screen.size()), (10, 40));
    rig.session.write(b"\n");

    rig.wait_for_text("10 40");
}

#[test]
fn many_changes_wake_the_app_once_until_a_draw_takes_them() {
    let rig = start("i=0; while [ $i -lt 3000 ]; do echo line$i; i=$((i+1)); done");

    rig.wait_for_exit();
    rig.wait_for_text("line2999");

    assert_eq!(rig.wakes.load(Ordering::SeqCst), 1);
    assert!(rig.session.take_dirty());
    assert!(!rig.session.take_dirty(), "taken once");
}

#[test]
fn taking_the_change_lets_the_next_one_wake_the_app_again() {
    let rig = start("printf one; sleep 0.4; printf two");
    rig.wait_for_text("one");
    assert_eq!(rig.wakes.load(Ordering::SeqCst), 1);

    assert!(rig.session.take_dirty());
    rig.wait_for_text("onetwo");

    assert_eq!(rig.wakes.load(Ordering::SeqCst), 2);
}

#[test]
fn the_app_is_woken_when_the_shell_ends_even_with_no_output() {
    let rig = start("exit 0");

    rig.wait_for_exit();
    rig.wait_until("a wake-up", |rig| rig.wakes.load(Ordering::SeqCst) > 0);
}

#[test]
fn a_write_to_a_shell_that_is_not_reading_gives_up_instead_of_blocking() {
    // Raw mode, as a shell's line editor sets it: a terminal in line mode throws
    // away what does not fit, and only a raw one makes the writer wait.
    let rig = start("stty raw -echo; sleep 30");
    thread::sleep(Duration::from_millis(300));
    let paste = vec![b'x'; 5 * 1024 * 1024];

    let start = Instant::now();
    let written = rig.session.write(&paste);

    assert!(!written.complete);
    assert!(written.sent > 0, "the first part fits");
    assert!(written.sent < 1024 * 1024, "{written:?}");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "took {:?}",
        start.elapsed()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn dropping_the_session_stops_the_shell() {
    let rig = start("sleep 30");
    let process = std::path::PathBuf::from(format!("/proc/{}", rig.session.process_id().unwrap()));
    assert!(process.exists());

    drop(rig);

    assert!(!process.exists(), "the shell is gone, and reaped");
}
