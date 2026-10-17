use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::{LaunchSpec, Manager, Notice, ServerKey};

use crate::test_support::{FakeLauncher, PATIENCE, dies, stays};

fn key() -> ServerKey {
    ServerKey {
        name: "rust".to_owned(),
        root: PathBuf::from("/p"),
    }
}

fn spec() -> LaunchSpec {
    LaunchSpec {
        name: "rust".to_owned(),
        command: "rust-analyzer".to_owned(),
        args: Vec::new(),
        root: std::env::temp_dir(),
        initialization_options: Value::Null,
    }
}

struct Rig {
    manager: Manager,
    launcher: Arc<FakeLauncher>,
    woken: Arc<AtomicUsize>,
    notices: Vec<Notice>,
}

fn rig(launcher: Arc<FakeLauncher>) -> Rig {
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&woken);
    let manager = Manager::new(Arc::clone(&launcher) as _, move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    Rig {
        manager,
        launcher,
        woken,
        notices: Vec::new(),
    }
}

impl Rig {
    fn ensure(&mut self) {
        let notices = self.manager.ensure(&key(), spec());
        self.notices.extend(notices);
    }

    /// Polls until `done` holds of the status, or the patience runs out.
    fn wait_for_status(&mut self, done: impl Fn(Option<&str>) -> bool) {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let notices = self.manager.poll();
            self.notices.extend(notices);
            let status = self.manager.status(&key());
            if done(status.as_deref()) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "status never became what was wanted; it is {status:?}, notices {:?}",
                self.notices
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn errors(&self) -> Vec<&Notice> {
        self.notices.iter().filter(|notice| notice.error).collect()
    }
}

#[test]
fn a_file_that_wants_a_server_starts_it_in_the_background_and_it_runs() {
    let mut rig = rig(FakeLauncher::new(|_| stays()));

    rig.ensure();
    assert_eq!(
        rig.manager.status(&key()).as_deref(),
        Some("rust-analyzer: starting")
    );
    rig.wait_for_status(|status| status == Some("rust-analyzer"));

    assert_eq!(rig.launcher.launched(), 1);
    assert!(rig.notices.is_empty(), "{:?}", rig.notices);
    assert!(
        rig.woken.load(Ordering::SeqCst) > 0,
        "the app was told the start finished"
    );
}

#[test]
fn asking_again_for_a_running_server_starts_nothing() {
    let mut rig = rig(FakeLauncher::new(|_| stays()));
    rig.ensure();
    rig.wait_for_status(|status| status == Some("rust-analyzer"));

    rig.ensure();
    rig.ensure();

    assert_eq!(rig.launcher.launched(), 1);
}

#[test]
fn a_program_that_cannot_be_run_is_one_error_and_never_tried_again() {
    let mut rig = rig(FakeLauncher::new(|_| None));

    rig.ensure();
    rig.wait_for_status(|status| status == Some("rust-analyzer: off"));
    rig.ensure();
    rig.ensure();
    std::thread::sleep(Duration::from_millis(50));
    let more = rig.manager.poll();

    assert_eq!(rig.launcher.launched(), 1);
    assert_eq!(rig.errors().len(), 1, "{:?}", rig.notices);
    assert!(
        rig.errors()[0].text.contains("was not found"),
        "{:?}",
        rig.notices
    );
    assert!(more.is_empty());
}

#[test]
fn a_server_that_dies_is_restarted_and_the_user_hears_about_it() {
    // The first run dies at once, the second stays.
    let mut rig = rig(FakeLauncher::new(|number| {
        if number == 0 { dies() } else { stays() }
    }));

    rig.ensure();
    let deadline = Instant::now() + PATIENCE;
    while rig.launcher.launched() < 2
        || rig.manager.status(&key()).as_deref() != Some("rust-analyzer")
    {
        let notices = rig.manager.poll();
        rig.notices.extend(notices);
        assert!(Instant::now() < deadline, "no restart: {:?}", rig.notices);
        std::thread::sleep(Duration::from_millis(2));
    }

    assert_eq!(rig.launcher.launched(), 2);
    let restarts: Vec<_> = rig
        .notices
        .iter()
        .filter(|notice| notice.text.contains("restarting it (1 of 3)"))
        .collect();
    assert_eq!(restarts.len(), 1, "{:?}", rig.notices);
    assert!(!restarts[0].error);
    assert!(rig.errors().is_empty());
}

#[test]
fn a_server_that_keeps_dying_is_restarted_three_times_and_then_stays_off() {
    let mut rig = rig(FakeLauncher::new(|_| dies()));

    rig.ensure();
    rig.wait_for_status(|status| status == Some("rust-analyzer: off"));

    assert_eq!(
        rig.launcher.launched(),
        1 + 3,
        "the first start and three restarts"
    );
    assert_eq!(rig.errors().len(), 1, "{:?}", rig.notices);
    assert!(
        rig.errors()[0].text.contains("stays off"),
        "{:?}",
        rig.notices
    );
    let restart_notices = rig
        .notices
        .iter()
        .filter(|notice| notice.text.contains("restarting it"))
        .count();
    assert_eq!(restart_notices, 3);
    std::thread::sleep(Duration::from_millis(50));
    rig.manager.poll();
    assert_eq!(rig.launcher.launched(), 4, "nothing more is started");
}

#[test]
fn what_the_server_reports_as_progress_reaches_the_status() {
    let launcher = FakeLauncher::new(|_| {
        Some(Box::new(|server| {
            server.notify("$/progress", json!({
                "token": "t",
                "value": { "kind": "begin", "title": "Indexing", "message": "1/2", "percentage": 50 },
            }));
            while server.next().is_some() {}
        }))
    });
    let mut rig = rig(launcher);

    rig.ensure();
    rig.wait_for_status(|status| status == Some("rust-analyzer: Indexing: 1/2 50%"));
}

#[test]
fn a_reset_ends_the_servers_and_lets_the_next_file_start_them_again() {
    let mut rig = rig(FakeLauncher::new(|_| stays()));
    rig.ensure();
    rig.wait_for_status(|status| status == Some("rust-analyzer"));

    rig.manager.reset();
    assert_eq!(rig.manager.status(&key()), None);
    rig.ensure();
    rig.wait_for_status(|status| status == Some("rust-analyzer"));

    assert_eq!(rig.launcher.launched(), 2);
}

#[test]
fn a_start_that_finishes_after_a_reset_is_thrown_away() {
    let mut rig = rig(FakeLauncher::new(|_| stays()));
    rig.ensure();

    rig.manager.reset();
    // Let the start that was under way finish.
    let deadline = Instant::now() + PATIENCE;
    while rig.launcher.launched() < 1 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    std::thread::sleep(Duration::from_millis(100));
    rig.manager.poll();

    assert_eq!(rig.manager.status(&key()), None, "nobody wants it");
}
